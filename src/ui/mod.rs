//! All drawing. Nothing here uses egui's stock widgets: every shape goes
//! through the painter so the look is fully ours.

pub mod boot;
mod boot_lines;
pub mod files;
pub mod keyboard;
pub mod layout;
pub mod monitor;
pub mod network;
pub mod panel;
pub mod terminal;

use egui::Painter;

use layout::{Column, Layout};
use panel::Style;

/// The backdrop and column headings.
pub fn draw_panels(painter: &Painter, layout: &Layout, style: &Style<'_>) {
    painter.add(panel::backdrop(painter.clip_rect(), style.theme.backdrop));
    for (rect, column) in [(layout.left_header, Column::Left), (layout.right_header, Column::Right)] {
        if rect.width() > 1.0 {
            panel::column_header(painter, rect, column, style);
        }
    }
}
