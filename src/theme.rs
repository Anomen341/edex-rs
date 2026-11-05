//! Every colour the UI uses lives here. A new theme is one more constructor.

use egui::Color32;

#[derive(Clone, Debug)]
pub struct Theme {
    /// Window background.
    pub background: Color32,
    /// Fill behind panels, a step above the background.
    pub panel: Color32,
    /// Primary text and bright lines.
    pub foreground: Color32,
    /// Secondary text.
    pub muted: Color32,
    /// Thin panel outlines.
    pub line: Color32,
    /// Halo drawn around outlines. Keep its alpha low.
    pub glow: Color32,
    pub terminal: TerminalColors,
}

/// Colours inside the terminal. The ANSI set keeps real hues, because programs
/// use them for meaning: red has to stay readable as red in `htop` and `git`.
#[derive(Clone, Debug)]
pub struct TerminalColors {
    /// The 16 ANSI colours: 8 normal, then 8 bright.
    pub ansi: [Color32; 16],
    pub foreground: Color32,
    pub background: Color32,
    pub cursor: Color32,
    pub selection: Color32,
}

impl TerminalColors {
    /// Look up a colour by xterm number: 0-15 ANSI, 16-231 the 6x6x6 cube,
    /// 232-255 the grey ramp, then 256 foreground, 257 background, 258 cursor.
    pub fn by_index(&self, index: usize) -> Color32 {
        match index {
            0..=15 => self.ansi[index],
            16..=231 => {
                let level = |v: usize| if v == 0 { 0 } else { (55 + v * 40) as u8 };
                let i = index - 16;
                Color32::from_rgb(level(i / 36), level(i / 6 % 6), level(i % 6))
            }
            232..=255 => {
                let grey = (8 + (index - 232) * 10) as u8;
                Color32::from_rgb(grey, grey, grey)
            }
            257 => self.background,
            258 => self.cursor,
            _ => self.foreground,
        }
    }
}

impl Theme {
    pub fn tron() -> Self {
        Self {
            background: Color32::from_rgb(0x05, 0x08, 0x0B),
            panel: Color32::from_rgb(0x09, 0x0F, 0x13),
            foreground: Color32::from_rgb(0xAA, 0xCF, 0xD1),
            muted: Color32::from_rgb(0x6F, 0x92, 0x96),
            line: Color32::from_rgb(0x3F, 0x5F, 0x64),
            glow: Color32::from_rgba_unmultiplied(0xAA, 0xCF, 0xD1, 16),
            terminal: TerminalColors {
                ansi: [
                    Color32::from_rgb(0x1B, 0x26, 0x2B),
                    Color32::from_rgb(0xE0, 0x6C, 0x75),
                    Color32::from_rgb(0x7F, 0xC8, 0x8E),
                    Color32::from_rgb(0xE0, 0xC0, 0x7A),
                    Color32::from_rgb(0x6C, 0xA8, 0xE0),
                    Color32::from_rgb(0xC6, 0x8F, 0xD8),
                    Color32::from_rgb(0x5F, 0xC8, 0xCE),
                    Color32::from_rgb(0xC4, 0xD8, 0xDA),
                    Color32::from_rgb(0x5C, 0x72, 0x78),
                    Color32::from_rgb(0xFF, 0x8A, 0x92),
                    Color32::from_rgb(0xA2, 0xE8, 0xB0),
                    Color32::from_rgb(0xF5, 0xD9, 0x97),
                    Color32::from_rgb(0x93, 0xC5, 0xF5),
                    Color32::from_rgb(0xE0, 0xAD, 0xF0),
                    Color32::from_rgb(0x8C, 0xE5, 0xEA),
                    Color32::from_rgb(0xF0, 0xF8, 0xF8),
                ],
                foreground: Color32::from_rgb(0xAA, 0xCF, 0xD1),
                background: Color32::from_rgb(0x09, 0x0F, 0x13),
                cursor: Color32::from_rgb(0xAA, 0xCF, 0xD1),
                selection: Color32::from_rgb(0x2F, 0x52, 0x5A),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xterm_colour_table_matches_the_standard_values() {
        let colours = Theme::tron().terminal;
        assert_eq!(colours.by_index(16), Color32::from_rgb(0, 0, 0));
        assert_eq!(colours.by_index(196), Color32::from_rgb(255, 0, 0));
        assert_eq!(colours.by_index(231), Color32::from_rgb(255, 255, 255));
        assert_eq!(colours.by_index(232), Color32::from_rgb(8, 8, 8));
        assert_eq!(colours.by_index(255), Color32::from_rgb(238, 238, 238));
        assert_eq!(colours.by_index(257), colours.background);
    }
}
