//! The shared panel frame and, for now, the placeholder contents.

use egui::{pos2, vec2, Align2, Color32, CornerRadius, FontId, Painter, Pos2, Rect, Shape, Stroke, StrokeKind};

use super::layout::{Column, PanelId};
use crate::config::FontSizes;
use crate::theme::Theme;

const PADDING: f32 = 10.0;
const CORNER: f32 = 9.0;

pub struct Style<'a> {
    pub theme: &'a Theme,
    pub fonts: &'a FontSizes,
}

impl Style<'_> {
    pub fn label(&self) -> FontId {
        FontId::monospace(self.fonts.label)
    }

    fn body(&self) -> FontId {
        FontId::monospace(self.fonts.body)
    }
}

/// A thin outline with a soft halo and brighter corner brackets.
pub fn frame(painter: &Painter, rect: Rect, style: &Style<'_>) {
    let theme = style.theme;
    painter.rect_filled(rect, CornerRadius::ZERO, theme.panel);
    for width in [7.0, 4.0, 2.0] {
        painter.rect_stroke(
            rect,
            CornerRadius::ZERO,
            Stroke::new(width, theme.glow),
            StrokeKind::Middle,
        );
    }
    painter.rect_stroke(
        rect,
        CornerRadius::ZERO,
        Stroke::new(1.0, theme.line),
        StrokeKind::Inside,
    );
    corners(painter, rect, theme.foreground);
}

fn corners(painter: &Painter, rect: Rect, colour: Color32) {
    let stroke = Stroke::new(1.5, colour);
    for (corner, dx, dy) in [
        (rect.left_top(), CORNER, CORNER),
        (rect.right_top(), -CORNER, CORNER),
        (rect.left_bottom(), CORNER, -CORNER),
        (rect.right_bottom(), -CORNER, -CORNER),
    ] {
        let points = vec![corner + vec2(dx, 0.0), corner, corner + vec2(0.0, dy)];
        painter.add(Shape::line(points, stroke));
    }
}

fn title(painter: &Painter, rect: Rect, text: &str, style: &Style<'_>) {
    let at = rect.left_top() + vec2(PADDING, PADDING * 0.7);
    painter.text(at, Align2::LEFT_TOP, text, style.label(), style.theme.foreground);
}

/// A column's heading: name at one end, rule running to the other.
pub fn column_header(painter: &Painter, rect: Rect, column: Column, style: &Style<'_>) {
    let (name, section) = match column {
        Column::Left => ("PANEL", "SYSTEM"),
        Column::Right => ("PANEL", "NETWORK"),
    };
    let theme = style.theme;
    let mid = rect.center().y;
    painter.text(pos2(rect.min.x, mid), Align2::LEFT_CENTER, name, style.label(), theme.muted);
    painter.text(pos2(rect.max.x, mid), Align2::RIGHT_CENTER, section, style.label(), theme.foreground);
    painter.hline(rect.x_range(), rect.max.y, Stroke::new(1.0, theme.line));
}

/// What one slot in the tab bar shows.
#[derive(Clone, Debug)]
pub struct TabLabel {
    pub text: String,
    pub open: bool,
    pub active: bool,
    pub closable: bool,
}

/// Where the close mark sits inside a tab.
pub fn tab_close_rect(tab: Rect) -> Rect {
    Rect::from_min_max(pos2(tab.max.x - tab.height(), tab.min.y), tab.max)
}

pub fn tab_bar(painter: &Painter, tabs: &[Rect], labels: &[TabLabel], style: &Style<'_>) {
    let theme = style.theme;
    for (rect, label) in tabs.iter().zip(labels) {
        let colour = if label.active { theme.foreground } else { theme.muted };
        if label.active {
            frame(painter, *rect, style);
        } else {
            let line = if label.open { theme.muted } else { theme.line };
            painter.rect_stroke(*rect, CornerRadius::ZERO, Stroke::new(1.0, line), StrokeKind::Inside);
        }
        let clip = painter.with_clip_rect(*rect);
        clip.text(rect.center(), Align2::CENTER_CENTER, &label.text, style.label(), colour);
        if label.closable {
            let at = tab_close_rect(*rect).center();
            clip.text(at, Align2::CENTER_CENTER, "x", style.label(), theme.muted);
        }
    }
}

pub fn placeholder(painter: &Painter, id: PanelId, rect: Rect, style: &Style<'_>) {
    frame(painter, rect, style);
    let inner = painter.with_clip_rect(rect.shrink(2.0));
    title(&inner, rect, id.title(), style);

    let wrap = rect.width() - PADDING * 2.0;
    let galley = inner.layout(id.plan().to_owned(), style.body(), style.theme.muted, wrap);
    let top: Pos2 = rect.center() - galley.size() * 0.5 + vec2(0.0, style.fonts.label * 0.7);
    inner.galley(top, galley, style.theme.muted);

    inner.text(
        rect.right_bottom() - vec2(PADDING, PADDING * 0.5),
        Align2::RIGHT_BOTTOM,
        format!("{:.0}x{:.0}", rect.width(), rect.height()),
        FontId::monospace(style.fonts.label * 0.8),
        style.theme.line,
    );
}

/// The clock has no title in its frame; it shows blanks at its real size so
/// the proportions can be judged before phase 3 makes it live.
pub fn clock_placeholder(painter: &Painter, rect: Rect, style: &Style<'_>) {
    const TEXT: &str = "--:--:--";
    frame(painter, rect, style);
    let inner = painter.with_clip_rect(rect.shrink(2.0));

    // Measure at the configured size, then shrink only if a narrow column
    // would otherwise push the digits into the frame.
    let font = FontId::monospace(style.fonts.clock);
    let measured = inner.layout_no_wrap(TEXT.to_owned(), font, style.theme.foreground).size().x;
    let available = rect.width() - PADDING * 2.0;
    let size = style.fonts.clock * (available / measured).min(1.0);

    inner.text(
        rect.center(),
        Align2::CENTER_CENTER,
        TEXT,
        FontId::monospace(size),
        style.theme.foreground,
    );
}
