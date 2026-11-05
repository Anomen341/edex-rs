//! Finds working-directory reports in a shell's output. The terminal emulator
//! ignores these sequences, so the raw bytes are scanned before it sees them.
//!
//! Understands OSC 7 (`file://host/path`, the common form) and OSC 9;9 (the
//! form Windows Terminal uses). A sequence may be split across reads.

use std::path::PathBuf;

const ESC: u8 = 0x1B;
const BEL: u8 = 0x07;
const MAX_LENGTH: usize = 4096;

#[derive(Clone, Copy, PartialEq, Eq)]
enum State {
    Ground,
    Escape,
    Osc,
    OscEscape,
}

pub struct OscScanner {
    state: State,
    buffer: Vec<u8>,
}

impl OscScanner {
    pub fn new() -> Self {
        Self { state: State::Ground, buffer: Vec::new() }
    }

    /// Scan a chunk of output, returning the newest directory reported in it.
    pub fn feed(&mut self, bytes: &[u8]) -> Option<PathBuf> {
        let mut found = None;
        for byte in bytes {
            self.state = match (self.state, *byte) {
                (State::Ground, ESC) => State::Escape,
                (State::Ground, _) => State::Ground,
                (State::Escape, b']') => {
                    self.buffer.clear();
                    State::Osc
                }
                (State::Escape, ESC) => State::Escape,
                (State::Escape, _) => State::Ground,
                (State::Osc, BEL) => {
                    found = parse_directory(&self.buffer).or(found);
                    State::Ground
                }
                (State::Osc, ESC) => State::OscEscape,
                (State::Osc, value) => {
                    if self.buffer.len() < MAX_LENGTH {
                        self.buffer.push(value);
                    }
                    State::Osc
                }
                (State::OscEscape, b'\\') => {
                    found = parse_directory(&self.buffer).or(found);
                    State::Ground
                }
                (State::OscEscape, b']') => {
                    self.buffer.clear();
                    State::Osc
                }
                (State::OscEscape, _) => State::Ground,
            };
        }
        found
    }
}

fn parse_directory(body: &[u8]) -> Option<PathBuf> {
    let text = String::from_utf8_lossy(body);
    if let Some(uri) = text.strip_prefix("7;") {
        return parse_file_uri(uri);
    }
    let path = text.strip_prefix("9;9;")?.trim_matches('"');
    (!path.is_empty()).then(|| PathBuf::from(path))
}

/// `file://host/some/path`, percent-encoded.
fn parse_file_uri(uri: &str) -> Option<PathBuf> {
    let rest = uri.strip_prefix("file://")?;
    let path = &rest[rest.find('/')?..];
    let decoded = percent_decode(path);
    // A Windows path arrives as /C:/Users, which needs its leading slash gone.
    let bytes = decoded.as_bytes();
    let drive = bytes.len() > 2 && bytes[0] == b'/' && bytes[1].is_ascii_alphabetic() && bytes[2] == b':';
    Some(PathBuf::from(if drive { &decoded[1..] } else { decoded.as_str() }))
}

fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let hex = |byte: u8| char::from(byte).to_digit(16);
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let escaped = (bytes[i] == b'%' && i + 2 < bytes.len())
            .then(|| Some(hex(bytes[i + 1])? * 16 + hex(bytes[i + 2])?))
            .flatten();
        match escaped {
            Some(value) => {
                out.push(value as u8);
                i += 3;
            }
            None => {
                out.push(bytes[i]);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scan(bytes: &[u8]) -> Option<PathBuf> {
        OscScanner::new().feed(bytes)
    }

    #[test]
    fn reads_osc_7_with_either_terminator() {
        assert_eq!(scan(b"\x1b]7;file://box/home/me\x07"), Some("/home/me".into()));
        assert_eq!(scan(b"\x1b]7;file://box/home/me\x1b\\"), Some("/home/me".into()));
        assert_eq!(scan(b"\x1b]7;file:///tmp\x07"), Some("/tmp".into()));
    }

    #[test]
    fn decodes_spaces_and_unicode() {
        assert_eq!(scan(b"\x1b]7;file://box/my%20dir/caf%C3%A9\x07"), Some("/my dir/café".into()));
    }

    #[test]
    fn bad_percent_sequences_stay_literal() {
        assert_eq!(scan(b"\x1b]7;file://box/100%\x07"), Some("/100%".into()));
        assert_eq!(scan(b"\x1b]7;file://box/a%zzb\x07"), Some("/a%zzb".into()));
    }

    #[test]
    fn reads_windows_forms() {
        assert_eq!(scan(b"\x1b]7;file://PC/C:/Users/me\x07"), Some("C:/Users/me".into()));
        assert_eq!(scan(b"\x1b]9;9;\"C:\\Users\\me\"\x1b\\"), Some("C:\\Users\\me".into()));
        assert_eq!(scan(b"\x1b]9;9;C:\\Temp\x07"), Some("C:\\Temp".into()));
    }

    #[test]
    fn a_sequence_split_across_reads_still_counts() {
        let mut scanner = OscScanner::new();
        assert_eq!(scanner.feed(b"text\x1b]7;fi"), None);
        assert_eq!(scanner.feed(b"le://h/ho"), None);
        assert_eq!(scanner.feed(b"me\x1b"), None);
        assert_eq!(scanner.feed(b"\\more"), Some("/home".into()));
    }

    #[test]
    fn the_newest_directory_in_a_chunk_wins() {
        assert_eq!(scan(b"\x1b]7;file://h/a\x07 \x1b]7;file://h/b\x07"), Some("/b".into()));
    }

    #[test]
    fn other_sequences_and_noise_are_ignored() {
        assert_eq!(scan(b"\x1b]0;window title\x07"), None);
        assert_eq!(scan(b"\x1b[31mred\x1b[0m plain"), None);
        assert_eq!(scan(b"\x1b]7;not-a-uri\x07"), None);
        assert_eq!(scan(b"\x1b]7;file://hostonly\x07"), None);
        assert_eq!(scan(&[0xFF; 100]), None);
    }

    #[test]
    fn an_endless_sequence_cannot_use_unbounded_memory() {
        let mut scanner = OscScanner::new();
        scanner.feed(b"\x1b]7;");
        scanner.feed(&vec![b'a'; 1_000_000]);
        assert!(scanner.buffer.len() <= MAX_LENGTH);
    }
}
