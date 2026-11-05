//! The only part of the terminal that differs between Windows and Debian.

use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use portable_pty::CommandBuilder;

/// PowerShell users expect Ctrl+V to paste. On Debian it stays a key the
/// program can use (it starts block selection in `vim`).
pub const CTRL_V_PASTES: bool = cfg!(windows);
/// Matches Windows Terminal: Ctrl+C copies if text is selected, else interrupts.
pub const CTRL_C_COPIES_SELECTION: bool = cfg!(windows);

/// Runs after the user's profile and wraps whatever prompt it set up, so themes
/// keep working. The file route exists because Windows' console layer may drop
/// the terminal sequences a shell would otherwise use to report its folder.
const POWERSHELL_HOOK: &str = r#"
$global:__edex_prompt = $function:prompt
function global:prompt {
    try {
        $here = Get-Location
        if ($here.Provider.Name -eq 'FileSystem') {
            [IO.File]::WriteAllText($env:EDEX_CWD_FILE, $here.ProviderPath)
        }
    } catch {}
    & $global:__edex_prompt
}
"#;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShellKind {
    Posix,
    PowerShell,
    Other,
}

/// Everything needed to start a shell and follow its working directory.
pub struct Launch {
    pub command: CommandBuilder,
    pub shell: ShellKind,
    /// Where a prompt hook writes the working directory, if one was added.
    pub cwd_file: Option<PathBuf>,
}

impl Launch {
    #[cfg(test)]
    pub fn plain(command: CommandBuilder) -> Self {
        Self { command, shell: ShellKind::Other, cwd_file: None }
    }
}

fn classify(program: &OsStr) -> ShellKind {
    let stem = Path::new(program).file_stem().map(|name| name.to_string_lossy().to_lowercase());
    match stem.as_deref() {
        Some("pwsh" | "powershell") => ShellKind::PowerShell,
        Some("bash" | "sh" | "zsh" | "fish" | "dash" | "ksh") => ShellKind::Posix,
        _ => ShellKind::Other,
    }
}

/// A file name unique to this tab, so two tabs or two copies of the app can't
/// read each other's folder.
fn new_cwd_file() -> PathBuf {
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let id = NEXT.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!("edex-rs-{}-{id}.cwd", std::process::id()))
}

/// PowerShell's `-EncodedCommand` takes base64 of UTF-16, which avoids every
/// quoting problem of passing a script on the command line.
fn encode_powershell(script: &str) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let bytes: Vec<u8> = script.encode_utf16().flat_map(u16::to_le_bytes).collect();
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let group = u32::from(chunk[0]) << 16
            | u32::from(*chunk.get(1).unwrap_or(&0)) << 8
            | u32::from(*chunk.get(2).unwrap_or(&0));
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(char::from(ALPHABET[(group >> (18 - 6 * i)) as usize & 63]));
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// Quote a path so the shell reads it as one word.
pub fn quote_path(shell: ShellKind, path: &Path) -> String {
    let text = path.to_string_lossy();
    match shell {
        ShellKind::Posix => format!("'{}'", text.replace('\'', "'\\''")),
        ShellKind::PowerShell => format!("'{}'", text.replace('\'', "''")),
        ShellKind::Other => format!("\"{text}\""),
    }
}

/// Bytes that clear the input line and change folder.
pub fn cd_command(shell: ShellKind, path: &Path) -> Vec<u8> {
    let quoted = quote_path(shell, path);
    match shell {
        // Ctrl+U clears a readline prompt.
        ShellKind::Posix => format!("\x15cd {quoted}\r"),
        // Escape clears a PSReadLine prompt in its default Windows mode.
        ShellKind::PowerShell if cfg!(windows) => format!("\x1bSet-Location -LiteralPath {quoted}\r"),
        ShellKind::PowerShell => format!("Set-Location -LiteralPath {quoted}\r"),
        ShellKind::Other if cfg!(windows) => format!("cd /d {quoted}\r"),
        ShellKind::Other => format!("cd {quoted}\r"),
    }
    .into_bytes()
}

/// The command for a new tab, with the environment programs look for.
pub fn shell_command(custom: Option<&[String]>) -> Launch {
    let is_custom = custom.is_some_and(|argv| !argv.is_empty());
    let mut argv: Vec<OsString> = match custom {
        Some(argv) if !argv.is_empty() => argv.iter().map(OsString::from).collect(),
        _ => default_shell(),
    };
    let shell = argv.first().map_or(ShellKind::Other, |program| classify(program));
    // Custom arguments are left alone unless it's a bare PowerShell.
    let add_hook = shell == ShellKind::PowerShell && (!is_custom || argv.len() == 1);
    if add_hook {
        argv.extend(["-NoExit".into(), "-EncodedCommand".into(), encode_powershell(POWERSHELL_HOOK).into()]);
    }
    let mut command = CommandBuilder::from_argv(argv);
    let cwd_file = add_hook.then(new_cwd_file);
    if let Some(file) = &cwd_file {
        command.env("EDEX_CWD_FILE", file);
    }
    command.env("TERM", "xterm-256color");
    command.env("COLORTERM", "truecolor");
    command.env("TERM_PROGRAM", "edex-rs");
    if let Some(home) = std::env::home_dir() {
        command.cwd(home);
    }
    Launch { command, shell, cwd_file }
}

