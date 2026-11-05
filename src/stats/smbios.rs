//! Reads the system and chassis records from a raw SMBIOS table. Firmware data
//! is untrusted input, so every offset is checked.

use super::{chassis_name, clean_field, Hardware};

const SYSTEM_RECORD: u8 = 1;
const CHASSIS_RECORD: u8 = 3;
const END_RECORD: u8 = 127;
const HEADER: usize = 4;

/// One record's formatted area and its trailing string table.
struct Record<'a> {
    kind: u8,
    formatted: &'a [u8],
    strings: &'a [u8],
}

impl Record<'_> {
    /// The string a byte at `offset` refers to. Numbering starts at 1.
    fn string_at(&self, offset: usize) -> Option<String> {
        let index = usize::from(*self.formatted.get(offset)?);
        let text = self.strings.split(|byte| *byte == 0).nth(index.checked_sub(1)?)?;
        clean_field(&String::from_utf8_lossy(text))
    }
}

fn next_record(table: &[u8]) -> Option<(Record<'_>, &[u8])> {
    let kind = *table.first()?;
    let length = usize::from(*table.get(1)?);
    if length < HEADER || length > table.len() {
        return None;
    }
    let (formatted, rest) = table.split_at(length);
    // Strings end at the first double NUL.
    let end = rest.windows(2).position(|pair| pair == [0, 0])?;
    let (strings, remainder) = rest.split_at(end + 2);
    Some((Record { kind, formatted, strings }, remainder))
}

pub fn parse(mut table: &[u8]) -> Hardware {
    let mut hardware = Hardware::default();
    while let Some((record, rest)) = next_record(table) {
        match record.kind {
            SYSTEM_RECORD => {
                hardware.manufacturer = record.string_at(4);
                hardware.model = record.string_at(5);
            }
            CHASSIS_RECORD => {
                hardware.chassis = record.formatted.get(5).copied().and_then(chassis_name);
            }
            END_RECORD => break,
            _ => {}
        }
        table = rest;
    }
    hardware
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(kind: u8, body: &[u8], strings: &[&str]) -> Vec<u8> {
        let mut out = vec![kind, (HEADER + body.len()) as u8, 0, 0];
        out.extend_from_slice(body);
        for text in strings {
            out.extend_from_slice(text.as_bytes());
            out.push(0);
        }
        if strings.is_empty() {
            out.push(0);
        }
        out.push(0);
        out
    }

    fn table() -> Vec<u8> {
        let mut table = record(0, &[1, 2], &["BIOS Vendor", "1.0"]);
        table.extend(record(SYSTEM_RECORD, &[1, 2, 0, 0], &["Dell Inc.", "XPS 15"]));
        table.extend(record(CHASSIS_RECORD, &[1, 0x8A, 0, 0], &["Dell Inc."]));
        table.extend(record(END_RECORD, &[], &[]));
        table
    }

    #[test]
    fn reads_system_and_chassis_records() {
        let hardware = parse(&table());
        assert_eq!(hardware.manufacturer.as_deref(), Some("Dell Inc."));
        assert_eq!(hardware.model.as_deref(), Some("XPS 15"));
        assert_eq!(hardware.chassis.as_deref(), Some("Notebook"));
    }

    #[test]
    fn filler_and_empty_references_are_missing() {
        let table = record(SYSTEM_RECORD, &[1, 0, 0, 0], &["Default string"]);
        assert_eq!(parse(&table), Hardware::default());
    }

    #[test]
    fn a_reference_past_the_strings_is_missing() {
        let table = record(SYSTEM_RECORD, &[9, 9, 0, 0], &["Only one"]);
        assert_eq!(parse(&table), Hardware::default());
    }

    #[test]
    fn truncated_or_corrupt_tables_cannot_panic() {
        let whole = table();
        for end in 0..whole.len() {
            parse(&whole[..end]);
        }
        parse(&[1, 0, 0, 0]);
        parse(&[1, 255, 0, 0]);
        parse(&[0xFF; 64]);
        let mut state = 0x1234_5678u32;
        for _ in 0..2000 {
            let noise: Vec<u8> = (0..200)
                .map(|_| {
                    state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                    (state >> 24) as u8
                })
                .collect();
            parse(&noise);
        }
    }

    #[test]
    fn records_stop_at_the_end_marker() {
        let mut table = record(END_RECORD, &[], &[]);
        table.extend(record(SYSTEM_RECORD, &[1, 2, 0, 0], &["After", "End"]));
        assert_eq!(parse(&table), Hardware::default());
    }
}
