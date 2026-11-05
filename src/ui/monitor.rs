//! The live left column. Each panel draws from the latest snapshot and falls
//! back to N/A for anything missing, at any panel size.

use std::time::{Duration, Instant};

use chrono::{DateTime, Local};
use egui::{pos2, vec2, Align2, FontId, Painter, Rect, Shape, Stroke};

use super::layout::{Layout, PanelId};
use super::panel::{self, Style};
use crate::stats::format::{self, NOT_AVAILABLE};
use crate::stats::{CpuStats, MemoryStats, ProcessRow, Snapshot, CPU_INTERVAL, HISTORY};

/// Monospace advance as a share of the font size.
const CHAR_WIDTH: f32 = 0.6;
const DOT_PITCH: f32 = 6.0;
const DOT_RADIUS: f32 = 1.5;

pub fn draw(
    painter: &Painter,
    layout: &Layout,
    snapshot: &Snapshot,
    now: DateTime<Local>,
    twelve_hour: bool,
    animate: bool,
    style: &Style<'_>,
) {
    let scroll = if animate { scroll_progress(snapshot.sampled_at, Instant::now(), CPU_INTERVAL) } else { 1.0 };
    for (id, rect) in &layout.panels {
        match id {
            PanelId::Clock => {
                let (time, suffix) = clock_text(now, twelve_hour);
                panel::clock(painter, *rect, &time, suffix.as_deref(), style);
            }
            PanelId::System => system(painter, *rect, snapshot, now, style),
            PanelId::Hardware => hardware(painter, *rect, snapshot, style),
            PanelId::Cpu => cpu(painter, *rect, &snapshot.cpu, scroll, style),
            PanelId::Memory => memory(painter, *rect, snapshot.memory, style),
            PanelId::Processes => processes(painter, *rect, &snapshot.processes, style),
            _ => {}
        }
    }
}

/// How far through the gap to the next sample we are, from 0 to 1.
pub fn scroll_progress(sampled_at: Option<Instant>, now: Instant, interval: Duration) -> f32 {
    sampled_at.map_or(1.0, |at| {
        (now.saturating_duration_since(at).as_secs_f32() / interval.as_secs_f32()).clamp(0.0, 1.0)
    })
}

fn clock_text(now: DateTime<Local>, twelve_hour: bool) -> (String, Option<String>) {
    if twelve_hour {
        (now.format("%I:%M:%S").to_string(), Some(now.format("%p").to_string()))
    } else {
        (now.format("%H:%M:%S").to_string(), None)
    }
}

/// How many characters of `size` fit in `width`.
fn chars_in(width: f32, size: f32) -> usize {
    (width / (size * CHAR_WIDTH)).max(0.0) as usize
}

/// A row of label-over-value cells. Columns take the width their text needs,
/// and all values share one size, shrunk only if the row would not fit.
pub fn cells(painter: &Painter, area: Rect, items: &[(&str, String)], style: &Style<'_>) {
    const GAP_CHARS: f32 = 1.5;
    let theme = style.theme;
    let caption = style.caption();
    let label_size = caption.size;
    let widest = |item: &(&str, String)| {
        let label = item.0.chars().count() as f32 * label_size * 0.86 / style.fonts.body;
        (item.1.chars().count() as f32).max(label)
    };
    let needed: f32 = items.iter().map(widest).sum();
    let gaps = GAP_CHARS * (items.len() as f32 - 1.0);
    let size = style.fonts.body.min(area.width() / ((needed + gaps) * CHAR_WIDTH));

    // Share what is left over evenly, so the row reaches the panel's far edge.
    let spare = area.width() - (needed + gaps) * size * CHAR_WIDTH;
    let gap = GAP_CHARS * size * CHAR_WIDTH + spare / (items.len() as f32 - 1.0).max(1.0);
    let mut x = area.min.x;
    for item in items {
        painter.text(pos2(x, area.min.y), Align2::LEFT_TOP, item.0, caption.clone(), theme.muted);
        let at = pos2(x, area.min.y + label_size * 1.25);
        painter.text(at, Align2::LEFT_TOP, &item.1, FontId::monospace(size), theme.foreground);
        x += widest(item) * size * CHAR_WIDTH + gap;
    }
}

