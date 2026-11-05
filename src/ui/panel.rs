//! Shared shapes: the ruled panel heading, the terminal's outline and tabs, and
//! the text placeholders for panels that are not live yet.

use std::sync::Arc;

use egui::epaint::Mesh;
use egui::{
    pos2, vec2, Align2, Color32, FontFamily, FontId, Painter, Pos2, Rect, Shape, Stroke,
};

use super::layout::{Column, PanelId};
use crate::config::FontSizes;
use crate::theme::Theme;

/// The display font's name in egui's font table.
pub const DISPLAY_FONT: &str = "display";

/// Space between a panel's rule and its content.
pub const PADDING: f32 = 4.0;
const RULE_TICK: f32 = 4.0;
/// How far a tab's edge leans over the height of the bar.
const TAB_SLANT: f32 = 16.0;
/// Size of the cut corners on the terminal's outline.
const CHAMFER: f32 = 14.0;
const BACKDROP_PITCH: f32 = 26.0;
const TAB_CLOSE_MIN_WIDTH: f32 = 120.0;

pub struct Style<'a> {
    pub theme: &'a Theme,
    pub fonts: &'a FontSizes,
}

impl Style<'_> {
    /// Headings and labels.
    pub fn heading(&self) -> FontId {
        FontId::new(self.fonts.label * 1.1, FontFamily::Name(DISPLAY_FONT.into()))
    }

    /// Small labels above values.
    pub fn caption(&self) -> FontId {
        FontId::new(self.fonts.label * 0.95, FontFamily::Name(DISPLAY_FONT.into()))
    }

    fn body(&self) -> FontId {
        FontId::monospace(self.fonts.body)
    }
}

/// The faint dot grid behind everything, as one mesh for the whole window.
pub fn backdrop(window: Rect, colour: Color32) -> Shape {
    let mut mesh = Mesh::default();
    let columns = (window.width() / BACKDROP_PITCH).ceil() as usize;
    let rows = (window.height() / BACKDROP_PITCH).ceil() as usize;
    for row in 0..rows {
        for column in 0..columns {
            let at = window.min + vec2(column as f32, row as f32) * BACKDROP_PITCH + vec2(8.0, 8.0);
            mesh.add_colored_rect(Rect::from_min_size(at, vec2(1.0, 1.0)), colour);
        }
    }
    Shape::mesh(Arc::new(mesh))
}

/// A soft halo under a line, then the line itself.
fn glowing_path(painter: &Painter, points: Vec<Pos2>, closed: bool, style: &Style<'_>) {
    let theme = style.theme;
    let shape = |stroke: Stroke| {
        if closed {
            Shape::closed_line(points.clone(), stroke)
        } else {
            Shape::line(points.clone(), stroke)
        }
    };
    painter.add(shape(Stroke::new(5.0, theme.glow)));
    painter.add(shape(Stroke::new(3.0, theme.glow)));
    painter.add(shape(Stroke::new(1.0, theme.rule)));
}

/// A thin glowing rule with small ticks at each end.
pub fn rule(painter: &Painter, left: f32, right: f32, y: f32, style: &Style<'_>) {
    glowing_path(painter, vec![pos2(left, y), pos2(right, y)], false, style);
    let stroke = Stroke::new(1.0, style.theme.foreground);
    for x in [left, right] {
        painter.vline(x, (y - RULE_TICK * 0.5)..=(y + RULE_TICK * 0.5), stroke);
    }
}

/// A panel: rule along the top, heading under it, and an optional value at the
/// right. Returns a painter clipped to the panel and the area left for content.
pub fn titled(painter: &Painter, rect: Rect, text: &str, value: Option<&str>, style: &Style<'_>) -> (Painter, Rect) {
    let (inner, area) = ruled(painter, rect, style);
    let theme = style.theme;
    inner.text(area.left_top(), Align2::LEFT_TOP, text, style.heading(), theme.foreground);
    if let Some(value) = value {
        inner.text(area.right_top(), Align2::RIGHT_TOP, value, style.caption(), theme.muted);
    }
    let top = style.fonts.label * 1.5;
    (inner, Rect::from_min_max(area.min + vec2(0.0, top), area.max))
}

/// A panel with a rule and no heading.
pub fn ruled(painter: &Painter, rect: Rect, style: &Style<'_>) -> (Painter, Rect) {
    rule(painter, rect.min.x, rect.max.x, rect.min.y, style);
    let inner = painter.with_clip_rect(rect.expand2(vec2(0.0, 3.0)));
    (inner, rect.shrink2(vec2(PADDING, PADDING)))
}

/// A column's heading: name at one end, section at the other, rule below.
pub fn column_header(painter: &Painter, rect: Rect, column: Column, style: &Style<'_>) {
    let section = match column {
        Column::Left => "SYSTEM",
        Column::Right => "NETWORK",
    };
    let mid = rect.center().y;
    let font = style.caption();
    let colour = style.theme.foreground;
    painter.text(pos2(rect.min.x + PADDING, mid), Align2::LEFT_CENTER, "PANEL", font.clone(), colour);
    painter.text(pos2(rect.max.x - PADDING, mid), Align2::RIGHT_CENTER, section, font, colour);
}

