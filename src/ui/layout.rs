//! Works out where every panel goes. Pure geometry with no drawing, so the
//! rules can be unit tested at any window size.

use egui::{pos2, vec2, Pos2, Rect, Vec2};

use crate::config::{FontSizes, Panels};

pub const MIN_WINDOW: Vec2 = vec2(1024.0, 680.0);
pub const TAB_COUNT: usize = 5;

const MARGIN: f32 = 8.0;
const GAP: f32 = 10.0;
/// Space between the side columns and the terminal.
const COLUMN_GAP: f32 = 16.0;

/// Share of the window's width given to each side column.
const SIDE_COLUMN_SHARE: f32 = 0.16;
const SIDE_COLUMN_MIN: f32 = 250.0;
const SIDE_COLUMN_MAX: f32 = 460.0;

/// Share of the window's height given to the bottom strip.
const BOTTOM_SHARE: f32 = 0.33;
/// Below this window height the columns need the room more than the strip.
const SHORT_WINDOW: f32 = 800.0;
const BOTTOM_SHARE_SHORT: f32 = 0.27;
/// Share of the bottom strip's width given to the file browser.
const FILES_SHARE: f32 = 0.445;
/// Width of the first tab in characters of the heading font, so that
/// MAIN SHELL always fits with its slanted edge.
const FIRST_TAB_MIN_CHARS: f32 = 15.0;

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
    fn shown(self, panels: &Panels) -> bool {
        match self {
            Self::Clock => panels.clock,
            Self::System => panels.system,
            Self::Hardware => panels.hardware,
            Self::Cpu => panels.cpu,
            Self::Memory => panels.memory,
            Self::Processes => panels.processes,
            Self::Terminal => true,
            Self::Files => panels.files,
            Self::Keyboard => panels.keyboard,
            Self::Network => panels.network,
            Self::Globe => panels.globe,
            Self::Traffic => panels.traffic,
        }
    }

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
}

#[derive(Clone, Debug)]
pub struct Layout {
    pub left_header: Rect,
    pub right_header: Rect,
    pub tabs: Vec<Rect>,
    /// The outline around the tabs and terminal.
    pub terminal_frame: Rect,
    pub panels: Vec<(PanelId, Rect)>,
}

/// A stacked panel's smallest useful height and its share of any spare height.
struct Slot {
    id: PanelId,
    min: f32,
    weight: f32,
}

