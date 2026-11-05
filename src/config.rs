//! User settings, from a TOML file in the platform's config folder.
//!
//! Bad settings never stop the app. A file that can't be read leaves the
//! defaults in place, with a notice on screen saying why.

use std::fs;
use std::path::PathBuf;

use directories::ProjectDirs;
use serde::Deserialize;

const FILE_NAME: &str = "config.toml";

/// Written on first run, so there is a documented file to edit.
const TEMPLATE: &str = r#"# edex-rs settings. Delete this file to get the defaults back.
# Restart the app after changing anything.

# tron, amber or green
theme = "tron"

# Play the startup sequence. Any key skips it.
boot_animation = true

# 24-hour clock unless this is true.
twelve_hour_clock = false

# Lines of history kept per tab.
scrollback = 10000

# The program each tab runs. Leave commented out for PowerShell on Windows and
# your $SHELL on Debian.
# shell = ["/usr/bin/zsh"]

# Latency is the time for a TCP connect to this host:port.
latency_host = "1.1.1.1:443"

# A plain HTTP address that replies with your public IP. "" turns this off, and
# the app then asks no outside service for it.
external_ip_url = "http://api.ipify.org/"

# Text sizes in points. These stay the same physical size on high-DPI screens.
[fonts]
label = 15.0
body = 17.0
clock = 52.0
terminal = 16.0

# Panels to show. The space from a hidden panel goes to its neighbours.
[panels]
clock = true
system = true
hardware = true
cpu = true
memory = true
processes = true
network = true
globe = true
traffic = true
files = true
keyboard = true
"#;

/// Text sizes in points. egui multiplies points by the display's scale factor,
/// so these stay the same physical size on high-DPI screens.
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
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

impl Default for FontSizes {
    fn default() -> Self {
        Self { label: 15.0, body: 17.0, clock: 52.0, terminal: 16.0 }
    }
}

/// Which panels are shown. The terminal always is.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct Panels {
    pub clock: bool,
    pub system: bool,
    pub hardware: bool,
    pub cpu: bool,
    pub memory: bool,
    pub processes: bool,
    pub network: bool,
    pub globe: bool,
    pub traffic: bool,
    pub files: bool,
    pub keyboard: bool,
}

impl Default for Panels {
    fn default() -> Self {
        Self {
            clock: true,
            system: true,
            hardware: true,
            cpu: true,
            memory: true,
            processes: true,
            network: true,
            globe: true,
            traffic: true,
            files: true,
            keyboard: true,
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub theme: String,
    pub boot_animation: bool,
    pub fonts: FontSizes,
    pub panels: Panels,
    /// Program and arguments to run in each tab. `None` uses the platform's
    /// default shell.
    pub shell: Option<Vec<String>>,
    /// Lines of history kept per tab.
    pub scrollback: usize,
    /// `host:port` that latency is measured against, by TCP connect.
    pub latency_host: String,
    /// A plain HTTP address that replies with your IP. Empty turns it off.
    pub external_ip_url: String,
    /// Show the clock as 12-hour with AM or PM instead of 24-hour.
    pub twelve_hour_clock: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            theme: "tron".to_owned(),
            boot_animation: true,
            fonts: FontSizes::default(),
            panels: Panels::default(),
            shell: None,
            scrollback: 10_000,
            latency_host: "1.1.1.1:443".to_owned(),
            external_ip_url: "http://api.ipify.org/".to_owned(),
            twelve_hour_clock: false,
        }
    }
}

impl Config {
    /// Read the config file, creating it on first run. The second value is a
    /// problem to show on screen, if there was one.
    pub fn load() -> (Self, Option<String>) {
        let Some(path) = config_dir().map(|dir| dir.join(FILE_NAME)) else {
            return (Self::default(), None);
        };
        match fs::read_to_string(&path) {
            Ok(text) => match parse(&text) {
                Ok(config) => (config, None),
                Err(problem) => (Self::default(), Some(format!("CONFIG IGNORED: {problem}"))),
            },
            Err(_) => {
                // Unable to write it just means no template. The app still runs.
                if let Some(dir) = path.parent() {
                    let _ = fs::create_dir_all(dir);
                }
                let _ = fs::write(&path, TEMPLATE);
                (Self::default(), None)
            }
        }
    }
}

pub fn config_dir() -> Option<PathBuf> {
    ProjectDirs::from("", "", "edex-rs").map(|dirs| dirs.config_dir().to_owned())
}

/// Read settings, keeping values in a range the layout can work with.
fn parse(text: &str) -> Result<Config, String> {
    let mut config: Config = toml::from_str(text).map_err(|error| describe(text, &error))?;
    config.fonts.label = config.fonts.label.clamp(9.0, 40.0);
    config.fonts.body = config.fonts.body.clamp(9.0, 40.0);
    config.fonts.clock = config.fonts.clock.clamp(16.0, 120.0);
    config.fonts.terminal = config.fonts.terminal.clamp(8.0, 48.0);
    config.scrollback = config.scrollback.min(200_000);
    if config.shell.as_ref().is_some_and(Vec::is_empty) {
        config.shell = None;
    }
    Ok(config)
}

/// One line saying where and what, for showing above the terminal.
fn describe(text: &str, error: &toml::de::Error) -> String {
    let message = error.message().lines().next().unwrap_or("INVALID SETTING").to_uppercase();
    match error.span() {
        Some(span) => {
            let line = text[..span.start.min(text.len())].matches('\n').count() + 1;
            format!("LINE {line}: {message}")
        }
        None => message,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_template_says_exactly_what_the_defaults_are() {
        assert_eq!(parse(TEMPLATE).unwrap(), Config::default());
    }

    #[test]
    fn an_empty_file_gives_the_defaults() {
        assert_eq!(parse("").unwrap(), Config::default());
    }

    #[test]
    fn a_partial_file_keeps_the_other_defaults() {
        let config = parse("theme = \"amber\"\n[fonts]\nterminal = 20.0\n[panels]\nglobe = false").unwrap();
        assert_eq!(config.theme, "amber");
        assert_eq!(config.fonts.terminal, 20.0);
        assert_eq!(config.fonts.label, 15.0);
        assert!(!config.panels.globe);
        assert!(config.panels.cpu);
    }

    #[test]
    fn a_custom_shell_is_read_and_an_empty_one_is_ignored() {
        assert_eq!(parse("shell = [\"zsh\", \"-l\"]").unwrap().shell.unwrap(), ["zsh", "-l"]);
        assert_eq!(parse("shell = []").unwrap().shell, None);
    }

    #[test]
    fn extreme_values_are_brought_into_range() {
        let config = parse("scrollback = 999999999\n[fonts]\nlabel = 1.0\nclock = 9000.0\nterminal = 0.0").unwrap();
        assert_eq!(config.fonts.label, 9.0);
        assert_eq!(config.fonts.clock, 120.0);
        assert_eq!(config.fonts.terminal, 8.0);
        assert_eq!(config.scrollback, 200_000);
    }

    #[test]
    fn mistakes_are_reported_rather_than_fatal() {
        assert!(parse("this is not toml").is_err());
        assert!(parse("scrollback = \"many\"").is_err());
        let typo = parse("thmee = \"amber\"").unwrap_err();
        assert!(typo.contains("THMEE"), "{typo}");
        let wrong_type = parse("\n\nscrollback = \"many\"").unwrap_err();
        assert!(wrong_type.starts_with("LINE 3:"), "{wrong_type}");
    }

    #[test]
    fn problems_fit_on_one_line() {
        let problem = parse("a = [").unwrap_err();
        assert!(!problem.contains('\n'));
    }
}