/// What one slot in the tab bar shows.
#[derive(Clone, Debug)]
pub struct TabLabel {
    pub text: String,
    pub open: bool,
    pub active: bool,
    pub closable: bool,
}

/// Where the close mark sits in a tab, if the tab is wide enough to hold one
/// beside its label. Drawing and clicking both use this, so a tab never has an
/// invisible close button. Ctrl+Shift+W always works.
pub fn tab_close_rect(tab: Rect) -> Option<Rect> {
    (tab.width() >= TAB_CLOSE_MIN_WIDTH).then(|| {
        Rect::from_min_max(
            pos2(tab.max.x - tab.height() - TAB_SLANT, tab.min.y),
            pos2(tab.max.x - TAB_SLANT * 0.5, tab.max.y),
        )
    })
}

/// The terminal and its tabs as one outline, with cut corners at the top right
/// and bottom left. `tabs` run along the top of `outer`.
pub fn terminal_frame(painter: &Painter, outer: Rect, tabs: &[Rect], labels: &[TabLabel], style: &Style<'_>) {
    let theme = style.theme;
    let points = vec![
        outer.left_top(),
        pos2(outer.max.x - CHAMFER, outer.min.y),
        pos2(outer.max.x, outer.min.y + CHAMFER),
        outer.right_bottom(),
        pos2(outer.min.x + CHAMFER, outer.max.y),
        pos2(outer.min.x, outer.max.y - CHAMFER),
    ];
    painter.add(Shape::convex_polygon(points.clone(), theme.terminal.background, Stroke::NONE));
    glowing_path(painter, points, true, style);

    let Some(bottom) = tabs.first().map(|tab| tab.max.y) else { return };
    painter.hline(outer.x_range(), bottom, Stroke::new(1.0, theme.rule));

    for (i, (rect, label)) in tabs.iter().zip(labels).enumerate() {
        let last = i + 1 == tabs.len();
        // Each tab's right edge leans out towards the bottom.
        let edge_top = pos2(rect.max.x - TAB_SLANT, rect.min.y);
        let edge_bottom = pos2(rect.max.x, rect.max.y);
        if label.active {
            let left_top = if i == 0 { rect.left_top() } else { pos2(rect.min.x - TAB_SLANT, rect.min.y) };
            let shape = vec![left_top, edge_top, edge_bottom, rect.left_bottom()];
            painter.add(Shape::convex_polygon(shape, theme.foreground, Stroke::NONE));
        }
        if !last {
            painter.line_segment([edge_top, edge_bottom], Stroke::new(1.0, theme.rule));
        }
        let colour = match (label.active, label.open) {
            (true, _) => theme.terminal.background,
            (false, true) => theme.foreground,
            (false, false) => theme.muted,
        };
        let clip = painter.with_clip_rect(*rect);
        let centre = rect.center() - vec2(TAB_SLANT * 0.5, 0.0);
        clip.text(centre, Align2::CENTER_CENTER, &label.text, style.heading(), colour);
        if let Some(close) = tab_close_rect(*rect).filter(|_| label.closable) {
            clip.text(close.center(), Align2::CENTER_CENTER, "x", style.heading(), colour);
        }
    }
}

/// Text standing in for a panel that a later phase makes live.
pub fn placeholder(painter: &Painter, id: PanelId, rect: Rect, style: &Style<'_>) {
    let (inner, content) = if id == PanelId::Keyboard {
        // The keyboard has no heading or rule in the final design.
        (painter.with_clip_rect(rect), rect)
    } else {
        titled(painter, rect, id.title(), None, style)
    };
    let wrap = content.width();
    let galley = inner.layout(id.plan().to_owned(), style.body(), style.theme.muted, wrap);
    // Centred, but never above the content area in a short panel.
    let at = (content.center() - galley.size() * 0.5).max(content.min);
    inner.galley(at, galley, style.theme.muted);
}

/// The large clock, with digits in even cells so it never shifts as they
/// change. It shrinks only if a narrow column would otherwise clip it.
pub fn clock(painter: &Painter, rect: Rect, text: &str, suffix: Option<&str>, style: &Style<'_>) {
    const SPACING: f32 = 0.74;
    const COLON: f32 = 0.5;
    let (inner, area) = ruled(painter, rect, style);
    let suffix_width = suffix.map_or(0.0, |_| style.fonts.label * 2.0);
    let units: f32 = text.chars().map(|c| if c == ':' { COLON } else { SPACING }).sum();
    let available = area.width() - suffix_width;
    let size = style.fonts.clock.min(available / units).min(area.height() * 1.15);
    let family = FontFamily::Name(DISPLAY_FONT.into());
    let colour = style.theme.foreground;

    let mut x = area.min.x + (available - units * size) * 0.5;
    let y = area.center().y;
    for c in text.chars() {
        let width = size * if c == ':' { COLON } else { SPACING };
        let font = FontId::new(size, family.clone());
        inner.text(pos2(x + width * 0.5, y), Align2::CENTER_CENTER, c, font, colour);
        x += width;
    }
    if let Some(suffix) = suffix {
        let at = pos2(x + 4.0, y + size * 0.3);
        inner.text(at, Align2::LEFT_BOTTOM, suffix, style.heading(), colour);
    }
}