fn system(painter: &Painter, rect: Rect, snapshot: &Snapshot, now: DateTime<Local>, style: &Style<'_>) {
    let (inner, content) = panel::ruled(painter, rect, style);
    let year = now.format("%Y").to_string();
    let date = now.format("%b %-d").to_string().to_uppercase();
    let os = if cfg!(windows) { "win" } else { std::env::consts::OS };
    cells(
        &inner,
        content,
        &[
            (&year, date),
            ("UPTIME", format::uptime(snapshot.uptime_seconds)),
            ("TYPE", os.to_owned()),
            ("POWER", format::power(snapshot.power)),
        ],
        style,
    );
}

/// Three columns, with long names cut to fit their column.
fn hardware(painter: &Painter, rect: Rect, snapshot: &Snapshot, style: &Style<'_>) {
    let (inner, content) = panel::ruled(painter, rect, style);
    let hardware = &snapshot.hardware;
    let columns = [
        ("MANUFACTURER", &hardware.manufacturer, 0.0),
        ("MODEL", &hardware.model, 0.44),
        ("CHASSIS", &hardware.chassis, 0.72),
    ];
    let caption = style.caption();
    let size = style.fonts.label;
    let gap_after = |i: usize| columns.get(i + 1).map_or(1.0, |c| c.2) - columns[i].2;
    for (i, (label, value, start)) in columns.iter().enumerate() {
        let x = content.min.x + content.width() * start;
        inner.text(pos2(x, content.min.y), Align2::LEFT_TOP, *label, caption.clone(), style.theme.muted);
        let width = content.width() * gap_after(i) - 6.0;
        let text = format::truncate(format::text(value.as_deref()), chars_in(width, size));
        let at = pos2(x, content.min.y + caption.size * 1.25);
        inner.text(at, Align2::LEFT_TOP, text, FontId::monospace(size), style.theme.foreground);
    }
}

fn cpu(painter: &Painter, rect: Rect, cpu: &CpuStats, scroll: f32, style: &Style<'_>) {
    let chars = chars_in(rect.width() * 0.62, style.caption().size * 0.86);
    let model = format::truncate(format::text(cpu.model.as_deref()), chars);
    let (inner, content) = panel::titled(painter, rect, PanelId::Cpu.title(), Some(&model), style);

    let footer_height = style.caption().size * 1.25 + style.fonts.body * 1.3;
    let footer = Rect::from_min_max(pos2(content.min.x, content.max.y - footer_height), content.max);
    let cores = (!cpu.history.is_empty()).then_some(cpu.history.len());
    cells(
        &inner,
        footer,
        &[
            ("CORES", format::count(cores)),
            ("SPD", format::ghz(cpu.frequency_mhz)),
            ("MAX", format::ghz(cpu.max_frequency_mhz)),
            ("TEMP", format::temperature(cpu.temperature)),
            ("TASKS", format::count(cpu.tasks)),
        ],
        style,
    );

    let graphs = Rect::from_min_max(content.min, pos2(content.max.x, footer.min.y - 6.0));
    let groups = core_groups(cpu.history.len());
    if groups.is_empty() {
        no_data(&inner, graphs, style);
        return;
    }
    for (area, group) in graph_areas(graphs, groups.len()).into_iter().zip(&groups) {
        core_graph(&inner, area, &cpu.history[group.clone()], group.start + 1, scroll, style);
    }
}

/// Stack the groups, or put them side by side when a short panel would leave
/// too little height for a label and a graph.
fn graph_areas(area: Rect, count: usize) -> Vec<Rect> {
    const GAP: f32 = 6.0;
    const MIN_STACKED_HEIGHT: f32 = 30.0;
    let gaps = GAP * (count.saturating_sub(1)) as f32;
    let stacked = (area.height() - gaps) / count.max(1) as f32;
    (0..count)
        .map(|i| {
            if stacked >= MIN_STACKED_HEIGHT {
                let top = area.min.y + i as f32 * (stacked + GAP);
                Rect::from_min_size(pos2(area.min.x, top), vec2(area.width(), stacked))
            } else {
                let width = (area.width() - gaps) / count as f32;
                let left = area.min.x + i as f32 * (width + GAP);
                Rect::from_min_size(pos2(left, area.min.y), vec2(width, area.height()))
            }
        })
        .collect()
}

/// Split the cores into at most two halves, as the graphs show them.
fn core_groups(cores: usize) -> Vec<std::ops::Range<usize>> {
    match cores {
        0 => Vec::new(),
        1 => vec![0..1],
        _ => {
            let half = cores.div_ceil(2);
            vec![0..half, half..cores]
        }
    }
}

