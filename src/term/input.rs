//! Turns keys, pastes and mouse actions into the bytes a terminal program
//! expects. Pure functions, so every sequence is unit tested.

use alacritty_terminal::term::TermMode;
use egui::{Key, Modifiers};

use super::{CTRL_C_COPIES_SELECTION, CTRL_V_PASTES};

const ESC: u8 = 0x1B;

/// Keys the app keeps for itself rather than sending to the shell.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shortcut {
    Copy,
    Paste,
    NewTab,
    CloseTab,
    NextTab,
    PreviousTab,
    GoToTab(usize),
    ToggleFullscreen,
    ScrollPageUp,
    ScrollPageDown,
    ScrollTop,
    ScrollBottom,
}

/// `physical` matters for Ctrl+Shift+digit, where shift changes the logical key.
pub fn shortcut(
    key: Key,
    physical: Option<Key>,
    mods: Modifiers,
    mode: TermMode,
    has_selection: bool,
) -> Option<Shortcut> {
    let ctrl_shift = mods.ctrl && mods.shift && !mods.alt;
    let ctrl_only = mods.ctrl && !mods.shift && !mods.alt;
    let shift_only = mods.shift && !mods.ctrl && !mods.alt;

    if key == Key::F11 && !mods.any() {
        return Some(Shortcut::ToggleFullscreen);
    }
    if ctrl_shift {
        let digit = [Key::Num1, Key::Num2, Key::Num3, Key::Num4, Key::Num5]
            .iter()
            .position(|k| *k == physical.unwrap_or(key));
        if let Some(index) = digit {
            return Some(Shortcut::GoToTab(index));
        }
        match key {
            Key::C => return Some(Shortcut::Copy),
            Key::V => return Some(Shortcut::Paste),
            Key::T => return Some(Shortcut::NewTab),
            Key::W => return Some(Shortcut::CloseTab),
            Key::Tab => return Some(Shortcut::PreviousTab),
            _ => {}
        }
    }
    if ctrl_only {
        match key {
            Key::Tab => return Some(Shortcut::NextTab),
            Key::V if CTRL_V_PASTES => return Some(Shortcut::Paste),
            Key::C if CTRL_C_COPIES_SELECTION && has_selection => return Some(Shortcut::Copy),
            _ => {}
        }
    }
    if shift_only {
        if key == Key::Insert {
            return Some(Shortcut::Paste);
        }
        // Full-screen programs have no scrollback and want these keys.
        if !mode.contains(TermMode::ALT_SCREEN) {
            match key {
                Key::PageUp => return Some(Shortcut::ScrollPageUp),
                Key::PageDown => return Some(Shortcut::ScrollPageDown),
                Key::Home => return Some(Shortcut::ScrollTop),
                Key::End => return Some(Shortcut::ScrollBottom),
                _ => {}
            }
        }
    }
    None
}

/// The xterm modifier parameter: 1 plus shift 1, alt 2, ctrl 4.
fn modifier_param(mods: Modifiers) -> u8 {
    1 + u8::from(mods.shift) + 2 * u8::from(mods.alt) + 4 * u8::from(mods.ctrl)
}

/// Keys with a letter final byte, such as the arrows.
fn letter_sequence(letter: u8, mods: Modifiers, application: bool) -> Vec<u8> {
    let param = modifier_param(mods);
    if param > 1 {
        format!("\x1b[1;{param}{}", letter as char).into_bytes()
    } else if application {
        vec![ESC, b'O', letter]
    } else {
        vec![ESC, b'[', letter]
    }
}

/// Keys with a numeric code, such as Page Up.
fn tilde_sequence(code: u8, mods: Modifiers) -> Vec<u8> {
    let param = modifier_param(mods);
    if param > 1 {
        format!("\x1b[{code};{param}~").into_bytes()
    } else {
        format!("\x1b[{code}~").into_bytes()
    }
}

fn with_alt(bytes: Vec<u8>, mods: Modifiers) -> Vec<u8> {
    if mods.alt {
        let mut out = vec![ESC];
        out.extend(bytes);
        out
    } else {
        bytes
    }
}

/// The control code for Ctrl+`key`, if it has one.
fn control_code(key: Key) -> Option<u8> {
    let name = key.name();
    if name.len() == 1 && name.as_bytes()[0].is_ascii_uppercase() {
        return Some(name.as_bytes()[0] - b'A' + 1);
    }
    match key {
        Key::Space | Key::Num2 => Some(0x00),
        Key::OpenBracket | Key::Num3 => Some(0x1B),
        Key::Backslash | Key::Num4 => Some(0x1C),
        Key::CloseBracket | Key::Num5 => Some(0x1D),
        Key::Num6 => Some(0x1E),
        Key::Slash | Key::Minus | Key::Num7 => Some(0x1F),
        Key::Num8 => Some(0x7F),
        _ => None,
    }
}

