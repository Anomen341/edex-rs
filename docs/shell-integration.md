# Keeping the file browser in sync with your shell

The file browser follows the active terminal's current folder.

## It works with no setup

- **Debian, any shell:** edex-rs reads the shell's folder from `/proc`.
- **PowerShell on Windows or Debian:** edex-rs adds a small prompt hook when it
  starts the shell. It runs after your profile and keeps your prompt, including
  themes such as oh-my-posh. It only writes the current folder to a private
  temporary file that edex-rs reads, and the file is removed when the tab closes.

## When you need the hooks below

Use these when the shell reports its folder to the terminal itself, such as in
a shell you started by hand inside a tab, or a custom shell in your config.

### bash

Add to `~/.bashrc`:

```bash
__edex_osc7() { printf '\033]7;file://%s%s\a' "$HOSTNAME" "$PWD"; }
PROMPT_COMMAND="__edex_osc7${PROMPT_COMMAND:+;$PROMPT_COMMAND}"
```

### PowerShell

Add to the file named by `$PROFILE`:

```powershell
$__edexPrompt = $function:prompt
function global:prompt {
    $here = Get-Location
    if ($here.Provider.Name -eq 'FileSystem') {
        [Console]::Write("$([char]27)]9;9;`"$($here.ProviderPath)`"$([char]27)\")
    }
    & $__edexPrompt
}
```

### cmd.exe

edex-rs can't track `cmd.exe` without help. The panel says so instead of showing
the wrong folder. This prompt setting may work, depending on your Windows build:

```bat
prompt $e]9;9;$P$e\$P$G
```

## How it works

edex-rs recognises two standard terminal sequences: OSC 7
(`ESC ] 7 ; file://host/path BEL`) and OSC 9;9 (`ESC ] 9 ; 9 ; "path" ESC \`).
Whichever source changed most recently wins. Windows' console layer may not pass
every sequence through, which is why PowerShell also uses the temporary file.
