# edex-rs

A native sci-fi terminal and system monitor for Windows 11 and Debian, written
in Rust. Its look is inspired by eDEX-UI, but it is a fresh implementation with
none of that project's code, fonts or sounds.

The terminal is the point: it runs your real shell, with colours, full-screen
programs, copy and paste, scrollback and tabs. Everything around it is a live
view of your machine, and nothing depends on a web service staying online.

## What's on screen

- **Left:** clock, system, hardware, per-core CPU graphs, memory and swap, top processes.
- **Centre:** up to five terminal tabs.
- **Right:** network status, a rotating globe with your connection count, and a traffic graph.
- **Bottom:** a file browser that follows your shell, and a keyboard that lights up as you type.

Anything that can't be read shows `N/A` or `OFFLINE`. It never stops the app.

## Build and run

You need a current stable Rust from [rustup](https://rustup.rs). Rust from
`apt` is too old for these dependencies.

### Debian 12 and 13

```sh
sudo apt install build-essential pkg-config
cargo build --release
./target/release/edex-rs
```

Build on the Debian version you plan to run it on. A binary built on Debian 13
needs a newer glibc than Debian 12 has, so it won't start there.

The app needs a desktop session and a graphics driver. With no working GPU it
falls back to OpenGL, then to software rendering, where it turns off smooth
graph scrolling to save CPU.

### Windows 11

Install Rust with rustup and the Visual Studio C++ Build Tools, then:

```powershell
cargo build --release
.\target\release\edex-rs.exe
```

The `.exe` runs on its own. It needs no installer, runtime or admin rights, and
uses only DLLs that ship with Windows. It isn't code-signed, so Windows may show
a SmartScreen warning the first time. Choose **More info**, then **Run anyway**.

To build the Windows version from Debian instead:

```sh
sudo apt install gcc-mingw-w64-x86-64
rustup target add x86_64-pc-windows-gnu
cargo build --release --target x86_64-pc-windows-gnu
```

## Keys and mouse

| Action | Debian | Windows |
|---|---|---|
| Copy | Ctrl+Shift+C | Ctrl+Shift+C, or Ctrl+C with text selected |
| Paste | Ctrl+Shift+V, Shift+Insert, right or middle click | The same, plus Ctrl+V |
| Select | Drag; double-click a word; triple-click a line | The same |
| Select inside a mouse-aware program | Hold Shift while dragging | The same |
| Scroll back | Mouse wheel, Shift+PgUp, Shift+PgDn, Shift+Home, Shift+End | The same |
| New tab | Ctrl+Shift+T, or click an EMPTY tab | The same |
| Close tab | Ctrl+Shift+W, or click its x | The same |
| Next or previous tab | Ctrl+Tab, Ctrl+Shift+Tab | The same |
| Jump to tab | Ctrl+Shift+1 to 5 | The same |
| Fullscreen | F11 | The same |
| Skip the startup sequence | Any key or click | The same |

In the file browser, click a folder to change to it, and click a file to type
its path at the prompt.

## Settings

The first run writes a commented `config.toml`:

- **Debian:** `~/.config/edex-rs/config.toml`
- **Windows:** `%APPDATA%\edex-rs\config\config.toml`

It sets the theme (`tron`, `amber` or `green`), font sizes, shell, scrollback,
12-hour clock, startup sequence, latency host, external IP lookup, and which
panels to show. Restart the app after changing it.

If the file has a mistake, the app starts with the default settings and shows
what's wrong above the terminal. Delete the file to get a fresh copy.

## File browser and your shell

This works with no setup for PowerShell, and for any shell on Debian. For other
cases, see [docs/shell-integration.md](docs/shell-integration.md).

## Connections on the globe

The globe always shows your connection count. To also plot where connections
go, add a free GeoLite2 City database. Nothing is downloaded for you.

1. Create a free account at [MaxMind](https://www.maxmind.com/en/geolite2/signup).
2. Download **GeoLite2 City** in `.mmdb` format.
3. Save it as `GeoLite2-City.mmdb` next to `config.toml`.
4. Restart the app.

Lookups happen on your machine, so your connections aren't sent anywhere.

## What it sends over the network

- A TCP connect to `latency_host` every 5 seconds, to measure latency.
- One plain HTTP request to `external_ip_url` about every 10 minutes, to show
  your public IP. Set it to `""` to turn this off.

Both are optional and fail quietly. Your public IP appears on screen, so check
before sharing a screenshot.

## Known limits

- CPU temperature usually reads `N/A` on Windows without admin rights.
- Windows reports the base clock as the maximum, so `MAX` rises to the highest speed seen.
- Input methods for Chinese, Japanese and Korean aren't enabled, though those characters display.
- Nerd Font icons aren't bundled, so some fancy prompts show blank boxes.
- The cursor doesn't blink, though its shape follows what programs ask for.
- The keyboard is display only, in a US-style layout. Left and right modifiers light up together.
- `cmd.exe` can't be followed by the file browser without help.
- There are no sound effects.

## Development

```sh
cargo test
```

| Folder | Holds |
|---|---|
| `src/ui/` | All drawing |
| `src/term/` | PTY sessions, tabs and terminal input |
| `src/stats/` | System statistics, with small per-platform files |
| `src/net/` | Network status, traffic and connections |
| `src/files.rs` | Background directory reading |
| `src/theme.rs` | Every colour |
| `src/config.rs` | Settings |

The UI thread never waits on I/O. PTYs, statistics, network checks and directory
reads all run on background threads.

## Credits

Third-party fonts and data are listed in [assets/NOTICE.md](assets/NOTICE.md).
