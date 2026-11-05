//! Works out where every panel goes. Pure geometry with no drawing, so the
//! rules can be unit tested at any window size.

use egui::{pos2, vec2, Pos2, Rect, Vec2};

use crate::config::FontSizes;

pub const MIN_WINDOW: Vec2 = vec2(1024.0, 640.0);
pub const TAB_COUNT: usize = 5;

const MARGIN: f32 = 10.0;
const GAP: f32 = 8.0;

/// Share of the window's width given to each side column.
const SIDE_COLUMN_SHARE: f32 = 0.18;
const SIDE_COLUMN_MIN: f32 = 240.0;
const SIDE_COLUMN_MAX: f32 = 460.0;

/// Share of the centre's height given to the file browser and keyboard.
const BOTTOM_SHARE: f32 = 0.31;
/// Share of the bottom strip's width given to the file browser.
const FILES_SHARE: f32 = 0.42;
/// Width of the first tab relative to the others.
const FIRST_TAB_SCALE: f32 = 1.4;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Column {
    Left,
    Right,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PanelId {
    Clock,
    System,
    Hardware,
    Cpu,
    Memory,
    Processes,
    Terminal,
    Files,
    Keyboard,
    Network,
    Globe,
    Traffic,
}

impl PanelId {
    pub fn title(self) -> &'static str {
        match self {
            Self::Clock => "CLOCK",
            Self::System => "SYSTEM",
            Self::Hardware => "HARDWARE",
            Self::Cpu => "CPU USAGE",
            Self::Memory => "MEMORY",
            Self::Processes => "TOP PROCESSES",
            Self::Terminal => "TERMINAL",
            Self::Files => "FILESYSTEM",
            Self::Keyboard => "KEYBOARD",
            Self::Network => "NETWORK STATUS",
            Self::Globe => "WORLD VIEW",
            Self::Traffic => "NETWORK TRAFFIC",
        }
    }

    /// What this panel will show, printed inside its placeholder.
    pub fn plan(self) -> &'static str {
        match self {
            Self::Clock => "",
            Self::System => "DATE / UPTIME / OS / POWER",
            Self::Hardware => "MANUFACTURER / MODEL / CHASSIS",
            Self::Cpu => "PER-CORE GRAPHS IN TWO GROUPS / TEMP / FREQ / TASKS",
            Self::Memory => "DOT GRID + SWAP BAR",
            Self::Processes => "PID / NAME / CPU% / MEM%",
            Self::Terminal => "SHELL SESSION",
            Self::Files => "CURRENT DIRECTORY OF ACTIVE TERMINAL",
            Self::Keyboard => "LIVE KEY HIGHLIGHT",
            Self::Network => "STATE / INTERFACE / IPV4 / EXTERNAL IP / LATENCY",
            Self::Globe => "WIREFRAME GLOBE + CONNECTION POINTS",
            Self::Traffic => "UP/DOWN GRAPH + TOTALS",
        }
    }
}

#[derive(Clone, Debug)]
pub struct Layout {
    pub left_header: Rect,
    pub right_header: Rect,
    pub tabs: Vec<Rect>,
    pub panels: Vec<(PanelId, Rect)>,
}

/// A stacked panel's smallest useful height and its share of any spare height.
struct Slot {
    id: PanelId,
    min: f32,
    weight: f32,
}

