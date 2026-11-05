//! The live left column. Each panel draws from the latest snapshot and falls
//! back to N/A for anything missing, at any panel size.

use chrono::{DateTime, Local};
use egui::{pos2, vec2, Align2, Color32, FontId, Painter, Rect, Shape, Stroke};

use super::layout::{Layout, PanelId};
use super::panel::{self, Style};
use crate::stats::format::{self, NOT_AVAILABLE};
use crate::stats::{CpuStats, MemoryStats, ProcessRow, Snapshot, HISTORY};

/// Monospace advance as a share of the font size.
const CHAR_WIDTH: f32 = 0.6;
const DOT_PITCH: f32 = 7.0;
const DOT_RADIUS: f32 = 2.0;
/// Hide the CPU model line below this panel height, so the graphs keep room.
const CPU_MODEL_MIN_HEIGHT: f32 = 170.0;

pub fn draw(painter: &Painter, layout: &Layout, snapshot: &Snapshot, now: DateTime<Local>, style: &Style<'_>) {
    for (id, rect) in &layout.panels {
        match id {
            PanelId::Clock => panel::clock(painter, *rect, &now.format("%H:%M:%S").to_string(), style),
            PanelId::System => system(painter, *rect, snapshot, now, style),
            PanelId::Hardware => hardware(painter, *rect, snapshot, style),
            PanelId::Cpu => cpu(painter, *rect, &snapshot.cpu, style),
            PanelId::Memory => memory(painter, *rect, snapshot.memory, style),
            PanelId::Processes => processes(painter, *rect, &snapshot.processes, style),
            _ => {}
        }
    }
}

/// How many characters of `size` fit in `width`.
fn chars_in(width: f32, size: f32) -> usize {
    (width / (size * CHAR_WIDTH)).max(0.0) as usize
}

/// A row of label-over-value cells. Columns take the width their text needs,
/// and all values share one size, shrunk only if the row would not fit.
fn cells(painter: &Painter, area: Rect, items: &[(&str, String)], style: &Style<'_>) {
    const GAP_CHARS: f32 = 1.5;
    let theme = style.theme;
    let label_size = style.fonts.label * 0.87;
    let widest = |item: &(&str, String)| {
        let label = item.0.chars().count() as f32 * label_size / style.fonts.body;
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
        painter.text(pos2(x, area.min.y), Align2::LEFT_TOP, item.0, FontId::monospace(label_size), theme.muted);
        let at = pos2(x, area.min.y + label_size * 1.25);
        painter.text(at, Align2::LEFT_TOP, &item.1, FontId::monospace(size), theme.foreground);
        x += widest(item) * size * CHAR_WIDTH + gap;
    }
}

fn system(painter: &Painter, rect: Rect, snapshot: &Snapshot, now: DateTime<Local>, style: &Style<'_>) {
    let (inner, content) = panel::titled(painter, rect, PanelId::System.title(), None, style);
    let date = now.format("%b %d").to_string().to_uppercase();
    cells(
        &inner,
        content,
        &[
            ("DATE", date),
            ("UPTIME", format::uptime(snapshot.uptime_seconds)),
            ("OS", std::env::consts::OS.to_uppercase()),
            ("POWER", format::power(snapshot.power)),
        ],
        style,
    );
}

fn hardware(painter: &Painter, rect: Rect, snapshot: &Snapshot, style: &Style<'_>) {
    let (inner, content) = panel::titled(painter, rect, PanelId::Hardware.title(), None, style);
    let hardware = &snapshot.hardware;
    let rows = [
        ("MAKER", &hardware.manufacturer),
        ("MODEL", &hardware.model),
        ("CHASSIS", &hardware.chassis),
    ];
    let size = style.fonts.label;
    let step = content.height() / rows.len() as f32;
    let key_width = 8.0 * size * CHAR_WIDTH;
    let value_chars = chars_in(content.width() - key_width, size);
    for (i, (key, value)) in rows.iter().enumerate() {
        let y = content.min.y + i as f32 * step;
        let font = FontId::monospace(size);
        inner.text(pos2(content.min.x, y), Align2::LEFT_TOP, *key, font.clone(), style.theme.muted);
        let text = format::truncate(format::text(value.as_deref()), value_chars);
        inner.text(pos2(content.min.x + key_width, y), Align2::LEFT_TOP, text, font, style.theme.foreground);
    }
}

