//! Draws the active terminal and routes keyboard and mouse input to it.

use alacritty_terminal::grid::{Dimensions, Scroll};
use alacritty_terminal::index::{Column, Line, Point, Side};
use alacritty_terminal::selection::{Selection, SelectionType};
use alacritty_terminal::term::TermMode;
use egui::{
    pos2, vec2, Align2, Color32, Context, CursorIcon, Event, FontFamily, FontId, Key, Modifiers,
    MouseWheelUnit, Painter, PointerButton, Pos2, Rect, Stroke, StrokeKind, ViewportCommand,
};

use super::layout::{Layout, PanelId, TAB_COUNT};
use super::panel::{self, Style, TabLabel};
use crate::term::frame::{Cursor, CursorKind, Frame, Span, TextStyle};
use crate::term::input::{self, MouseAction, Shortcut};
use crate::term::session::{GridSize, Session};
use crate::term::tabs::Tabs;
use crate::theme::Theme;

/// Space between the terminal's frame and its text.
const INSET: f32 = 8.0;
/// Lines moved by one notch of a mouse wheel.
const WHEEL_LINES: f32 = 3.0;
/// Longest gap between clicks that still counts as a double or triple click.
const MULTI_CLICK_SECONDS: f64 = 0.4;

pub const BOLD: &str = "terminal-bold";
pub const ITALIC: &str = "terminal-italic";
pub const BOLD_ITALIC: &str = "terminal-bold-italic";

#[derive(Clone, Copy)]
struct Metrics {
    width: f32,
    height: f32,
}

#[derive(Default)]
struct Clicks {
    last_time: f64,
    last_cell: (usize, usize),
    count: u8,
}

pub struct TerminalView {
    font_size: f32,
    /// Cell size, with the display scale it was measured at.
    metrics: Option<(f32, Metrics)>,
    selecting: bool,
    clicks: Clicks,
    /// A mouse button a program is tracking, with the last cell reported.
    reported_button: Option<u8>,
    last_motion_cell: (usize, usize),
    wheel_remainder: f32,
    /// Modifiers on the latest key, used for the text event that follows it.
    key_mods: Modifiers,
    focused: bool,
}

impl TerminalView {
    pub fn new(font_size: f32) -> Self {
        Self {
            font_size,
            metrics: None,
            selecting: false,
            clicks: Clicks::default(),
            reported_button: None,
            last_motion_cell: (0, 0),
            wheel_remainder: 0.0,
            key_mods: Modifiers::NONE,
            focused: true,
        }
    }

    fn font(&self, style: TextStyle) -> FontId {
        let family = match style {
            TextStyle::Regular => FontFamily::Monospace,
            TextStyle::Bold => FontFamily::Name(BOLD.into()),
            TextStyle::Italic => FontFamily::Name(ITALIC.into()),
            TextStyle::BoldItalic => FontFamily::Name(BOLD_ITALIC.into()),
        };
        FontId::new(self.font_size, family)
    }

    /// Measured from the real font, snapped so rows land on whole pixels.
    /// Measured again if the window moves to a display with another scale.
    fn metrics(&mut self, painter: &Painter) -> Metrics {
        let pixels = painter.pixels_per_point();
        if let Some((scale, metrics)) = self.metrics {
            if scale == pixels {
                return metrics;
            }
        }
        let font = FontId::new(self.font_size, FontFamily::Monospace);
        let sample = "M".repeat(100);
        let width = painter.layout_no_wrap(sample, font.clone(), Color32::WHITE).size().x / 100.0;
        let height = painter.fonts_mut(|fonts| fonts.row_height(&font));
        let metrics = Metrics { width, height: (height * pixels).ceil() / pixels };
        self.metrics = Some((pixels, metrics));
        metrics
    }

