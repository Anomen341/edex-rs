//! The only part of the terminal that differs between Windows and Debian.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use portable_pty::CommandBuilder;

/// PowerShell users expect Ctrl+V to paste. On Debian it stays a key the
/// program can use (it starts block selection in `vim`).
pub const CTRL_V_PASTES: bool = cfg!(windows);
/// Matches Windows Terminal: Ctrl+C copies if text is selected, else interrupts.
pub const CTRL_C_COPIES_SELECTION: bool = cfg!(windows);

/// The command for a new tab, with the environment programs look for.
pub fn shell_command(custom: Option<&[String]>) -> CommandBuilder {
    let argv: Vec<OsString> = match custom {
        Some(argv) if !argv.is_empty() => argv.iter().map(OsString::from).collect(),
        _ => default_shell(),
    };
    let mut command = CommandBuilder::from_argv(argv);
    command.env("TERM", "xterm-256color");
    command.env("COLORTERM", "truecolor");
    command.env("TERM_PROGRAM", "edex-rs");
    if let Some(home) = std::env::home_dir() {
        command.cwd(home);
    }
    command
}

#[cfg(windows)]
fn default_shell() -> Vec<OsString> {
    let path = std::env::var_os("PATH").unwrap_or_default();
    let program = if find_on_path("pwsh.exe", &path).is_some() { "pwsh.exe" } else { "powershell.exe" };
    vec![program.into(), "-NoLogo".into()]
}

#[cfg(unix)]
fn default_shell() -> Vec<OsString> {
    let path = std::env::var_os("PATH").unwrap_or_default();
    let from_env = std::env::var_os("SHELL").filter(|shell| !shell.is_empty());
    let program = from_env
        .into_iter()
        .chain(["/bin/bash".into(), "/bin/sh".into()])
        .find(|shell| is_runnable(shell.as_ref(), &path))
        .unwrap_or_else(|| "/bin/sh".into());
    vec![program, "-l".into()]
}

#[cfg(unix)]
fn is_runnable(program: &Path, path: &std::ffi::OsStr) -> bool {
    if program.is_absolute() {
        program.is_file()
    } else {
        find_on_path(program, path).is_some()
    }
}

fn find_on_path(program: impl AsRef<Path>, path: &std::ffi::OsStr) -> Option<PathBuf> {
    std::env::split_paths(path)
        .map(|dir| dir.join(program.as_ref()))
        .find(|candidate| candidate.is_file())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_search_finds_only_real_files() {
        let dir = std::env::temp_dir().join(format!("edex-path-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("some-shell"), "").unwrap();
        let path = std::env::join_paths([&dir]).unwrap();
        assert_eq!(find_on_path("some-shell", &path), Some(dir.join("some-shell")));
        assert_eq!(find_on_path("missing-shell", &path), None);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_custom_command_replaces_the_default_shell() {
        let custom = ["/bin/zsh".to_owned(), "-f".to_owned()];
        let command = shell_command(Some(&custom));
        assert_eq!(command.get_argv(), &vec![OsString::from("/bin/zsh"), OsString::from("-f")]);
        assert_eq!(command.get_env("TERM").unwrap(), "xterm-256color");
    }
}
