//! All drawing. Nothing here uses egui's stock widgets: every shape goes
//! through the painter so the look is fully ours.

pub mod layout;
pub mod monitor;
pub mod panel;
pub mod terminal;

use egui::Painter;

use layout::{Column, Layout, PanelId};
use panel::Style;

/// The panels that are not live yet, and the column headings.
pub fn draw_panels(painter: &Painter, layout: &Layout, style: &Style<'_>) {
    painter.add(panel::backdrop(painter.clip_rect(), style.theme.backdrop));
    panel::column_header(painter, layout.left_header, Column::Left, style);
    panel::column_header(painter, layout.right_header, Column::Right, style);
    for (id, rect) in &layout.panels {
        if matches!(id, PanelId::Files | PanelId::Keyboard | PanelId::Network | PanelId::Globe | PanelId::Traffic) {
            panel::placeholder(painter, *id, *rect, style);
        }
    }
}