    /// Run one frame: apply the grid size, take input, then draw.
    pub fn show(&mut self, ctx: &Context, painter: &Painter, layout: &Layout, tabs: &mut Tabs, style: &Style<'_>) {
        let panel = layout
            .panels
            .iter()
            .find(|(id, _)| *id == PanelId::Terminal)
            .map_or(Rect::NOTHING, |(_, rect)| *rect);
        let grid_area = panel.shrink(INSET);
        let metrics = self.metrics(painter);
        tabs.resize(grid_size(grid_area, metrics, painter.pixels_per_point()));
        tabs.close_finished();

        let view = Area { grid: grid_area, metrics };
        self.handle_events(ctx, &view, &layout.tabs, tabs);
        self.update_pointer_icon(ctx, &view, &layout.tabs, tabs);
        for session in tabs.sessions() {
            if let Some(text) = session.take_clipboard() {
                ctx.copy_text(text);
            }
        }

        panel::tab_bar(painter, &layout.tabs, &tab_labels(tabs), style);
        panel::frame(painter, panel, style);
        if let Some(session) = tabs.active() {
            let frame = session.frame(&style.theme.terminal, self.focused);
            self.draw_grid(painter, &view, &frame, style.theme);
            if let Some(ended) = session.ended() {
                let hint = format!("{} / PRESS ENTER TO RESTART", ended.message);
                draw_banner(painter, panel, &hint, style);
            }
        }
    }

    fn handle_events(&mut self, ctx: &Context, view: &Area, tab_rects: &[Rect], tabs: &mut Tabs) {
        let (events, time, pointer) = ctx.input(|i| (i.events.clone(), i.time, i.pointer.latest_pos()));
        for event in events {
            match event {
                Event::Key { key, physical_key, pressed: true, modifiers, .. } => {
                    self.key_mods = modifiers;
                    self.on_key(ctx, key, physical_key, modifiers, tabs);
                }
                Event::Text(text) => {
                    if let Some(session) = live(tabs) {
                        session.scroll(Scroll::Bottom);
                        session.send(input::encode_text(&text, self.key_mods.alt));
                    }
                }
                Event::Paste(text) => {
                    if let Some(session) = live(tabs) {
                        session.scroll(Scroll::Bottom);
                        session.paste(&text);
                    }
                }
                Event::PointerButton { pos, button, pressed, modifiers } => {
                    if pressed && self.on_tab_press(pos, button, tab_rects, tabs) {
                        continue;
                    }
                    if let Some(session) = live(tabs) {
                        self.on_button(ctx, session, view, pos, button, pressed, modifiers, time);
                    }
                }
                Event::PointerMoved(pos) => {
                    if let Some(session) = live(tabs) {
                        self.on_move(session, view, pos);
                    }
                }
                Event::MouseWheel { unit, delta, modifiers, .. } => {
                    let over_grid = pointer.is_some_and(|p| view.grid.contains(p));
                    if let (true, Some(session)) = (over_grid, live(tabs)) {
                        self.on_wheel(session, view, unit, delta.y, modifiers, pointer.unwrap_or_default());
                    }
                }
                Event::WindowFocused(focused) => {
                    self.focused = focused;
                    if let Some(session) = live(tabs) {
                        if session.mode().contains(TermMode::FOCUS_IN_OUT) {
                            session.send(if focused { b"\x1b[I".to_vec() } else { b"\x1b[O".to_vec() });
                        }
                    }
                }
                _ => {}
            }
        }
    }

    fn on_key(&mut self, ctx: &Context, key: Key, physical: Option<Key>, mods: Modifiers, tabs: &mut Tabs) {
        let (mode, selected) = tabs.active().map_or((TermMode::empty(), false), |session| {
            (session.mode(), session.with_term(|term| term.selection.as_ref().is_some_and(|s| !s.is_empty())))
        });
        if let Some(shortcut) = input::shortcut(key, physical, mods, mode, selected) {
            self.run_shortcut(ctx, shortcut, tabs);
            return;
        }
        if tabs.active().is_some_and(|session| session.ended().is_some()) {
            if key == Key::Enter {
                tabs.restart_active();
            }
            return;
        }
        if let (Some(bytes), Some(session)) = (input::encode_key(key, mods, mode), tabs.active()) {
            session.scroll(Scroll::Bottom);
            session.send(bytes);
        }
    }