fn cpu(painter: &Painter, rect: Rect, cpu: &CpuStats, style: &Style<'_>) {
    let average = format!("AVG {}", format::percent(cpu.average));
    let (inner, mut content) = panel::titled(painter, rect, PanelId::Cpu.title(), Some(&average), style);

    let small = style.fonts.label * 0.87;
    if rect.height() >= CPU_MODEL_MIN_HEIGHT {
        let text = format::truncate(format::text(cpu.model.as_deref()), chars_in(content.width(), small));
        inner.text(content.min, Align2::LEFT_TOP, text, FontId::monospace(small), style.theme.muted);
        content.min.y += small * 1.4;
    }

    let footer_height = small * 1.25 + style.fonts.body * 1.3;
    let footer = Rect::from_min_max(pos2(content.min.x, content.max.y - footer_height), content.max);
    cells(
        &inner,
        footer,
        &[
            ("TEMP", format::temperature(cpu.temperature)),
            ("GHZ", format::frequency(cpu.frequency_mhz, cpu.max_frequency_mhz)),
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
    let gap = 5.0;
    let height = (graphs.height() - gap * (groups.len() - 1) as f32) / groups.len() as f32;
    for (i, group) in groups.iter().enumerate() {
        let top = graphs.min.y + i as f32 * (height + gap);
        let area = Rect::from_min_size(pos2(graphs.min.x, top), vec2(graphs.width(), height));
        core_graph(&inner, area, &cpu.history[group.clone()], group.start + 1, style);
    }
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

/// Faint lines for each core, one bright line for the group's mean, on a fixed
/// 0-100 scale so two graphs can be compared at a glance.
fn core_graph(painter: &Painter, area: Rect, cores: &[Vec<f32>], first_core: usize, style: &Style<'_>) {
    let theme = style.theme;
    let clip = painter.with_clip_rect(area);
    clip.rect_stroke(area, 0.0, Stroke::new(1.0, theme.line), egui::StrokeKind::Inside);
    clip.hline(area.x_range(), area.center().y, Stroke::new(1.0, theme.grid));

    let core_stroke = Stroke::new(1.0, theme.muted);
    for core in cores {
        clip.add(Shape::line(graph_points(area, core), core_stroke));
    }
    let mean = mean_series(cores);
    clip.add(Shape::line(graph_points(area, &mean), Stroke::new(1.6, theme.foreground)));

    let last = first_core + cores.len() - 1;
    let label = if cores.len() == 1 { format!("#{first_core}") } else { format!("#{first_core}-{last}") };
    let size = style.fonts.label * 0.8;
    clip.text(area.left_top() + vec2(4.0, 2.0), Align2::LEFT_TOP, label, FontId::monospace(size), theme.foreground);
}

/// Newest sample on the right edge, so the line grows in from the right.
fn graph_points(area: Rect, values: &[f32]) -> Vec<egui::Pos2> {
    let step = area.width() / (HISTORY - 1) as f32;
    let count = values.len();
    values
        .iter()
        .enumerate()
        .map(|(i, value)| {
            let x = area.max.x - (count - 1 - i) as f32 * step;
            let y = area.max.y - area.height() * value.clamp(0.0, 100.0) / 100.0;
            pos2(x, y)
        })
        .collect()
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
    let value = memory.map(|m| format::gib_pair(m.used, m.total));
    let (inner, content) = panel::titled(painter, rect, PanelId::Memory.title(), value.as_deref(), style);
    let Some(memory) = memory else {
        no_data(&inner, content, style);
        return;
    };

    let bar_height = style.fonts.label * 1.3;
    let swap = Rect::from_min_max(pos2(content.min.x, content.max.y - bar_height), content.max);
    let grid = Rect::from_min_max(content.min, pos2(content.max.x, swap.min.y - 5.0));
    dot_grid(&inner, grid, memory.used as f32 / memory.total.max(1) as f32, style);
    swap_bar(&inner, swap, memory, style);
}

/// Bright dots for memory in use, dim ones for the rest.
fn dot_grid(painter: &Painter, area: Rect, used: f32, style: &Style<'_>) {
    let columns = (area.width() / DOT_PITCH).floor().max(1.0) as usize;
    let rows = (area.height() / DOT_PITCH).floor().max(1.0) as usize;
    let filled = lit_dots(used, columns * rows);
    // Column by column, so the used share reads as a level from the left.
    for column in 0..columns {
        for row in 0..rows {
            let index = column * rows + row;
            let colour = if index < filled { style.theme.foreground } else { style.theme.line };
            let at = area.min + vec2((column as f32 + 0.5) * DOT_PITCH, (row as f32 + 0.5) * DOT_PITCH);
            painter.circle_filled(at, DOT_RADIUS, colour);
        }
    }
}

fn lit_dots(used: f32, dots: usize) -> usize {
    ((used.clamp(0.0, 1.0) * dots as f32).round() as usize).min(dots)
}

fn swap_bar(painter: &Painter, area: Rect, memory: MemoryStats, style: &Style<'_>) {
    let theme = style.theme;
    let size = style.fonts.label * 0.87;
    let font = FontId::monospace(size);
    painter.text(area.left_center(), Align2::LEFT_CENTER, "SWAP", font.clone(), theme.muted);
    if memory.swap_total == 0 {
        painter.text(area.right_center(), Align2::RIGHT_CENTER, "NONE", font, theme.foreground);
        return;
    }
    let value = format::gib_pair(memory.swap_used, memory.swap_total);
    painter.text(area.right_center(), Align2::RIGHT_CENTER, &value, font, theme.foreground);

    let left = area.min.x + 5.0 * size * CHAR_WIDTH;
    let right = area.max.x - (value.chars().count() as f32 + 1.0) * size * CHAR_WIDTH;
    if right - left < 16.0 {
        return;
    }
    let bar = Rect::from_center_size(pos2((left + right) * 0.5, area.center().y), vec2(right - left, 6.0));
    painter.rect_filled(bar, 0.0, theme.line);
    let share = (memory.swap_used as f32 / memory.swap_total as f32).clamp(0.0, 1.0);
    let used = Rect::from_min_size(bar.min, vec2(bar.width() * share, bar.height()));
    painter.rect_filled(used, 0.0, theme.foreground);
}

fn processes(painter: &Painter, rect: Rect, rows: &[ProcessRow], style: &Style<'_>) {
    let (inner, content) = panel::titled(painter, rect, PanelId::Processes.title(), None, style);
    if rows.is_empty() {
        no_data(&inner, content, style);
        return;
    }
    let size = style.fonts.label;
    let font = FontId::monospace(size);
    let char_width = size * CHAR_WIDTH;
    let line = size * 1.3;
    let cpu_x = content.max.x - 6.0 * char_width;
    let mem_x = content.max.x;
    // Columns are sized from the widest real PID, leaving the rest for names.
    let pid_chars = rows.iter().map(|row| row.pid.to_string().len()).max().unwrap_or(3).max(3);
    let name_x = content.min.x + (pid_chars + 1) as f32 * char_width;
    let name_chars = chars_in(cpu_x - 5.0 * char_width - name_x, size);

    let row_text = |y: f32, pid: &str, name: &str, cpu: &str, mem: &str, colour: Color32| {
        inner.text(pos2(content.min.x, y), Align2::LEFT_TOP, pid, font.clone(), colour);
        inner.text(pos2(name_x, y), Align2::LEFT_TOP, format::truncate(name, name_chars), font.clone(), colour);
        inner.text(pos2(cpu_x, y), Align2::RIGHT_TOP, cpu, font.clone(), colour);
        inner.text(pos2(mem_x, y), Align2::RIGHT_TOP, mem, font.clone(), colour);
    };

    row_text(content.min.y, "PID", "NAME", "CPU%", "MEM%", style.theme.muted);
    let shown = ((content.height() / line) as usize).saturating_sub(1);
    for (i, row) in rows.iter().take(shown).enumerate() {
        let y = content.min.y + (i + 1) as f32 * line;
        let cpu = format!("{:.1}", row.cpu);
        let mem = format!("{:.1}", row.memory);
        row_text(y, &row.pid.to_string(), &row.name, &cpu, &mem, style.theme.foreground);
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
            draw(&painter, &compute(window, &config.fonts), snapshot, Local::now(), &style);
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
                average: Some(50.0),
                temperature: Some(52.0),
                frequency_mhz: Some(2100),
                max_frequency_mhz: Some(4600),
                tasks: Some(321),
            },
            memory: Some(MemoryStats { total: 8 << 30, used: 4 << 30, swap_total: 2 << 30, swap_used: 1 << 30 }),
            processes: (0..24)
                .map(|pid| ProcessRow { pid, name: "process".to_owned(), cpu: 1.0, memory: 2.0 })
                .collect(),
        }
    }

    #[test]
    fn an_empty_snapshot_shows_na_in_every_panel() {
        let text = render((1600.0, 900.0), &Snapshot::default());
        let na = text.iter().filter(|t| t.as_str() == NOT_AVAILABLE).count();
        // Uptime, power, three hardware rows, CPU model, graph, temp, clock speed,
        // tasks, memory and processes.
        assert_eq!(na, 12, "{text:?}");
        assert!(text.iter().any(|t| t == "AVG N/A"));
    }

    #[test]
    fn a_full_snapshot_shows_its_values() {
        let text = render((1600.0, 900.0), &full_snapshot());
        for expected in ["84%+", "1d 01:01", "52°C", "2.1/4.6", "321", "4.0/8.0 GiB", "1.0/2.0 GiB", "AVG 50%"] {
            assert!(text.iter().any(|t| t == expected), "{expected} missing from {text:?}");
        }
        assert!(!text.iter().any(|t| t == NOT_AVAILABLE), "{text:?}");
    }

    #[test]
    fn panels_survive_every_window_size_and_data_shape() {
        let mut one_core = full_snapshot();
        one_core.cpu.history.truncate(1);
        let mut no_swap = full_snapshot();
        no_swap.memory = Some(MemoryStats { total: 1, used: 2, swap_total: 0, swap_used: 0 });
        let mut short_history = full_snapshot();
        short_history.cpu.history = vec![vec![], vec![1.0], vec![1.0, 2.0]];
        for size in [(1.0, 1.0), (200.0, 150.0), (1024.0, 640.0), (3840.0, 2160.0)] {
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
        let text = render((1024.0, 640.0), &snapshot);
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
    fn graph_points_stay_inside_their_area() {
        let area = Rect::from_min_size(pos2(10.0, 20.0), vec2(100.0, 50.0));
        let points = graph_points(area, &[-5.0, 0.0, 50.0, 100.0, 500.0]);
        assert!(points.iter().all(|p| area.expand(0.01).contains(*p)));
        assert_eq!(points.last().unwrap().x, area.max.x);
        assert_eq!(points[1].y, area.max.y);
        assert_eq!(points[3].y, area.min.y);
    }

    #[test]
    fn mean_lines_up_samples_from_the_newest_end() {
        let cores = vec![vec![10.0, 20.0, 30.0], vec![50.0, 70.0]];
        assert_eq!(mean_series(&cores), vec![35.0, 50.0]);
        assert!(mean_series(&[]).is_empty());
    }

    #[test]
    fn dots_track_memory_use_exactly() {
        assert_eq!(lit_dots(0.0, 100), 0);
        assert_eq!(lit_dots(0.5, 100), 50);
        assert_eq!(lit_dots(1.0, 100), 100);
        assert_eq!(lit_dots(2.0, 100), 100);
        assert_eq!(lit_dots(f32::NAN, 100), 0);
    }
}
