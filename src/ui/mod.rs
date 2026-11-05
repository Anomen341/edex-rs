//! All drawing. Nothing here uses egui's stock widgets: every shape goes
//! through the painter so the look is fully ours.

pub mod layout;
pub mod panel;
pub mod terminal;

use egui::Painter;

use layout::{Column, Layout, PanelId};
use panel::Style;

/// Everything around the terminal.
pub fn draw_panels(painter: &Painter, layout: &Layout, style: &Style<'_>) {
    panel::column_header(painter, layout.left_header, Column::Left, style);
    panel::column_header(painter, layout.right_header, Column::Right, style);
    for (id, rect) in &layout.panels {
        match id {
            PanelId::Clock => panel::clock_placeholder(painter, *rect, style),
            PanelId::Terminal => {}
            _ => panel::placeholder(painter, *id, *rect, style),
        }
    }
}