    fn run_shortcut(&mut self, ctx: &Context, shortcut: Shortcut, tabs: &mut Tabs) {
        match shortcut {
            Shortcut::Copy => {
                let text = tabs.active().and_then(|s| s.with_term(|term| term.selection_to_string()));
                if let Some(text) = text.filter(|t| !t.is_empty()) {
                    ctx.copy_text(text);
                }
            }
            Shortcut::Paste => ctx.send_viewport_cmd(ViewportCommand::RequestPaste),
            Shortcut::NewTab => tabs.open_next_free(),
            Shortcut::CloseTab => tabs.close(tabs.active_index()),
            Shortcut::NextTab => tabs.cycle(true),
            Shortcut::PreviousTab => tabs.cycle(false),
            Shortcut::GoToTab(index) => tabs.select(index),
            Shortcut::ToggleFullscreen => {
                let fullscreen = ctx.input(|i| i.viewport().fullscreen.unwrap_or(false));
                ctx.send_viewport_cmd(ViewportCommand::Fullscreen(!fullscreen));
            }
            Shortcut::ScrollPageUp => scroll(tabs, Scroll::PageUp),
            Shortcut::ScrollPageDown => scroll(tabs, Scroll::PageDown),
            Shortcut::ScrollTop => scroll(tabs, Scroll::Top),
            Shortcut::ScrollBottom => scroll(tabs, Scroll::Bottom),
        }
    }

    /// Returns true if the press landed on the tab bar.
    fn on_tab_press(&mut self, pos: Pos2, button: PointerButton, rects: &[Rect], tabs: &mut Tabs) -> bool {
        let Some(index) = rects.iter().position(|r| r.contains(pos)) else { return false };
        if button != PointerButton::Primary {
            return true;
        }
        if !tabs.is_open(index) {
            tabs.open(index);
        } else if index > 0 && panel::tab_close_rect(rects[index]).contains(pos) {
            tabs.close(index);
        } else {
            tabs.select(index);
        }
        true
    }

    #[allow(clippy::too_many_arguments)]
    fn on_button(
        &mut self,
        ctx: &Context,
        session: &Session,
        view: &Area,
        pos: Pos2,
        button: PointerButton,
        pressed: bool,
        mods: Modifiers,
        time: f64,
    ) {
        let code = match button {
            PointerButton::Primary => 0,
            PointerButton::Middle => 1,
            PointerButton::Secondary => 2,
            _ => return,
        };
        let inside = view.grid.contains(pos);
        let mode = session.mode();
        // Shift lets you select even when a program has taken the mouse.
        if mode.intersects(TermMode::MOUSE_MODE) && !mods.shift {
            let (col, row) = view.cell(pos);
            let action = if pressed && inside {
                self.reported_button = Some(code);
                self.last_motion_cell = (col, row);
                MouseAction::Press(code)
            } else if !pressed && self.reported_button == Some(code) {
                self.reported_button = None;
                MouseAction::Release(code)
            } else {
                return;
            };
            if let Some(bytes) = input::encode_mouse(action, col, row, mods, mode) {
                session.send(bytes);
            }
            return;
        }
        match (button, pressed) {
            (PointerButton::Primary, true) if inside => self.start_selection(session, view, pos, time),
            (PointerButton::Primary, false) => {
                self.selecting = false;
                session.with_term(|term| {
                    if term.selection.as_ref().is_some_and(Selection::is_empty) {
                        term.selection = None;
                    }
                });
            }
            (PointerButton::Secondary | PointerButton::Middle, true) if inside => {
                ctx.send_viewport_cmd(ViewportCommand::RequestPaste);
            }
            _ => {}
        }
    }

    fn start_selection(&mut self, session: &Session, view: &Area, pos: Pos2, time: f64) {
        let cell = view.cell(pos);
        let again = time - self.clicks.last_time < MULTI_CLICK_SECONDS && cell == self.clicks.last_cell;
        self.clicks.count = if again { self.clicks.count % 3 + 1 } else { 1 };
        self.clicks.last_time = time;
        self.clicks.last_cell = cell;
        let kind = match self.clicks.count {
            1 => SelectionType::Simple,
            2 => SelectionType::Semantic,
            _ => SelectionType::Lines,
        };
        self.selecting = true;
        session.with_term(|term| {
            let (point, side) = view.point(pos, term.grid().display_offset(), term.grid());
            term.selection = Some(Selection::new(kind, point, side));
        });
    }