pub fn compute(window: Rect, fonts: &FontSizes, shown: &Panels) -> Layout {
    let area = window.shrink(MARGIN);
    let label_row = fonts.label * 1.5;
    let body_row = fonts.body * 1.3;
    let caption_row = fonts.label * 1.25;
    let panel_top = label_row + 8.0;

    let left_slots: Vec<Slot> = [
        Slot { id: PanelId::Clock, min: fonts.clock * 0.77, weight: 0.3 },
        Slot { id: PanelId::System, min: caption_row + body_row + 6.0, weight: 0.1 },
        Slot { id: PanelId::Hardware, min: caption_row + body_row + 6.0, weight: 0.1 },
        Slot { id: PanelId::Cpu, min: panel_top + caption_row + body_row + 40.0, weight: 2.0 },
        Slot { id: PanelId::Memory, min: panel_top + caption_row + 28.0, weight: 0.8 },
        Slot { id: PanelId::Processes, min: panel_top + fonts.label * 2.8, weight: 4.0 },
    ]
    .into_iter()
    .filter(|slot| slot.id.shown(shown))
    .collect();
    let right_slots: Vec<Slot> = [
        Slot { id: PanelId::Network, min: panel_top + caption_row * 2.0 + body_row, weight: 0.0 },
        Slot { id: PanelId::Globe, min: panel_top + caption_row + body_row * 4.0, weight: 3.0 },
        Slot { id: PanelId::Traffic, min: panel_top + caption_row + body_row * 3.0, weight: 2.0 },
    ]
    .into_iter()
    .filter(|slot| slot.id.shown(shown))
    .collect();

    // An empty column takes no width, and an empty strip no height, so the
    // terminal grows into the space.
    let side = (window.width() * SIDE_COLUMN_SHARE).clamp(SIDE_COLUMN_MIN, SIDE_COLUMN_MAX);
    let left_width = if left_slots.is_empty() { 0.0 } else { side };
    let right_width = if right_slots.is_empty() { 0.0 } else { side };
    let left_gap = if left_slots.is_empty() { 0.0 } else { COLUMN_GAP };
    let right_gap = if right_slots.is_empty() { 0.0 } else { COLUMN_GAP };
    let has_strip = shown.files || shown.keyboard;
    let share = if window.height() < SHORT_WINDOW { BOTTOM_SHARE_SHORT } else { BOTTOM_SHARE };
    let bottom_height = if has_strip { window.height() * share + GAP } else { 0.0 };

    // The side columns and terminal share the top; the strip spans the window.
    let top = Rect::from_min_max(area.min, pos2(area.max.x, area.max.y - bottom_height));
    let bottom = Rect::from_min_max(pos2(area.min.x, top.max.y + GAP), area.max);

    let left = Rect::from_min_size(top.min, vec2(left_width, top.height()));
    let right = Rect::from_min_size(pos2(top.max.x - right_width, top.min.y), vec2(right_width, top.height()));
    let (left_header, left_body) = split_top(left, label_row);
    let (right_header, right_body) = split_top(right, label_row);

    let mut panels = Vec::with_capacity(12);
    stack(left_body, &left_slots, &mut panels);
    stack(right_body, &right_slots, &mut panels);

    let terminal_frame = Rect::from_min_max(
        pos2(left.max.x + left_gap, left_body.min.y),
        pos2(right.min.x - right_gap, top.max.y),
    );
    let tab_height = fonts.label * 1.9;
    let tab_strip = Rect::from_min_size(terminal_frame.min, vec2(terminal_frame.width(), tab_height));
    let terminal = Rect::from_min_max(pos2(terminal_frame.min.x, tab_strip.max.y + 1.0), terminal_frame.max);
    panels.push((PanelId::Terminal, terminal));

    match (shown.files, shown.keyboard) {
        (true, true) => {
            let files = Rect::from_min_size(bottom.min, vec2(bottom.width() * FILES_SHARE, bottom.height()));
            let keyboard = Rect::from_min_max(pos2(files.max.x + COLUMN_GAP, bottom.min.y), bottom.max);
            panels.push((PanelId::Files, files));
            panels.push((PanelId::Keyboard, keyboard));
        }
        (true, false) => panels.push((PanelId::Files, bottom)),
        (false, true) => panels.push((PanelId::Keyboard, bottom)),
        (false, false) => {}
    }

    let first_tab = FIRST_TAB_MIN_CHARS * fonts.label * 0.6;
    Layout {
        left_header,
        right_header,
        tabs: tab_slots(tab_strip, first_tab),
        terminal_frame,
        panels,
    }
}

/// Cut a strip off the top of `rect`, leaving a gap before the remainder.
fn split_top(rect: Rect, height: f32) -> (Rect, Rect) {
    let top = Rect::from_min_size(rect.min, vec2(rect.width(), height));
    let rest = Rect::from_min_max(pos2(rect.min.x, top.max.y + 4.0), rect.max);
    (top, rest)
}

/// Equal tabs, except that the first never gets too narrow for its label.
fn tab_slots(strip: Rect, first_min: f32) -> Vec<Rect> {
    let equal = strip.width() / TAB_COUNT as f32;
    let first = equal.max(first_min);
    let others = (strip.width() - first) / (TAB_COUNT as f32 - 1.0);
    let mut x = strip.min.x;
    (0..TAB_COUNT)
        .map(|i| {
            let width = if i == 0 { first } else { others };
            let rect = Rect::from_min_size(pos2(x, strip.min.y), vec2(width, strip.height()));
            x += width;
            rect
        })
        .collect()
}