/// Bytes for a key press, or `None` if it arrives as text instead.
pub fn encode_key(key: Key, mods: Modifiers, mode: TermMode) -> Option<Vec<u8>> {
    let application = mode.contains(TermMode::APP_CURSOR);
    let bytes = match key {
        Key::ArrowUp => letter_sequence(b'A', mods, application),
        Key::ArrowDown => letter_sequence(b'B', mods, application),
        Key::ArrowRight => letter_sequence(b'C', mods, application),
        Key::ArrowLeft => letter_sequence(b'D', mods, application),
        Key::Home => letter_sequence(b'H', mods, application),
        Key::End => letter_sequence(b'F', mods, application),
        Key::Insert => tilde_sequence(2, mods),
        Key::Delete => tilde_sequence(3, mods),
        Key::PageUp => tilde_sequence(5, mods),
        Key::PageDown => tilde_sequence(6, mods),
        Key::F1 => letter_sequence(b'P', mods, true),
        Key::F2 => letter_sequence(b'Q', mods, true),
        Key::F3 => letter_sequence(b'R', mods, true),
        Key::F4 => letter_sequence(b'S', mods, true),
        Key::F5 => tilde_sequence(15, mods),
        Key::F6 => tilde_sequence(17, mods),
        Key::F7 => tilde_sequence(18, mods),
        Key::F8 => tilde_sequence(19, mods),
        Key::F9 => tilde_sequence(20, mods),
        Key::F10 => tilde_sequence(21, mods),
        Key::F11 => tilde_sequence(23, mods),
        Key::F12 => tilde_sequence(24, mods),
        Key::Enter if mode.contains(TermMode::LINE_FEED_NEW_LINE) => with_alt(vec![b'\r', b'\n'], mods),
        Key::Enter => with_alt(vec![b'\r'], mods),
        Key::Tab if mods.shift => b"\x1b[Z".to_vec(),
        Key::Tab => with_alt(vec![b'\t'], mods),
        Key::Backspace if mods.ctrl => with_alt(vec![0x08], mods),
        Key::Backspace => with_alt(vec![0x7F], mods),
        Key::Escape => with_alt(vec![ESC], mods),
        _ if mods.ctrl => with_alt(vec![control_code(key)?], mods),
        _ => return None,
    };
    Some(bytes)
}

/// Bytes for typed text. Alt sends an escape first, as in xterm.
pub fn encode_text(text: &str, alt: bool) -> Vec<u8> {
    let mut out = Vec::with_capacity(text.len() + 1);
    if alt {
        out.push(ESC);
    }
    out.extend_from_slice(text.as_bytes());
    out
}