    fn on_move(&mut self, session: &Session, view: &Area, pos: Pos2) {
        if self.selecting {
            session.with_term(|term| {
                // Dragging past the top or bottom edge scrolls the history.
                if pos.y < view.grid.min.y {
                    term.scroll_display(Scroll::Delta(1));
                } else if pos.y > view.grid.max.y {
                    term.scroll_display(Scroll::Delta(-1));
                }
                let (point, side) = view.point(pos, term.grid().display_offset(), term.grid());
                if let Some(selection) = &mut term.selection {
                    selection.update(point, side);
                }
            });
            return;
        }
        let mode = session.mode();
        let wants_motion = mode.contains(TermMode::MOUSE_MOTION)
            || (mode.contains(TermMode::MOUSE_DRAG) && self.reported_button.is_some());
        let cell = view.cell(pos);
        if wants_motion && cell != self.last_motion_cell && view.grid.contains(pos) {
            self.last_motion_cell = cell;
            let action = MouseAction::Motion(self.reported_button.unwrap_or(3));
            if let Some(bytes) = input::encode_mouse(action, cell.0, cell.1, Modifiers::NONE, mode) {
                session.send(bytes);
            }
        }
    }

    fn on_wheel(&mut self, session: &Session, view: &Area, unit: MouseWheelUnit, delta: f32, mods: Modifiers, pos: Pos2) {
        let lines = match unit {
            MouseWheelUnit::Line => delta * WHEEL_LINES,
            MouseWheelUnit::Point => delta / view.metrics.height,
            MouseWheelUnit::Page => delta * view.rows() as f32,
        } + self.wheel_remainder;
        let whole = lines.trunc();
        self.wheel_remainder = lines - whole;
        if whole == 0.0 {
            return;
        }

        let mode = session.mode();
        let count = whole.abs() as usize;
        if mode.intersects(TermMode::MOUSE_MODE) {
            let action = if whole > 0.0 { MouseAction::WheelUp } else { MouseAction::WheelDown };
            let (col, row) = view.cell(pos);
            for _ in 0..count {
                if let Some(bytes) = input::encode_mouse(action, col, row, mods, mode) {
                    session.send(bytes);
                }
            }
        } else if mode.contains(TermMode::ALT_SCREEN | TermMode::ALTERNATE_SCROLL) {
            // Full-screen programs have no history, so the wheel acts as arrows.
            let key = if whole > 0.0 { Key::ArrowUp } else { Key::ArrowDown };
            if let Some(bytes) = input::encode_key(key, Modifiers::NONE, mode) {
                session.send(bytes.repeat(count));
            }
        } else {
            session.scroll(Scroll::Delta(whole as i32));
        }
    }

    fn update_pointer_icon(&self, ctx: &Context, view: &Area, tab_rects: &[Rect], tabs: &Tabs) {
        let Some(pos) = ctx.input(|i| i.pointer.latest_pos()) else { return };
        if tab_rects.iter().any(|r| r.contains(pos)) {
            ctx.set_cursor_icon(CursorIcon::PointingHand);
        } else if view.grid.contains(pos) {
            let tracked = tabs.active().is_some_and(|s| s.mode().intersects(TermMode::MOUSE_MODE));
            if !tracked {
                ctx.set_cursor_icon(CursorIcon::Text);
            }
        }
    }

