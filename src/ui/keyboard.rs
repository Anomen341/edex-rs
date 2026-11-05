//! The on-screen keyboard. It lights up keys as they are physically pressed,
//! and is display only: typing still goes straight to the terminal.

use std::collections::HashMap;

use egui::{pos2, vec2, Align2, Color32, Context, Event, FontFamily, FontId, Key, Modifiers, Painter, Pos2, Rect, Shape, Stroke};

use super::layout::{Layout, PanelId};
use super::panel::{Style, DISPLAY_FONT};

/// How long a key takes to fade after release, in seconds.
const FADE: f64 = 0.22;
/// The widest row, in key units.
const ROW_UNITS: f32 = 16.0;
const ROWS: usize = 5;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Id {
    Key(Key),
    Shift,
    Ctrl,
    Alt,
    /// Has no key event, so it never lights up.
    Inert,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Look {
    Plain,
    Space,
    Arrow(Arrow),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Arrow {
    Up,
    Down,
    Left,
    Right,
}

#[derive(Clone, Copy, Debug)]
struct KeyCap {
    label: &'static str,
    shifted: &'static str,
    width: f32,
    id: Id,
    look: Look,
}

const fn key(label: &'static str, shifted: &'static str, id: Key) -> KeyCap {
    KeyCap { label, shifted, width: 1.0, id: Id::Key(id), look: Look::Plain }
}

const fn wide(label: &'static str, width: f32, id: Id) -> KeyCap {
    KeyCap { label, shifted: "", width, id, look: Look::Plain }
}

const fn arrow(id: Key, direction: Arrow) -> KeyCap {
    KeyCap { label: "", shifted: "", width: 1.0, id: Id::Key(id), look: Look::Arrow(direction) }
}

const NUMBER_ROW: [KeyCap; 14] = [
    wide("ESC", 1.5, Id::Key(Key::Escape)),
    key("`", "~", Key::Backtick),
    key("1", "!", Key::Num1),
    key("2", "@", Key::Num2),
    key("3", "#", Key::Num3),
    key("4", "$", Key::Num4),
    key("5", "%", Key::Num5),
    key("6", "^", Key::Num6),
    key("7", "&", Key::Num7),
    key("8", "*", Key::Num8),
    key("9", "(", Key::Num9),
    key("0", ")", Key::Num0),
    key("-", "_", Key::Minus),
    key("=", "+", Key::Equals),
];
const BACK: KeyCap = wide("BACK", 1.5, Id::Key(Key::Backspace));

const TOP_ROW: [KeyCap; 13] = [
    wide("TAB", 1.75, Id::Key(Key::Tab)),
    key("Q", "", Key::Q),
    key("W", "", Key::W),
    key("E", "", Key::E),
    key("R", "", Key::R),
    key("T", "", Key::T),
    key("Y", "", Key::Y),
    key("U", "", Key::U),
    key("I", "", Key::I),
    key("O", "", Key::O),
    key("P", "", Key::P),
    key("[", "{", Key::OpenBracket),
    key("]", "}", Key::CloseBracket),
];

const HOME_ROW: [KeyCap; 13] = [
    wide("CAPS", 2.0, Id::Key(Key::CapsLock)),
    key("A", "", Key::A),
    key("S", "", Key::S),
    key("D", "", Key::D),
    key("F", "", Key::F),
    key("G", "", Key::G),
    key("H", "", Key::H),
    key("J", "", Key::J),
    key("K", "", Key::K),
    key("L", "", Key::L),
    key(";", ":", Key::Semicolon),
    key("'", "\"", Key::Quote),
    key("\\", "|", Key::Backslash),
];

const BOTTOM_ROW: [KeyCap; 14] = [
    wide("SHIFT", 1.75, Id::Shift),
    key("<", ">", Key::IntlBackslash),
    key("Z", "", Key::Z),
    key("X", "", Key::X),
    key("C", "", Key::C),
    key("V", "", Key::V),
    key("B", "", Key::B),
    key("N", "", Key::N),
    key("M", "", Key::M),
    key(",", "<", Key::Comma),
    key(".", ">", Key::Period),
    key("/", "?", Key::Slash),
    wide("SHIFT", 1.25, Id::Shift),
    arrow(Key::ArrowUp, Arrow::Up),
];

const SPACE_ROW: [KeyCap; 8] = [
    wide("CTRL", 1.5, Id::Ctrl),
    wide("FN", 1.0, Id::Inert),
    KeyCap { label: "", shifted: "", width: 8.25, id: Id::Key(Key::Space), look: Look::Space },
    wide("ALT GR", 1.25, Id::Alt),
    wide("CTRL", 1.0, Id::Ctrl),
    arrow(Key::ArrowLeft, Arrow::Left),
    arrow(Key::ArrowDown, Arrow::Down),
    arrow(Key::ArrowRight, Arrow::Right),
];

const ENTER: KeyCap = wide("ENTER", 1.75, Id::Key(Key::Enter));

#[derive(Default)]
pub struct KeyboardView {
    held: HashMap<Id, ()>,
    /// When a key was let go, for fading.
    released: HashMap<Id, f64>,
    modifiers: Modifiers,
}

impl KeyboardView {
    pub fn show(&mut self, ctx: &Context, painter: &Painter, layout: &Layout, animate: bool, style: &Style<'_>) {
        let Some(rect) = layout.panels.iter().find(|(id, _)| *id == PanelId::Keyboard).map(|(_, r)| *r) else {
            return;
        };
        let (events, time, modifiers) = ctx.input(|i| (i.events.clone(), i.time, i.modifiers));
        self.observe(&events, time);
        self.track_modifiers(modifiers, time);
        self.released.retain(|_, at| time - *at < FADE);

        let clip = painter.with_clip_rect(rect);
        let board = Board::new(rect);
        let rows: [&[KeyCap]; ROWS] = [&NUMBER_ROW, &TOP_ROW, &HOME_ROW, &BOTTOM_ROW, &SPACE_ROW];
        for (row, keys) in rows.iter().enumerate() {
            let mut x = 0.0;
            for cap in keys.iter() {
                self.draw_key(&clip, board.key_rect(row, x, cap.width), cap, time, animate, style);
                x += cap.width;
            }
        }
        let back = board.key_rect(0, ROW_UNITS - BACK.width, BACK.width);
        self.draw_key(&clip, back, &BACK, time, animate, style);
        self.draw_enter(&clip, &board, time, animate, style);

        if animate && !self.released.is_empty() {
            ctx.request_repaint();
        }
    }

    fn observe(&mut self, events: &[Event], time: f64) {
        for event in events {
            match event {
                Event::Key { key, physical_key, pressed, .. } => {
                    // The physical key, because shift changes the logical one.
                    let id = Id::Key(physical_key.unwrap_or(*key));
                    self.set(id, *pressed, time);
                }
                // Key-up is lost when focus moves away, so nothing may stick.
                Event::WindowFocused(false) => self.held.clear(),
                _ => {}
            }
        }
    }

    fn track_modifiers(&mut self, modifiers: Modifiers, time: f64) {
        if modifiers == self.modifiers {
            return;
        }
        self.set(Id::Shift, modifiers.shift, time);
        self.set(Id::Ctrl, modifiers.ctrl, time);
        self.set(Id::Alt, modifiers.alt, time);
        self.modifiers = modifiers;
    }

    fn set(&mut self, id: Id, pressed: bool, time: f64) {
        if pressed {
            self.held.insert(id, ());
            self.released.remove(&id);
        } else if self.held.remove(&id).is_some() {
            self.released.insert(id, time);
        }
    }

    /// 1 while held, falling to 0 as a released key fades.
    fn glow(&self, id: Id, time: f64, animate: bool) -> f32 {
        if self.held.contains_key(&id) {
            return 1.0;
        }
        match self.released.get(&id) {
            Some(at) if animate => (1.0 - (time - at) / FADE).clamp(0.0, 1.0) as f32,
            _ => 0.0,
        }
    }

    fn draw_key(&self, painter: &Painter, rect: Rect, cap: &KeyCap, time: f64, animate: bool, style: &Style<'_>) {
        let theme = style.theme;
        let level = self.glow(cap.id, time, animate);
        let body = rect.shrink(rect.height() * 0.09);
        if level > 0.0 {
            painter.rect_filled(body, 3.0, theme.foreground.gamma_multiply(level));
        }
        let ink = mix(theme.foreground, theme.terminal.background, level);
        let shifted_ink = mix(theme.muted, theme.terminal.background, level);
        let family = FontFamily::Name(DISPLAY_FONT.into());
        let size = (rect.height() * 0.42).min(rect.width() * 0.6).clamp(9.0, 26.0);

        match cap.look {
            Look::Plain => {
                let wide_label = cap.label.chars().count() > 1;
                let size = if wide_label { size * 0.8 } else { size };
                let at = if cap.shifted.is_empty() { body.center() } else { body.center() + vec2(size * 0.3, size * 0.15) };
                painter.text(at, Align2::CENTER_CENTER, cap.label, FontId::new(size, family.clone()), ink);
                if !cap.shifted.is_empty() {
                    let corner = body.center() - vec2(size * 0.45, size * 0.55);
                    painter.text(corner, Align2::CENTER_CENTER, cap.shifted, FontId::new(size * 0.62, family), shifted_ink);
                }
            }
            Look::Space => {
                painter.rect_stroke(body, 3.0, Stroke::new(1.0, theme.rule), egui::StrokeKind::Inside);
            }
            Look::Arrow(direction) => {
                let square = Rect::from_center_size(body.center(), vec2(1.0, 1.0) * body.height().min(body.width()) * 0.72);
                painter.rect_stroke(square, 2.0, Stroke::new(1.0, theme.rule), egui::StrokeKind::Inside);
                painter.add(Shape::convex_polygon(arrow_points(square, direction), ink, Stroke::NONE));
            }
        }
    }

    /// The tall ENTER key: wide on the top row, narrower below it.
    fn draw_enter(&self, painter: &Painter, board: &Board, time: f64, animate: bool, style: &Style<'_>) {
        let theme = style.theme;
        let top = board.key_rect(1, ROW_UNITS - ENTER.width, ENTER.width);
        let lower = board.key_rect(2, ROW_UNITS - ENTER.width + 0.25, ENTER.width - 0.25);
        let outline = vec![
            top.left_top(),
            top.right_top(),
            lower.right_bottom(),
            lower.left_bottom(),
            pos2(lower.min.x, top.max.y),
            top.left_bottom(),
        ];
        let level = self.glow(ENTER.id, time, animate);
        if level > 0.0 {
            let fill = theme.foreground.gamma_multiply(level);
            painter.rect_filled(top, 3.0, fill);
            painter.rect_filled(lower, 3.0, fill);
        }
        painter.add(Shape::closed_line(outline, Stroke::new(1.0, theme.rule)));
        let size = (top.height() * 0.34).clamp(9.0, 22.0);
        let font = FontId::new(size, FontFamily::Name(DISPLAY_FONT.into()));
        let ink = mix(theme.foreground, theme.terminal.background, level);
        painter.text(top.center(), Align2::CENTER_CENTER, ENTER.label, font, ink);
    }
}

/// Converts key units into screen space.
struct Board {
    area: Rect,
    unit: f32,
    row_height: f32,
}

impl Board {
    fn new(area: Rect) -> Self {
        Self { area, unit: area.width() / ROW_UNITS, row_height: area.height() / ROWS as f32 }
    }

    fn key_rect(&self, row: usize, x: f32, width: f32) -> Rect {
        let min = self.area.min + vec2(x * self.unit, row as f32 * self.row_height);
        Rect::from_min_size(min, vec2(width * self.unit, self.row_height))
    }
}

fn mix(from: Color32, to: Color32, amount: f32) -> Color32 {
    let blend = |a: u8, b: u8| (f32::from(a) + (f32::from(b) - f32::from(a)) * amount) as u8;
    Color32::from_rgb(blend(from.r(), to.r()), blend(from.g(), to.g()), blend(from.b(), to.b()))
}

fn arrow_points(square: Rect, direction: Arrow) -> Vec<Pos2> {
    let c = square.center();
    let s = square.width() * 0.25;
    match direction {
        Arrow::Up => vec![c + vec2(0.0, -s), c + vec2(s, s), c + vec2(-s, s)],
        Arrow::Down => vec![c + vec2(-s, -s), c + vec2(s, -s), c + vec2(0.0, s)],
        Arrow::Left => vec![c + vec2(-s, 0.0), c + vec2(s, -s), c + vec2(s, s)],
        Arrow::Right => vec![c + vec2(-s, -s), c + vec2(s, 0.0), c + vec2(-s, s)],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn press(key: Key, physical: Option<Key>, pressed: bool) -> Event {
        Event::Key { key, physical_key: physical, pressed, repeat: false, modifiers: Modifiers::NONE }
    }

    #[test]
    fn a_held_key_glows_then_fades_after_release() {
        let mut view = KeyboardView::default();
        let id = Id::Key(Key::A);
        view.observe(&[press(Key::A, Some(Key::A), true)], 1.0);
        assert_eq!(view.glow(id, 1.0, true), 1.0);
        view.observe(&[press(Key::A, Some(Key::A), false)], 2.0);
        let halfway = view.glow(id, 2.0 + FADE / 2.0, true);
        assert!((halfway - 0.5).abs() < 0.01, "{halfway}");
        assert_eq!(view.glow(id, 2.0 + FADE * 2.0, true), 0.0);
    }

    #[test]
    fn without_animation_a_key_goes_dark_at_once() {
        let mut view = KeyboardView::default();
        view.observe(&[press(Key::A, None, true), press(Key::A, None, false)], 1.0);
        assert_eq!(view.glow(Id::Key(Key::A), 1.0, false), 0.0);
    }

    #[test]
    fn shifted_keys_light_the_physical_key() {
        let mut view = KeyboardView::default();
        view.observe(&[press(Key::Exclamationmark, Some(Key::Num1), true)], 1.0);
        assert_eq!(view.glow(Id::Key(Key::Num1), 1.0, true), 1.0);
    }

    #[test]
    fn losing_focus_clears_held_keys() {
        let mut view = KeyboardView::default();
        view.observe(&[press(Key::A, None, true), Event::WindowFocused(false)], 1.0);
        assert_eq!(view.glow(Id::Key(Key::A), 1.0, true), 0.0);
    }

    #[test]
    fn modifiers_light_up_and_fade() {
        let mut view = KeyboardView::default();
        view.track_modifiers(Modifiers::SHIFT, 1.0);
        assert_eq!(view.glow(Id::Shift, 1.0, true), 1.0);
        assert_eq!(view.glow(Id::Ctrl, 1.0, true), 0.0);
        view.track_modifiers(Modifiers::NONE, 2.0);
        assert!(view.glow(Id::Shift, 2.0, true) > 0.9);
    }

    fn row_width(keys: &[KeyCap]) -> f32 {
        keys.iter().map(|cap| cap.width).sum()
    }

    #[test]
    fn rows_fit_the_board_and_leave_room_for_their_edge_keys() {
        assert!(row_width(&NUMBER_ROW) + BACK.width <= ROW_UNITS);
        assert!(row_width(&TOP_ROW) + ENTER.width <= ROW_UNITS);
        assert!(row_width(&HOME_ROW) + ENTER.width - 0.25 <= ROW_UNITS);
        assert!(row_width(&BOTTOM_ROW) <= ROW_UNITS);
        assert!(row_width(&SPACE_ROW) <= ROW_UNITS);
    }

    #[test]
    fn the_up_arrow_sits_above_the_down_arrow() {
        let start_of = |row: &[KeyCap], key: Key| {
            let index = row.iter().position(|cap| cap.id == Id::Key(key)).unwrap();
            row_width(&row[..index])
        };
        assert_eq!(start_of(&BOTTOM_ROW, Key::ArrowUp), start_of(&SPACE_ROW, Key::ArrowDown));
    }

    #[test]
    fn every_letter_and_digit_key_is_on_the_board() {
        let all: Vec<Id> = [&NUMBER_ROW[..], &TOP_ROW, &HOME_ROW, &BOTTOM_ROW, &SPACE_ROW]
            .iter()
            .flat_map(|row| row.iter().map(|cap| cap.id))
            .collect();
        for key in [Key::A, Key::M, Key::Z, Key::Num0, Key::Num9, Key::Space, Key::ArrowUp, Key::Tab] {
            assert!(all.contains(&Id::Key(key)), "{key:?}");
        }
    }

    #[test]
    fn key_rectangles_stay_on_the_board() {
        let area = Rect::from_min_size(pos2(10.0, 10.0), vec2(775.0, 250.0));
        let board = Board::new(area);
        assert!(area.expand(0.01).contains_rect(board.key_rect(4, 15.0, 1.0)));
        assert!((board.key_rect(0, 0.0, 1.0).width() - 775.0 / ROW_UNITS).abs() < 0.01);
    }
}