/// The shell's current folder from the system, where that is possible.
#[cfg(target_os = "linux")]
pub fn process_cwd(pid: u32) -> Option<PathBuf> {
    let path = std::fs::read_link(format!("/proc/{pid}/cwd")).ok()?;
    Some(strip_deleted_mark(path))
}

/// The kernel adds this to a folder that has been removed.
#[cfg(target_os = "linux")]
fn strip_deleted_mark(path: PathBuf) -> PathBuf {
    let text = path.to_string_lossy();
    text.strip_suffix(" (deleted)").map_or_else(|| path.clone(), PathBuf::from)
}

/// Windows offers no safe way to read another process's folder.
#[cfg(not(target_os = "linux"))]
pub fn process_cwd(_pid: u32) -> Option<PathBuf> {
    None
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
        let launch = shell_command(Some(&custom));
        assert_eq!(launch.command.get_argv(), &vec![OsString::from("/bin/zsh"), OsString::from("-f")]);
        assert_eq!(launch.command.get_env("TERM").unwrap(), "xterm-256color");
        assert_eq!(launch.shell, ShellKind::Posix);
        assert!(launch.cwd_file.is_none());
    }

    #[test]
    fn shells_are_recognised_by_name_on_any_platform() {
        assert_eq!(classify("/usr/bin/pwsh".as_ref()), ShellKind::PowerShell);
        assert_eq!(classify("powershell.exe".as_ref()), ShellKind::PowerShell);
        assert_eq!(classify("PWSH.EXE".as_ref()), ShellKind::PowerShell);
        assert_eq!(classify("/bin/bash".as_ref()), ShellKind::Posix);
        assert_eq!(classify("cmd.exe".as_ref()), ShellKind::Other);
    }

    #[test]
    fn powershell_gets_the_hook_and_its_own_file() {
        let bare = ["pwsh".to_owned()];
        let one = shell_command(Some(&bare));
        let two = shell_command(Some(&bare));
        let argv = one.command.get_argv();
        assert_eq!(argv[1], "-NoExit");
        assert_eq!(argv[2], "-EncodedCommand");
        assert_eq!(argv[3], OsString::from(encode_powershell(POWERSHELL_HOOK)));
        assert_ne!(one.cwd_file, two.cwd_file);
        assert_eq!(one.command.get_env("EDEX_CWD_FILE").unwrap(), one.cwd_file.as_ref().unwrap().as_os_str());
    }

    #[test]
    fn powershell_with_custom_arguments_is_left_alone() {
        let custom = ["pwsh".to_owned(), "-NoProfile".to_owned()];
        let launch = shell_command(Some(&custom));
        assert_eq!(launch.command.get_argv().len(), 2);
        assert!(launch.cwd_file.is_none());
    }

    #[test]
    fn encoded_commands_match_powershell_format() {
        // Reference: [Convert]::ToBase64String([Text.Encoding]::Unicode.GetBytes('dir'))
        assert_eq!(encode_powershell("dir"), "ZABpAHIA");
        assert_eq!(encode_powershell("a"), "YQA=");
        assert_eq!(encode_powershell(""), "");
    }

    #[test]
    fn the_hook_only_uses_what_windows_powershell_5_has() {
        // These three all need PowerShell 7.
        for newer in ["`e", "??", "?."] {
            assert!(!POWERSHELL_HOOK.contains(newer), "{newer}");
        }
    }

    #[test]
    fn paths_are_quoted_for_each_shell() {
        let path = Path::new("/tmp/it's a dir");
        assert_eq!(quote_path(ShellKind::Posix, path), "'/tmp/it'\\''s a dir'");
        assert_eq!(quote_path(ShellKind::PowerShell, path), "'/tmp/it''s a dir'");
        assert_eq!(quote_path(ShellKind::Other, Path::new("C:/a b")), "\"C:/a b\"");
    }

    #[test]
    fn a_hostile_folder_name_cannot_run_commands() {
        let bytes = cd_command(ShellKind::Posix, Path::new("/tmp/x'; rm -rf ~"));
        assert_eq!(bytes, b"\x15cd '/tmp/x'\\''; rm -rf ~'\r");
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn a_removed_folder_loses_the_kernels_mark() {
        assert_eq!(strip_deleted_mark("/tmp/gone (deleted)".into()), PathBuf::from("/tmp/gone"));
        assert_eq!(strip_deleted_mark("/tmp/here".into()), PathBuf::from("/tmp/here"));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn a_process_folder_can_be_read() {
        assert_eq!(process_cwd(std::process::id()), std::env::current_dir().ok());
        assert_eq!(process_cwd(u32::MAX), None);
    }
}