    fn draw_grid(&self, painter: &Painter, view: &Area, frame: &Frame, theme: &Theme) {
        let clip = painter.with_clip_rect(view.grid);
        for span in &frame.fills {
            clip.rect_filled(view.span_rect(span.line, span.col, span.cells), 0.0, span.colour);
        }
        if let Some(cursor) = frame.cursor.filter(|c| c.kind == CursorKind::Block) {
            clip.rect_filled(view.span_rect(cursor.line, cursor.col, cursor.cells), 0.0, cursor.colour);
        }
        for run in &frame.runs {
            let font = self.font(run.style);
            let galley = clip.layout_no_wrap(run.text.clone(), font, run.colour);
            let area = view.span_rect(run.line, run.col, run.cells);
            // Centring only matters for a glyph from a fallback font.
            let x = area.min.x + (area.width() - galley.size().x).max(0.0) * 0.5;
            clip.galley(pos2(x, area.min.y), galley, run.colour);
        }
        for span in &frame.underlines {
            line_through(&clip, view, span, 0.92);
        }
        for span in &frame.strikes {
            line_through(&clip, view, span, 0.55);
        }
        if let Some(cursor) = frame.cursor {
            draw_outline_cursor(&clip, view, cursor);
        }
        draw_scroll_position(&clip, view, frame, theme);
    }
}

fn scroll(tabs: &Tabs, scroll: Scroll) {
    if let Some(session) = tabs.active() {
        session.scroll(scroll);
    }
}

/// The active session, unless its shell has ended.
fn live(tabs: &Tabs) -> Option<&Session> {
    tabs.active().filter(|session| session.ended().is_none())
}

fn grid_size(area: Rect, metrics: Metrics, pixels: f32) -> GridSize {
    GridSize {
        cols: ((area.width() / metrics.width) as usize).max(2),
        rows: ((area.height() / metrics.height) as usize).max(1),
        cell_width: (metrics.width * pixels).round() as u16,
        cell_height: (metrics.height * pixels).round() as u16,
    }
}

fn tab_labels(tabs: &Tabs) -> Vec<TabLabel> {
    (0..TAB_COUNT)
        .map(|index| {
            let open = tabs.is_open(index);
            let text = match (index, open) {
                (0, _) => "MAIN SHELL".to_owned(),
                (_, true) => format!("SHELL {}", index + 1),
                (_, false) => "EMPTY".to_owned(),
            };
            TabLabel { text, open, active: index == tabs.active_index(), closable: index > 0 && open }
        })
        .collect()
}

/// Geometry of the grid on screen.
struct Area {
    grid: Rect,
    metrics: Metrics,
}

impl Area {
    fn rows(&self) -> usize {
        ((self.grid.height() / self.metrics.height) as usize).max(1)
    }

    fn cols(&self) -> usize {
        ((self.grid.width() / self.metrics.width) as usize).max(2)
    }

    /// The zero-based (column, row) under `pos`, clamped to the grid.
    fn cell(&self, pos: Pos2) -> (usize, usize) {
        let local = pos - self.grid.min;
        let col = (local.x / self.metrics.width).max(0.0) as usize;
        let row = (local.y / self.metrics.height).max(0.0) as usize;
        (col.min(self.cols() - 1), row.min(self.rows() - 1))
    }

    fn point<D: Dimensions>(&self, pos: Pos2, offset: usize, grid: &D) -> (Point, Side) {
        let (col, row) = self.cell(pos);
        let col = col.min(grid.columns() - 1);
        let row = row.min(grid.screen_lines() - 1);
        let within = (pos.x - self.grid.min.x) / self.metrics.width - col as f32;
        let side = if within < 0.5 { Side::Left } else { Side::Right };
        (Point::new(Line(row as i32 - offset as i32), Column(col)), side)
    }

    fn span_rect(&self, line: usize, col: usize, cells: usize) -> Rect {
        let min = self.grid.min + vec2(col as f32 * self.metrics.width, line as f32 * self.metrics.height);
        Rect::from_min_size(min, vec2(cells as f32 * self.metrics.width, self.metrics.height))
    }
}

fn line_through(painter: &Painter, view: &Area, span: &Span, height: f32) {
    let area = view.span_rect(span.line, span.col, span.cells);
    let y = area.min.y + area.height() * height;
    painter.hline(area.x_range(), y, Stroke::new(1.0, span.colour));
}