/// Stack slots down `column`. Each gets its minimum, then a weighted share of
/// what remains. If the minimums do not fit, all shrink in proportion so that
/// panels never overlap or leave the window.
fn stack(column: Rect, slots: &[Slot], out: &mut Vec<(PanelId, Rect)>) {
    if slots.is_empty() {
        return;
    }
    let gaps = GAP * (slots.len() - 1) as f32;
    let available = (column.height() - gaps).max(0.0);
    let min_total: f32 = slots.iter().map(|s| s.min).sum();
    let weight_total: f32 = slots.iter().map(|s| s.weight).sum::<f32>().max(0.01);
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
        (1024.0, 680.0),
        (1280.0, 720.0),
        (1366.0, 768.0),
        (1600.0, 900.0),
        (1920.0, 1080.0),
        (2560.0, 1440.0),
        (3840.0, 2160.0),
    ];

    fn layout_at(size: (f32, f32)) -> (Rect, Layout) {
        let window = Rect::from_min_size(Pos2::ZERO, vec2(size.0, size.1));
        (window, compute(window, &Config::default().fonts, &Panels::default()))
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
                assert!(rect.width() > 50.0 && rect.height() > 28.0, "{id:?} tiny at {size:?}");
                assert!(window.contains_rect(*rect), "{id:?} escapes at {size:?}: {rect:?}");
            }
        }
    }

    #[test]
    fn the_bottom_strip_spans_the_window_under_all_three_columns() {
        for size in SIZES {
            let (window, layout) = layout_at(size);
            let files = panel(&layout, PanelId::Files);
            let keyboard = panel(&layout, PanelId::Keyboard);
            assert_eq!(files.min.x, window.min.x + MARGIN);
            assert_eq!(keyboard.max.x, window.max.x - MARGIN);
            for id in [PanelId::Processes, PanelId::Traffic, PanelId::Terminal] {
                assert!(panel(&layout, id).max.y <= files.min.y, "{id:?} at {size:?}");
            }
        }
    }

    #[test]
    fn the_terminal_sits_inside_its_frame_below_the_tabs() {
        for size in SIZES {
            let (_, layout) = layout_at(size);
            let terminal = panel(&layout, PanelId::Terminal);
            assert!(layout.terminal_frame.contains_rect(terminal));
            assert!(layout.tabs.iter().all(|tab| tab.max.y <= terminal.min.y));
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
            let mut rects: Vec<Rect> =
                layout.panels.iter().filter(|p| p.0 != PanelId::Terminal).map(|p| p.1).collect();
            rects.push(layout.terminal_frame);
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

    fn with(change: impl Fn(&mut Panels)) -> (Rect, Layout) {
        let window = Rect::from_min_size(Pos2::ZERO, vec2(1600.0, 900.0));
        let mut panels = Panels::default();
        change(&mut panels);
        (window, compute(window, &Config::default().fonts, &panels))
    }

    #[test]
    fn a_hidden_panel_gives_its_space_to_its_column() {
        let (_, all) = layout_at((1600.0, 900.0));
        let (_, fewer) = with(|p| p.memory = false);
        assert!(fewer.panels.iter().all(|(id, _)| *id != PanelId::Memory));
        assert!(panel(&fewer, PanelId::Cpu).height() > panel(&all, PanelId::Cpu).height());
    }

    #[test]
    fn hiding_a_whole_column_widens_the_terminal() {
        let (_, all) = layout_at((1600.0, 900.0));
        let (_, fewer) = with(|p| {
            p.network = false;
            p.globe = false;
            p.traffic = false;
        });
        assert!(panel(&fewer, PanelId::Terminal).width() > panel(&all, PanelId::Terminal).width() + 200.0);
    }

    #[test]
    fn hiding_the_strip_makes_the_terminal_taller() {
        let (window, fewer) = with(|p| {
            p.files = false;
            p.keyboard = false;
        });
        assert!(panel(&fewer, PanelId::Terminal).max.y > window.max.y - 12.0);
    }

    #[test]
    fn one_strip_panel_takes_the_whole_width() {
        let (window, fewer) = with(|p| p.files = false);
        assert_eq!(panel(&fewer, PanelId::Keyboard).width(), window.width() - MARGIN * 2.0);
    }

    #[test]
    fn hiding_everything_leaves_a_full_window_terminal() {
        let (window, only) = with(|p| *p = Panels {
            clock: false,
            system: false,
            hardware: false,
            cpu: false,
            memory: false,
            processes: false,
            network: false,
            globe: false,
            traffic: false,
            files: false,
            keyboard: false,
        });
        assert_eq!(only.panels.len(), 1);
        assert!(panel(&only, PanelId::Terminal).width() > window.width() - 20.0);
    }

    #[test]
    fn left_column_minimums_fit_the_smallest_window() {
        let (_, layout) = layout_at((MIN_WINDOW.x, MIN_WINDOW.y));
        let cpu = panel(&layout, PanelId::Cpu);
        assert!(cpu.height() >= 95.0, "CPU panel crushed to {}", cpu.height());
        assert!(panel(&layout, PanelId::Processes).height() >= 68.0);
    }

    #[test]
    fn bigger_fonts_shrink_panels_rather_than_breaking_the_layout() {
        let window = Rect::from_min_size(Pos2::ZERO, MIN_WINDOW);
        let fonts = FontSizes { label: 30.0, body: 34.0, clock: 104.0, terminal: 32.0 };
        let layout = compute(window, &fonts, &Panels::default());
        for (id, rect) in &layout.panels {
            assert!(window.contains_rect(*rect), "{id:?} escapes: {rect:?}");
        }
    }
}