/// Names and mean at the left; faint lines for each core and one bright line
/// for the group's mean at the right. The fixed 0-100 scale keeps groups
/// comparable at a glance.
fn core_graph(painter: &Painter, group: Rect, cores: &[Vec<f32>], first_core: usize, scroll: f32, style: &Style<'_>) {
    const LABEL_WIDTH: f32 = 66.0;
    let theme = style.theme;
    let last = first_core + cores.len() - 1;
    let name = if cores.len() == 1 { format!("#{first_core}") } else { format!("#{first_core}-{last}") };
    let mean = mean_series(cores);
    let average = format!("Avg. {}", format::percent(recent_mean(&mean)));
    let caption = style.caption();
    let centre = group.center().y;
    painter.text(pos2(group.min.x, centre), Align2::LEFT_BOTTOM, name, style.heading(), theme.foreground);
    painter.text(pos2(group.min.x, centre), Align2::LEFT_TOP, average, caption, theme.muted);

    let area = Rect::from_min_max(pos2(group.min.x + LABEL_WIDTH, group.min.y), group.max);
    let clip = painter.with_clip_rect(area.expand(1.0));
    clip.hline(area.x_range(), area.max.y, Stroke::new(1.0, theme.line));
    clip.hline(area.x_range(), area.center().y, Stroke::new(1.0, theme.grid));
    let core_stroke = Stroke::new(1.0, theme.muted);
    for core in cores {
        clip.add(Shape::line(graph_points(area, core, scroll, HISTORY), core_stroke));
    }
    clip.add(Shape::line(graph_points(area, &mean, scroll, HISTORY), Stroke::new(1.6, theme.foreground)));
}

/// Newest sample at the right, sliding in as `scroll` goes from 0 to 1 so the
/// line moves continuously between samples. The caller clips the overhang.
pub fn graph_points(area: Rect, values: &[f32], scroll: f32, history: usize) -> Vec<egui::Pos2> {
    let step = area.width() / (history - 1) as f32;
    let count = values.len();
    let right = area.max.x + (1.0 - scroll) * step;
    values
        .iter()
        .enumerate()
        .map(|(i, value)| {
            let x = right - (count - 1 - i) as f32 * step;
            let y = area.max.y - area.height() * value.clamp(0.0, 100.0) / 100.0;
            pos2(x, y)
        })
        .collect()
}

/// The last second's mean, so the printed figure stays readable at 4 samples a second.
fn recent_mean(values: &[f32]) -> Option<f32> {
    let recent = &values[values.len().saturating_sub(4)..];
    (!recent.is_empty()).then(|| recent.iter().sum::<f32>() / recent.len() as f32)
}

fn mean_series(cores: &[Vec<f32>]) -> Vec<f32> {
    let length = cores.iter().map(Vec::len).min().unwrap_or(0);
    (0..length)
        .map(|i| {
            let total: f32 = cores.iter().map(|core| core[core.len() - length + i]).sum();
            total / cores.len() as f32
        })
        .collect()
}

fn memory(painter: &Painter, rect: Rect, memory: Option<MemoryStats>, style: &Style<'_>) {
    let value = memory.map(|m| format::memory_use(m.used, m.total));
    let (inner, content) = panel::titled(painter, rect, PanelId::Memory.title(), value.as_deref(), style);
    let Some(memory) = memory else {
        no_data(&inner, content, style);
        return;
    };

    let bar_height = style.fonts.label * 1.3;
    let swap = Rect::from_min_max(pos2(content.min.x, content.max.y - bar_height), content.max);
    let grid = Rect::from_min_max(content.min, pos2(content.max.x, swap.min.y - 4.0));
    dot_grid(&inner, grid, memory, style);
    swap_bar(&inner, swap, memory, style);
}

