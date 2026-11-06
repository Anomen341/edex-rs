//! Every colour the UI uses lives here. A new theme is one more constructor.

use egui::Color32;

/// Status colours in the startup log. Every theme shares them: green has to
/// read as green, whatever the accent.
pub const STATUS_OK: Color32 = Color32::from_rgb(0x3D, 0xE0, 0x6B);
pub const STATUS_WARN: Color32 = Color32::from_rgb(0xF5, 0xC8, 0x42);
pub const STATUS_FAILED: Color32 = Color32::from_rgb(0xFF, 0x5C, 0x5C);
pub const STATUS_RARE: Color32 = Color32::from_rgb(0xE0, 0x7B, 0xFF);

#[derive(Clone, Debug)]
pub struct Theme {
    /// Window background.
    pub background: Color32,
    /// Primary text and bright lines.
    pub foreground: Color32,
    /// Secondary text.
    pub muted: Color32,
    /// Dim marks, such as unused dots and empty tracks.
    pub line: Color32,
    /// Panel rules and the terminal's outline.
    pub rule: Color32,
    /// The dots behind everything. Barely above the background.
    pub backdrop: Color32,
    /// Reference lines inside graphs: visible, but never competing with data.
    pub grid: Color32,
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

/// Mix `amount` of `other` into `colour`.
fn mix(colour: Color32, other: Color32, amount: f32) -> Color32 {
    let blend = |a: u8, b: u8| (f32::from(a) + (f32::from(b) - f32::from(a)) * amount) as u8;
    Color32::from_rgb(blend(colour.r(), other.r()), blend(colour.g(), other.g()), blend(colour.b(), other.b()))
}

impl Theme {
    /// The theme with this name, if there is one.
    pub fn named(name: &str) -> Option<Self> {
        match name.to_ascii_lowercase().as_str() {
            "tron" => Some(Self::tron()),
            "amber" => Some(Self::tinted(Color32::from_rgb(0xFF, 0xB8, 0x6B))),
            "green" => Some(Self::tinted(Color32::from_rgb(0x8F, 0xE6, 0xA0))),
            _ => None,
        }
    }

    /// The tron theme recoloured around another accent. The ANSI colours stay,
    /// because programs rely on them for meaning.
    fn tinted(accent: Color32) -> Self {
        let tron = Self::tron();
        let background = tron.background;
        let glow = Color32::from_rgba_unmultiplied(accent.r(), accent.g(), accent.b(), 16);
        Self {
            foreground: accent,
            muted: mix(accent, background, 0.32),
            line: mix(accent, background, 0.78),
            rule: mix(accent, background, 0.55),
            grid: mix(accent, background, 0.86),
            backdrop: mix(accent, background, 0.93),
            glow,
            terminal: TerminalColors {
                foreground: accent,
                cursor: accent,
                selection: mix(accent, background, 0.7),
                ..tron.terminal.clone()
            },
            ..tron
        }
    }

    pub fn tron() -> Self {
        Self {
            background: Color32::from_rgb(0x05, 0x08, 0x0D),
            foreground: Color32::from_rgb(0xAA, 0xCF, 0xD1),
            muted: Color32::from_rgb(0x6F, 0x92, 0x96),
            line: Color32::from_rgb(0x2A, 0x3D, 0x42),
            rule: Color32::from_rgb(0x57, 0x6C, 0x6F),
            backdrop: Color32::from_rgb(0x12, 0x1A, 0x20),
            grid: Color32::from_rgb(0x1F, 0x30, 0x36),
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
                background: Color32::from_rgb(0x05, 0x08, 0x0D),
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
    fn themes_are_found_by_name_in_any_case() {
        assert!(Theme::named("tron").is_some());
        assert!(Theme::named("AMBER").is_some());
        assert!(Theme::named("Green").is_some());
        assert!(Theme::named("nope").is_none());
    }

    #[test]
    fn a_tinted_theme_keeps_real_ansi_colours_and_readable_text() {
        let amber = Theme::named("amber").unwrap();
        assert_eq!(amber.terminal.ansi, Theme::tron().terminal.ansi);
        assert_ne!(amber.foreground, Theme::tron().foreground);
        let brightness = |c: Color32| u32::from(c.r()) + u32::from(c.g()) + u32::from(c.b());
        assert!(brightness(amber.muted) > brightness(amber.rule));
        assert!(brightness(amber.rule) > brightness(amber.background));
    }

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