pub fn compute(window: Rect, fonts: &FontSizes) -> Layout {
    let area = window.shrink(MARGIN);
    let side = (window.width() * SIDE_COLUMN_SHARE).clamp(SIDE_COLUMN_MIN, SIDE_COLUMN_MAX);
    let label_row = fonts.label * 1.6;
    let body_row = fonts.body * 1.35;
    let panel_top = label_row + 6.0;

    let left = Rect::from_min_size(area.min, vec2(side, area.height()));
    let right = Rect::from_min_size(pos2(area.max.x - side, area.min.y), vec2(side, area.height()));
    let centre = Rect::from_min_max(
        pos2(left.max.x + GAP, area.min.y),
        pos2(right.min.x - GAP, area.max.y),
    );

    let (left_header, left_body) = split_top(left, label_row);
    let (right_header, right_body) = split_top(right, label_row);

    let mut panels = Vec::with_capacity(12);
    stack(
        left_body,
        &[
            Slot { id: PanelId::Clock, min: fonts.clock * 1.25, weight: 0.5 },
            Slot { id: PanelId::System, min: panel_top + body_row * 2.0, weight: 0.5 },
            Slot { id: PanelId::Hardware, min: panel_top + body_row * 2.0, weight: 0.5 },
            Slot { id: PanelId::Cpu, min: panel_top + body_row * 4.0, weight: 3.5 },
            Slot { id: PanelId::Memory, min: panel_top + body_row * 3.0, weight: 2.0 },
            Slot { id: PanelId::Processes, min: panel_top + body_row * 3.0, weight: 3.0 },
        ],
        &mut panels,
    );
    stack(
        right_body,
        &[
            Slot { id: PanelId::Network, min: panel_top + body_row * 4.0, weight: 1.0 },
            Slot { id: PanelId::Globe, min: panel_top + body_row * 5.0, weight: 3.0 },
            Slot { id: PanelId::Traffic, min: panel_top + body_row * 4.0, weight: 1.5 },
        ],
        &mut panels,
    );

    let tab_height = fonts.label * 2.2;
    let (tab_strip, below_tabs) = split_top(centre, tab_height);
    let bottom_height = centre.height() * BOTTOM_SHARE;
    let terminal = Rect::from_min_max(
        below_tabs.min,
        pos2(below_tabs.max.x, below_tabs.max.y - bottom_height - GAP),
    );
    let bottom = Rect::from_min_max(
        pos2(centre.min.x, terminal.max.y + GAP),
        centre.max,
    );
    let files_width = bottom.width() * FILES_SHARE;
    let files = Rect::from_min_size(bottom.min, vec2(files_width, bottom.height()));
    let keyboard = Rect::from_min_max(pos2(files.max.x + GAP, bottom.min.y), bottom.max);

    panels.push((PanelId::Terminal, terminal));
    panels.push((PanelId::Files, files));
    panels.push((PanelId::Keyboard, keyboard));

    Layout {
        left_header,
        right_header,
        tabs: tab_slots(tab_strip),
        panels,
    }
}

/// Cut a strip off the top of `rect`, leaving a gap before the remainder.
fn split_top(rect: Rect, height: f32) -> (Rect, Rect) {
    let top = Rect::from_min_size(rect.min, vec2(rect.width(), height));
    let rest = Rect::from_min_max(pos2(rect.min.x, top.max.y + GAP), rect.max);
    (top, rest)
}

/// The first tab is wider because "MAIN SHELL" is the longest label.
fn tab_slots(strip: Rect) -> Vec<Rect> {
    let gaps = GAP * (TAB_COUNT - 1) as f32;
    let unit = (strip.width() - gaps) / (TAB_COUNT as f32 - 1.0 + FIRST_TAB_SCALE);
    let mut x = strip.min.x;
    (0..TAB_COUNT)
        .map(|i| {
            let width = if i == 0 { unit * FIRST_TAB_SCALE } else { unit };
            let rect = Rect::from_min_size(pos2(x, strip.min.y), vec2(width, strip.height()));
            x += width + GAP;
            rect
        })
        .collect()
}

