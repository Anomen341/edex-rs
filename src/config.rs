//! User settings. Defaults only for now; the TOML file arrives in phase 6.

/// Text sizes in points. egui multiplies points by the display's scale factor,
/// so these stay the same physical size on high-DPI screens.
#[derive(Clone, Debug)]
pub struct FontSizes {
    /// Uppercase panel labels.
    pub label: f32,
    /// Values and lists inside panels.
    pub body: f32,
    /// The large clock.
    pub clock: f32,
    /// Text in the terminal.
    pub terminal: f32,
}

#[derive(Clone, Debug)]
pub struct Config {
    pub fonts: FontSizes,
    /// Program and arguments to run in each tab. `None` uses the platform's
    /// default shell.
    pub shell: Option<Vec<String>>,
    /// Lines of history kept per tab.
    pub scrollback: usize,
    /// Show the clock as 12-hour with AM or PM instead of 24-hour.
    pub twelve_hour_clock: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            fonts: FontSizes {
                label: 15.0,
                body: 17.0,
                clock: 52.0,
                terminal: 16.0,
            },
            shell: None,
            scrollback: 10_000,
            twelve_hour_clock: false,
        }
    }
}