/// Bytes for pasted text. Escape characters are removed in bracketed mode so a
/// pasted string cannot end the bracket and run commands.
pub fn encode_paste(text: &str, mode: TermMode) -> Vec<u8> {
    if mode.contains(TermMode::BRACKETED_PASTE) {
        let clean: String = text.chars().filter(|c| *c != '\x1b' && *c != '\u{9b}').collect();
        format!("\x1b[200~{clean}\x1b[201~").into_bytes()
    } else {
        text.replace("\r\n", "\r").replace('\n', "\r").into_bytes()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MouseAction {
    Press(u8),
    Release(u8),
    /// Pointer moved with this button held, or 3 for no button.
    Motion(u8),
    WheelUp,
    WheelDown,
}

/// A mouse report for a zero-based cell, in SGR form if the program asked for
/// it, or `None` if the position does not fit the legacy format.
pub fn encode_mouse(action: MouseAction, col: usize, row: usize, mods: Modifiers, mode: TermMode) -> Option<Vec<u8>> {
    let held = 4 * u8::from(mods.shift) + 8 * u8::from(mods.alt) + 16 * u8::from(mods.ctrl);
    let (code, pressed) = match action {
        MouseAction::Press(button) => (button, true),
        MouseAction::Release(button) => (button, false),
        MouseAction::Motion(button) => (button + 32, true),
        MouseAction::WheelUp => (64, true),
        MouseAction::WheelDown => (65, true),
    };
    if mode.contains(TermMode::SGR_MOUSE) {
        let end = if pressed { 'M' } else { 'm' };
        return Some(format!("\x1b[<{};{};{}{end}", code + held, col + 1, row + 1).into_bytes());
    }
    // The legacy format has no way to say which button was released.
    let code = if pressed { code } else { 3 } + held;
    if col > 222 || row > 222 {
        return None;
    }
    Some(vec![ESC, b'[', b'M', code + 32, col as u8 + 33, row as u8 + 33])
}

#[cfg(test)]
mod tests {
    use super::*;

    const NONE: Modifiers = Modifiers::NONE;
    const CTRL: Modifiers = Modifiers::CTRL;
    const SHIFT: Modifiers = Modifiers::SHIFT;
    const ALT: Modifiers = Modifiers::ALT;

    fn key(k: Key, m: Modifiers) -> Option<Vec<u8>> {
        encode_key(k, m, TermMode::empty())
    }

    #[test]
    fn control_keys_reach_the_shell() {
        assert_eq!(key(Key::C, CTRL), Some(vec![3]));
        assert_eq!(key(Key::D, CTRL), Some(vec![4]));
        assert_eq!(key(Key::Z, CTRL), Some(vec![26]));
        assert_eq!(key(Key::Space, CTRL), Some(vec![0]));
        assert_eq!(key(Key::OpenBracket, CTRL), Some(vec![0x1B]));
        assert_eq!(key(Key::Slash, CTRL), Some(vec![0x1F]));
    }

    #[test]
    fn plain_letters_come_through_as_text_only() {
        assert_eq!(key(Key::A, NONE), None);
        assert_eq!(key(Key::Space, NONE), None);
        assert_eq!(encode_text("é", false), "é".as_bytes());
        assert_eq!(encode_text("b", true), b"\x1bb");
    }

    #[test]
    fn arrows_follow_the_cursor_key_mode() {
        assert_eq!(key(Key::ArrowUp, NONE).unwrap(), b"\x1b[A");
        assert_eq!(encode_key(Key::ArrowUp, NONE, TermMode::APP_CURSOR).unwrap(), b"\x1bOA");
        assert_eq!(key(Key::ArrowLeft, CTRL).unwrap(), b"\x1b[1;5D");
        assert_eq!(key(Key::ArrowRight, SHIFT).unwrap(), b"\x1b[1;2C");
    }

    #[test]
    fn editing_and_function_keys_use_xterm_sequences() {
        assert_eq!(key(Key::Enter, NONE).unwrap(), b"\r");
        assert_eq!(key(Key::Backspace, NONE).unwrap(), b"\x7f");
        assert_eq!(key(Key::Backspace, ALT).unwrap(), b"\x1b\x7f");
        assert_eq!(key(Key::Tab, SHIFT).unwrap(), b"\x1b[Z");
        assert_eq!(key(Key::Delete, NONE).unwrap(), b"\x1b[3~");
        assert_eq!(key(Key::PageUp, NONE).unwrap(), b"\x1b[5~");
        assert_eq!(key(Key::F1, NONE).unwrap(), b"\x1bOP");
        assert_eq!(key(Key::F10, NONE).unwrap(), b"\x1b[21~");
        assert_eq!(key(Key::F5, SHIFT).unwrap(), b"\x1b[15;2~");
    }

    #[test]
    fn paste_respects_bracketed_mode_and_cannot_escape_it() {
        assert_eq!(encode_paste("a\nb", TermMode::empty()), b"a\rb");
        assert_eq!(encode_paste("a\r\nb", TermMode::empty()), b"a\rb");
        let evil = "x\x1b[201~rm -rf /\n";
        assert_eq!(
            encode_paste(evil, TermMode::BRACKETED_PASTE),
            b"\x1b[200~x[201~rm -rf /\n\x1b[201~"
        );
    }

    #[test]
    fn mouse_reports_use_the_requested_format() {
        let sgr = TermMode::SGR_MOUSE;
        let press = MouseAction::Press(0);
        assert_eq!(encode_mouse(press, 4, 9, NONE, sgr).unwrap(), b"\x1b[<0;5;10M");
        assert_eq!(encode_mouse(MouseAction::Release(0), 4, 9, NONE, sgr).unwrap(), b"\x1b[<0;5;10m");
        assert_eq!(encode_mouse(MouseAction::WheelDown, 0, 0, CTRL, sgr).unwrap(), b"\x1b[<81;1;1M");
        assert_eq!(encode_mouse(MouseAction::Motion(0), 1, 1, NONE, sgr).unwrap(), b"\x1b[<32;2;2M");

        let legacy = TermMode::empty();
        assert_eq!(encode_mouse(press, 0, 0, NONE, legacy).unwrap(), vec![ESC, b'[', b'M', 32, 33, 33]);
        assert_eq!(encode_mouse(MouseAction::Release(2), 0, 0, NONE, legacy).unwrap()[3], 35);
        assert_eq!(encode_mouse(press, 300, 0, NONE, legacy), None);
    }

    #[test]
    fn app_shortcuts_do_not_steal_the_shells_keys() {
        let plain = TermMode::empty();
        let cs = Modifiers { ctrl: true, shift: true, ..NONE };
        assert_eq!(shortcut(Key::C, None, cs, plain, false), Some(Shortcut::Copy));
        assert_eq!(shortcut(Key::V, None, cs, plain, false), Some(Shortcut::Paste));
        assert_eq!(shortcut(Key::Exclamationmark, Some(Key::Num1), cs, plain, false), Some(Shortcut::GoToTab(0)));
        assert_eq!(shortcut(Key::PageUp, None, SHIFT, plain, false), Some(Shortcut::ScrollPageUp));
        assert_eq!(shortcut(Key::PageUp, None, SHIFT, TermMode::ALT_SCREEN, false), None);
        assert_eq!(shortcut(Key::PageUp, None, NONE, plain, false), None);
        if !CTRL_C_COPIES_SELECTION {
            assert_eq!(shortcut(Key::C, None, CTRL, plain, true), None);
        }
        assert_eq!(shortcut(Key::C, None, CTRL, plain, false), None);
    }
}