/// Stack slots down `column`. Each gets its minimum, then a weighted share of
/// what remains. If the minimums do not fit, all shrink in proportion so that
/// panels never overlap or leave the window.
fn stack(column: Rect, slots: &[Slot], out: &mut Vec<(PanelId, Rect)>) {
    let gaps = GAP * (slots.len() - 1) as f32;
    let available = (column.height() - gaps).max(0.0);
    let min_total: f32 = slots.iter().map(|s| s.min).sum();
    let weight_total: f32 = slots.iter().map(|s| s.weight).sum();
    let spare = (available - min_total).max(0.0);
    let shrink = (available / min_total).min(1.0);

    let mut y = column.min.y;
    for slot in slots {
        let height = slot.min * shrink + spare * slot.weight / weight_total;
        let min = Pos2::new(column.min.x, y);
        out.push((slot.id, Rect::from_min_size(min, vec2(column.width(), height))));
        y += height + GAP;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    const SIZES: [(f32, f32); 7] = [
        (1024.0, 640.0),
        (1280.0, 720.0),
        (1366.0, 768.0),
        (1600.0, 900.0),
        (1920.0, 1080.0),
        (2560.0, 1440.0),
        (3840.0, 2160.0),
    ];

    fn layout_at(size: (f32, f32)) -> (Rect, Layout) {
        let window = Rect::from_min_size(Pos2::ZERO, vec2(size.0, size.1));
        (window, compute(window, &Config::default().fonts))
    }

    fn panel(layout: &Layout, id: PanelId) -> Rect {
        layout.panels.iter().find(|(p, _)| *p == id).unwrap().1
    }

    #[test]
    fn every_panel_exists_once_and_stays_in_the_window() {
        for size in SIZES {
            let (window, layout) = layout_at(size);
            assert_eq!(layout.panels.len(), 12);
            for (id, rect) in &layout.panels {
                assert!(rect.width() > 50.0 && rect.height() > 30.0, "{id:?} tiny at {size:?}");
                assert!(window.contains_rect(*rect), "{id:?} escapes at {size:?}: {rect:?}");
            }
        }
    }

    #[test]
    fn tabs_span_the_terminal_exactly() {
        for size in SIZES {
            let (_, layout) = layout_at(size);
            let terminal = panel(&layout, PanelId::Terminal);
            assert_eq!(layout.tabs.len(), TAB_COUNT);
            assert!((layout.tabs[0].min.x - terminal.min.x).abs() < 0.01);
            assert!((layout.tabs[TAB_COUNT - 1].max.x - terminal.max.x).abs() < 0.01);
        }
    }

    #[test]
    fn nothing_overlaps() {
        for size in SIZES {
            let (_, layout) = layout_at(size);
            let mut rects: Vec<Rect> = layout.panels.iter().map(|p| p.1).collect();
            rects.extend(&layout.tabs);
            rects.push(layout.left_header);
            rects.push(layout.right_header);
            for (i, a) in rects.iter().enumerate() {
                for b in &rects[i + 1..] {
                    assert!(!a.intersects(*b), "overlap at {size:?}: {a:?} {b:?}");
                }
            }
        }
    }

    #[test]
    fn terminal_is_the_largest_panel() {
        for size in SIZES {
            let (_, layout) = layout_at(size);
            let terminal = panel(&layout, PanelId::Terminal).area();
            for (id, rect) in &layout.panels {
                assert!(*id == PanelId::Terminal || rect.area() < terminal, "{id:?} at {size:?}");
            }
        }
    }

    #[test]
    fn columns_hold_the_panels_in_the_requested_order() {
        let (_, layout) = layout_at((1600.0, 900.0));
        let order = |ids: &[PanelId]| {
            ids.windows(2).all(|w| panel(&layout, w[0]).max.y <= panel(&layout, w[1]).min.y)
        };
        assert!(order(&[
            PanelId::Clock,
            PanelId::System,
            PanelId::Hardware,
            PanelId::Cpu,
            PanelId::Memory,
            PanelId::Processes,
        ]));
        assert!(order(&[PanelId::Network, PanelId::Globe, PanelId::Traffic]));
        assert!(panel(&layout, PanelId::Files).max.x <= panel(&layout, PanelId::Keyboard).min.x);
        assert!(panel(&layout, PanelId::Terminal).max.y <= panel(&layout, PanelId::Files).min.y);
    }

    #[test]
    fn bigger_fonts_shrink_panels_rather_than_breaking_the_layout() {
        let window = Rect::from_min_size(Pos2::ZERO, MIN_WINDOW);
        let fonts = FontSizes { label: 30.0, body: 34.0, clock: 104.0, terminal: 32.0 };
        let layout = compute(window, &fonts);
        for (id, rect) in &layout.panels {
            assert!(window.contains_rect(*rect), "{id:?} escapes: {rect:?}");
        }
    }
}