fn draw_outline_cursor(painter: &Painter, view: &Area, cursor: Cursor) {
    let area = view.span_rect(cursor.line, cursor.col, cursor.cells);
    match cursor.kind {
        CursorKind::Block => {}
        CursorKind::Hollow => {
            painter.rect_stroke(area, 0.0, Stroke::new(1.0, cursor.colour), StrokeKind::Inside);
        }
        CursorKind::Beam => {
            let bar = Rect::from_min_size(area.min, vec2(2.0, area.height()));
            painter.rect_filled(bar, 0.0, cursor.colour);
        }
        CursorKind::Underline => {
            let bar = Rect::from_min_max(pos2(area.min.x, area.max.y - 2.0), area.max);
            painter.rect_filled(bar, 0.0, cursor.colour);
        }
    }
}

/// While scrolled back, show where you are and how far back it is.
fn draw_scroll_position(painter: &Painter, view: &Area, frame: &Frame, theme: &Theme) {
    if frame.offset == 0 || frame.history == 0 {
        return;
    }
    let total = (frame.history + frame.rows) as f32;
    let height = (view.grid.height() * frame.rows as f32 / total).max(16.0);
    let travel = view.grid.height() - height;
    let top = view.grid.min.y + travel * (1.0 - frame.offset as f32 / frame.history as f32);
    let bar = Rect::from_min_size(pos2(view.grid.max.x - 3.0, top), vec2(3.0, height));
    painter.rect_filled(bar, 0.0, theme.foreground);
    painter.text(
        view.grid.right_top() + vec2(-10.0, 2.0),
        Align2::RIGHT_TOP,
        format!("-{} / {}", frame.offset, frame.history),
        FontId::monospace(14.0),
        theme.foreground,
    );
}

fn draw_banner(painter: &Painter, panel: Rect, text: &str, style: &Style<'_>) {
    let height = style.fonts.label * 2.2;
    let bar = Rect::from_min_max(pos2(panel.min.x + 2.0, panel.max.y - height - 2.0), panel.max - vec2(2.0, 2.0));
    painter.rect_filled(bar, 0.0, style.theme.foreground);
    painter.with_clip_rect(bar).text(
        bar.center(),
        Align2::CENTER_CENTER,
        text,
        style.label(),
        style.theme.background,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn area() -> Area {
        let grid = Rect::from_min_size(pos2(100.0, 50.0), vec2(800.0, 400.0));
        Area { grid, metrics: Metrics { width: 10.0, height: 20.0 } }
    }

    #[test]
    fn grid_size_fits_whole_cells_only() {
        let metrics = Metrics { width: 10.0, height: 20.0 };
        let rect = Rect::from_min_size(Pos2::ZERO, vec2(805.0, 419.0));
        let size = grid_size(rect, metrics, 2.0);
        assert_eq!((size.cols, size.rows), (80, 20));
        assert_eq!((size.cell_width, size.cell_height), (20, 40));
    }

    #[test]
    fn grid_size_survives_a_tiny_window() {
        let metrics = Metrics { width: 10.0, height: 20.0 };
        let size = grid_size(Rect::from_min_size(Pos2::ZERO, vec2(1.0, 1.0)), metrics, 1.0);
        assert_eq!((size.cols, size.rows), (2, 1));
    }

    #[test]
    fn positions_map_to_cells_and_clamp_at_the_edges() {
        let view = area();
        assert_eq!(view.cell(pos2(100.0, 50.0)), (0, 0));
        assert_eq!(view.cell(pos2(125.0, 95.0)), (2, 2));
        assert_eq!(view.cell(pos2(0.0, 0.0)), (0, 0));
        assert_eq!(view.cell(pos2(5000.0, 5000.0)), (79, 19));
    }

    #[test]
    fn selection_points_account_for_scrollback() {
        let view = area();
        let size = GridSize { cols: 80, rows: 20, cell_width: 10, cell_height: 20 };
        let (point, side) = view.point(pos2(128.0, 70.0), 5, &size);
        assert_eq!(point, Point::new(Line(-4), Column(2)));
        assert_eq!(side, Side::Right);
    }
}