/// One dot per slice of memory, scattered like the original: bright for used,
/// mid for cache, dim for free. Positions are fixed, so a dot only changes
/// when the share it represents is crossed.
fn dot_grid(painter: &Painter, area: Rect, memory: MemoryStats, style: &Style<'_>) {
    let theme = style.theme;
    let columns = (area.width() / DOT_PITCH).floor().max(1.0) as usize;
    let rows = (area.height() / DOT_PITCH).floor().max(1.0) as usize;
    let total = memory.total.max(1) as f32;
    let used = memory.used as f32 / total;
    let cache = (1.0 - used - memory.free as f32 / total).max(0.0);
    for row in 0..rows {
        for column in 0..columns {
            let colour = match dot_state(row * columns + column, used, cache) {
                DotState::Used => theme.foreground,
                DotState::Cache => theme.muted,
                DotState::Free => theme.line,
            };
            let at = area.min + vec2((column as f32 + 0.5) * DOT_PITCH, (row as f32 + 0.5) * DOT_PITCH);
            painter.circle_filled(at, DOT_RADIUS, colour);
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DotState {
    Used,
    Cache,
    Free,
}

/// Each dot gets a fixed pseudo-random place in 0-1, then falls into a band.
fn dot_state(index: usize, used: f32, cache: f32) -> DotState {
    let mut hash = (index as u32).wrapping_add(1).wrapping_mul(0x9E37_79B1);
    hash ^= hash >> 15;
    hash = hash.wrapping_mul(0x85EB_CA77);
    hash ^= hash >> 13;
    let place = hash as f32 / u32::MAX as f32;
    if place < used {
        DotState::Used
    } else if place < used + cache {
        DotState::Cache
    } else {
        DotState::Free
    }
}

/// A thin track with end ticks and a brighter line for what is in use.
fn swap_bar(painter: &Painter, area: Rect, memory: MemoryStats, style: &Style<'_>) {
    let theme = style.theme;
    let font = style.caption();
    painter.text(area.left_center(), Align2::LEFT_CENTER, "SWAP", font.clone(), theme.foreground);
    let value = if memory.swap_total == 0 {
        "NONE".to_owned()
    } else {
        format::gib_pair(memory.swap_used, memory.swap_total)
    };
    painter.text(area.right_center(), Align2::RIGHT_CENTER, &value, font.clone(), theme.muted);

    let left = area.min.x + font.size * 3.0;
    let right = area.max.x - value.chars().count() as f32 * font.size * 0.56;
    if right - left < 16.0 {
        return;
    }
    let y = area.center().y;
    painter.hline(left..=right, y, Stroke::new(1.0, theme.rule));
    painter.vline(right, (y - 4.0)..=(y + 4.0), Stroke::new(1.0, theme.foreground));
    let share = (memory.swap_used as f32 / memory.swap_total.max(1) as f32).clamp(0.0, 1.0);
    painter.hline(left..=left + (right - left) * share, y, Stroke::new(3.0, theme.foreground));
}

fn processes(painter: &Painter, rect: Rect, rows: &[ProcessRow], style: &Style<'_>) {
    let columns = "PID | NAME | CPU | MEM";
    let (inner, content) = panel::titled(painter, rect, PanelId::Processes.title(), Some(columns), style);
    if rows.is_empty() {
        no_data(&inner, content, style);
        return;
    }
    let size = style.fonts.label;
    let font = FontId::monospace(size);
    let colour = style.theme.foreground;
    let char_width = size * CHAR_WIDTH;
    let line = size * 1.3;
    let cpu_x = content.max.x - 6.0 * char_width;
    let mem_x = content.max.x;
    // Columns are sized from the widest real PID, leaving the rest for names.
    let pid_chars = rows.iter().map(|row| row.pid.to_string().len()).max().unwrap_or(3).max(3);
    let name_x = content.min.x + (pid_chars + 1) as f32 * char_width;
    let name_chars = chars_in(cpu_x - 5.0 * char_width - name_x, size);

    let shown = (content.height() / line) as usize;
    for (i, row) in rows.iter().take(shown).enumerate() {
        let y = content.min.y + i as f32 * line;
        inner.text(pos2(content.min.x, y), Align2::LEFT_TOP, row.pid.to_string(), font.clone(), style.theme.muted);
        inner.text(pos2(name_x, y), Align2::LEFT_TOP, format::truncate(&row.name, name_chars), font.clone(), colour);
        inner.text(pos2(cpu_x, y), Align2::RIGHT_TOP, format!("{:.1}", row.cpu), font.clone(), colour);
        inner.text(pos2(mem_x, y), Align2::RIGHT_TOP, format!("{:.1}", row.memory), font.clone(), colour);
    }
}

fn no_data(painter: &Painter, area: Rect, style: &Style<'_>) {
    painter.text(area.center(), Align2::CENTER_CENTER, NOT_AVAILABLE, FontId::monospace(style.fonts.body), style.theme.muted);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::stats::{Hardware, Power};
    use crate::theme::Theme;
    use crate::ui::layout::compute;
    use egui::epaint::ClippedShape;
    use egui::{LayerId, Pos2};

    /// Run the real panel code in a headless frame, returning all text drawn.
    fn render(size: (f32, f32), snapshot: &Snapshot) -> Vec<String> {
        let ctx = egui::Context::default();
        let config = Config::default();
        let theme = Theme::tron();
        let window = Rect::from_min_size(Pos2::ZERO, vec2(size.0, size.1));
        let input = egui::RawInput { screen_rect: Some(window), ..Default::default() };
        let mut output = ctx.run_ui(input, |ui| {
            let painter = Painter::new(ui.ctx().clone(), LayerId::background(), window);
            let style = Style { theme: &theme, fonts: &config.fonts };
            draw(&painter, &compute(window, &config.fonts), snapshot, Local::now(), false, true, &style);
        });
        output.textures_delta.clear();
        output.shapes.iter().filter_map(text_of).collect()
    }

    fn text_of(shape: &ClippedShape) -> Option<String> {
        match &shape.shape {
            Shape::Text(text) => Some(text.galley.text().to_owned()),
            _ => None,
        }
    }

    fn full_snapshot() -> Snapshot {
        Snapshot {
            uptime_seconds: Some(90_061),
            power: Some(Power::Battery { percent: Some(84), charging: true }),
            hardware: Hardware {
                manufacturer: Some("A very long manufacturer name that cannot possibly fit".to_owned()),
                model: Some("Model".to_owned()),
                chassis: Some("Laptop".to_owned()),
            },
            cpu: CpuStats {
                model: Some("Some CPU".to_owned()),
                history: vec![vec![50.0; HISTORY]; 16],
                temperature: Some(52.0),
                frequency_mhz: Some(2100),
                max_frequency_mhz: Some(4600),
                tasks: Some(321),
            },
            memory: Some(MemoryStats { total: 8 << 30, used: 4 << 30, free: 1 << 30, swap_total: 2 << 30, swap_used: 1 << 30 }),
            processes: (0..24)
                .map(|pid| ProcessRow { pid, name: "process".to_owned(), cpu: 1.0, memory: 2.0 })
                .collect(),
            sampled_at: None,
        }
    }

    #[test]
    fn an_empty_snapshot_shows_na_in_every_panel() {
        let text = render((1600.0, 900.0), &Snapshot::default());
        let na = text.iter().filter(|t| t.as_str() == NOT_AVAILABLE).count();
        // Uptime, power, three hardware values, CPU model, graph, cores, speed,
        // maximum, temperature, tasks, memory and processes.
        assert_eq!(na, 14, "{text:?}");
    }

    #[test]
    fn a_full_snapshot_shows_its_values() {
        let text = render((1600.0, 900.0), &full_snapshot());
        let expected = ["84%+", "1d 01:01", "52°C", "2.1G", "4.6G", "321", "16", "USING 4.0 OUT OF 8.0 GiB", "1.0/2.0 GiB", "Avg. 50%"];
        for expected in expected {
            assert!(text.iter().any(|t| t == expected), "{expected} missing from {text:?}");
        }
        assert!(!text.iter().any(|t| t == NOT_AVAILABLE), "{text:?}");
    }

    #[test]
    fn panels_survive_every_window_size_and_data_shape() {
        let mut one_core = full_snapshot();
        one_core.cpu.history.truncate(1);
        let mut no_swap = full_snapshot();
        no_swap.memory = Some(MemoryStats { total: 1, used: 2, free: 9, swap_total: 0, swap_used: 0 });
        let mut short_history = full_snapshot();
        short_history.cpu.history = vec![vec![], vec![1.0], vec![1.0, 2.0]];
        for size in [(1.0, 1.0), (200.0, 150.0), (1024.0, 680.0), (3840.0, 2160.0)] {
            for snapshot in [&Snapshot::default(), &full_snapshot(), &one_core, &no_swap, &short_history] {
                render(size, snapshot);
            }
        }
    }

    #[test]
    fn process_names_keep_their_room_at_the_minimum_size() {
        let mut snapshot = full_snapshot();
        snapshot.processes[0].name = "edex-rs-long-name".to_owned();
        snapshot.processes[0].pid = 30545;
        let text = render((1024.0, 680.0), &snapshot);
        assert!(text.iter().any(|t| t.starts_with("edex-r") && t.ends_with('…')), "{text:?}");
    }

    #[test]
    fn cores_split_into_two_even_groups() {
        assert!(core_groups(0).is_empty());
        assert_eq!(core_groups(1), vec![0..1]);
        assert_eq!(core_groups(2), vec![0..1, 1..2]);
        assert_eq!(core_groups(7), vec![0..4, 4..7]);
        assert_eq!(core_groups(16), vec![0..8, 8..16]);
    }

    #[test]
    fn short_panels_put_the_core_groups_side_by_side() {
        let tall = graph_areas(Rect::from_min_size(Pos2::ZERO, vec2(200.0, 120.0)), 2);
        assert!(tall[0].max.y <= tall[1].min.y);
        assert_eq!(tall[0].width(), 200.0);

        let short = graph_areas(Rect::from_min_size(Pos2::ZERO, vec2(200.0, 40.0)), 2);
        assert!(short[0].max.x <= short[1].min.x);
        assert_eq!(short[0].height(), 40.0);
    }

    #[test]
    fn graph_points_stay_inside_their_area() {
        let area = Rect::from_min_size(pos2(10.0, 20.0), vec2(100.0, 50.0));
        let points = graph_points(area, &[-5.0, 0.0, 50.0, 100.0, 500.0], 1.0, HISTORY);
        assert!(points.iter().all(|p| area.expand(0.01).contains(*p)));
        assert_eq!(points.last().unwrap().x, area.max.x);
        assert_eq!(points[1].y, area.max.y);
        assert_eq!(points[3].y, area.min.y);
    }

    #[test]
    fn the_graph_slides_a_full_step_between_samples() {
        let area = Rect::from_min_size(pos2(0.0, 0.0), vec2((HISTORY - 1) as f32, 50.0));
        let start = graph_points(area, &[10.0, 20.0], 0.0, HISTORY);
        let end = graph_points(area, &[10.0, 20.0], 1.0, HISTORY);
        assert_eq!(start[1].x - end[1].x, 1.0);
        assert_eq!(end[1].x, area.max.x);
    }

    #[test]
    fn scroll_progress_runs_from_zero_to_one_and_stops() {
        let start = Instant::now();
        assert_eq!(scroll_progress(Some(start), start, CPU_INTERVAL), 0.0);
        assert!((scroll_progress(Some(start), start + CPU_INTERVAL / 2, CPU_INTERVAL) - 0.5).abs() < 0.01);
        assert_eq!(scroll_progress(Some(start), start + CPU_INTERVAL * 3, CPU_INTERVAL), 1.0);
        assert_eq!(scroll_progress(None, start, CPU_INTERVAL), 1.0);
    }

    #[test]
    fn the_printed_average_covers_the_last_second() {
        assert_eq!(recent_mean(&[100.0, 0.0, 10.0, 20.0, 30.0, 40.0]), Some(25.0));
        assert_eq!(recent_mean(&[8.0]), Some(8.0));
        assert_eq!(recent_mean(&[]), None);
    }

    #[test]
    fn mean_lines_up_samples_from_the_newest_end() {
        let cores = vec![vec![10.0, 20.0, 30.0], vec![50.0, 70.0]];
        assert_eq!(mean_series(&cores), vec![35.0, 50.0]);
        assert!(mean_series(&[]).is_empty());
    }

    #[test]
    fn dots_show_each_share_of_memory_in_proportion() {
        let dots = 5_000;
        let count = |state| (0..dots).filter(|i| dot_state(*i, 0.3, 0.2) == state).count() as f32 / dots as f32;
        assert!((count(DotState::Used) - 0.3).abs() < 0.03, "{}", count(DotState::Used));
        assert!((count(DotState::Cache) - 0.2).abs() < 0.03, "{}", count(DotState::Cache));
        assert!((0..dots).all(|i| dot_state(i, 1.0, 0.0) == DotState::Used));
        assert!((0..dots).all(|i| dot_state(i, 0.0, 0.0) == DotState::Free));
    }

    #[test]
    fn dots_keep_their_place_as_memory_grows() {
        for i in 0..1_000 {
            if dot_state(i, 0.2, 0.0) == DotState::Used {
                assert_eq!(dot_state(i, 0.5, 0.0), DotState::Used);
            }
        }
    }

    #[test]
    fn the_clock_can_show_either_format() {
        use chrono::TimeZone;
        let afternoon = Local.with_ymd_and_hms(2026, 11, 5, 13, 20, 24).unwrap();
        assert_eq!(clock_text(afternoon, false), ("13:20:24".to_owned(), None));
        assert_eq!(clock_text(afternoon, true), ("01:20:24".to_owned(), Some("PM".to_owned())));
    }
}
