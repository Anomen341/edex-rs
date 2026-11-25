//! The live right column. Anything missing reads N/A or OFFLINE, and the globe
//! draws with or without connections.

use std::sync::LazyLock;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use egui::epaint::Mesh;
use egui::{pos2, vec2, Align2, Color32, Painter, Pos2, Rect, Shape, Stroke};

use super::dice::Dice;
use super::layout::{Layout, PanelId};
use super::monitor::{cells, graph_points, scroll_progress};
use super::panel::{self, Style};
use crate::net::{GeoPoint, Link, NetSnapshot, TRAFFIC_HISTORY, TRAFFIC_INTERVAL};
use crate::stats::format;

/// A full turn takes this many seconds.
const ROTATION_SECONDS: f64 = 60.0;
/// Lean of the globe towards the viewer.
const TILT_DEGREES: f32 = 18.0;
/// The globe's share of the space it could fill, leaving room for orbits.
const GLOBE_SHARE: f32 = 0.8;
/// Decorative satellites circling the globe.
const SATELLITES: usize = 5;
/// The made-up "connected satellites" figure changes this often, in seconds.
const RECOUNT_SECONDS: u64 = 600;
/// The made-up figure always falls in this range.
const SATELLITE_RANGE: (u64, u64) = (10, 40);
/// The graph never scales below this, so idle noise stays flat. Bytes a second.
const MIN_SCALE: f32 = 64.0 * 1024.0;

/// Land dots as unit vectors, decoded once from the built-in data.
static LAND: LazyLock<Vec<[f32; 3]>> = LazyLock::new(|| {
    include_bytes!("../../assets/land-points.bin")
        .chunks_exact(4)
        .map(|pair| {
            let latitude = f32::from(i16::from_le_bytes([pair[0], pair[1]])) / 100.0;
            let longitude = f32::from(i16::from_le_bytes([pair[2], pair[3]])) / 100.0;
            to_unit_vector(latitude, longitude)
        })
        .collect()
});

pub fn draw(painter: &Painter, layout: &Layout, snapshot: &NetSnapshot, time: f64, animate: bool, style: &Style<'_>) {
    let scroll = if animate { scroll_progress(snapshot.sampled_at, Instant::now(), TRAFFIC_INTERVAL) } else { 1.0 };
    for (id, rect) in &layout.panels {
        match id {
            PanelId::Network => status(painter, *rect, snapshot, style),
            PanelId::Globe => globe(painter, *rect, snapshot, time, style),
            PanelId::Traffic => traffic(painter, *rect, snapshot, scroll, style),
            _ => {}
        }
    }
}

fn link_text(link: Link) -> &'static str {
    match link {
        Link::Unknown => "...",
        Link::Offline => "OFFLINE",
        Link::LocalOnly => "LOCAL",
        Link::Online => "ONLINE",
    }
}

fn status(painter: &Painter, rect: Rect, snapshot: &NetSnapshot, style: &Style<'_>) {
    let interface = snapshot.interface.as_deref().map(|name| format!("Interface: {name}"));
    let (inner, content) = panel::titled(painter, rect, PanelId::Network.title(), interface.as_deref(), style);
    let latency = snapshot.latency_ms.map(|ms| format!("{ms}ms"));
    let ipv4 = snapshot.ipv4.map(|ip| ip.to_string());
    cells(
        &inner,
        content,
        &[
            ("STATE", link_text(snapshot.link).to_owned()),
            ("IPv4", format::text(ipv4.as_deref()).to_owned()),
            ("PING", format::text(latency.as_deref()).to_owned()),
        ],
        style,
    );
    let row = content.min.y + style.caption().size * 1.25 + style.fonts.body * 1.3;
    let font = style.caption();
    inner.text(pos2(content.min.x, row), Align2::LEFT_TOP, "EXTERNAL IP", font.clone(), style.theme.muted);
    let external = snapshot.external_ip.map_or_else(|| "OFFLINE".to_owned(), |ip| ip.to_string());
    inner.text(pos2(content.max.x, row), Align2::RIGHT_TOP, external, font, style.theme.foreground);
}

fn globe(painter: &Painter, rect: Rect, snapshot: &NetSnapshot, time: f64, style: &Style<'_>) {
    let (inner, content) = panel::titled(painter, rect, PanelId::Globe.title(), Some("GLOBAL NETWORK MAP"), style);
    let theme = style.theme;
    let font = style.caption();
    let count = format::count(snapshot.connections);
    inner.text(content.left_top(), Align2::LEFT_TOP, format!("CONNECTIONS {count}"), font.clone(), theme.muted);
    let located = if snapshot.geoip_loaded {
        format!("LOCATED {}", snapshot.points.len())
    } else {
        "NO GEOIP DATABASE".to_owned()
    };
    inner.text(content.right_top(), Align2::RIGHT_TOP, located, font.clone(), theme.muted);
    let unix = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |since| since.as_secs());
    let satellites = format!("CONNECTED SATELLITES {}", satellite_count(unix));
    inner.text(content.left_bottom(), Align2::LEFT_BOTTOM, satellites, font.clone(), theme.muted);

    let caption = font.size * 1.4;
    let area = Rect::from_min_max(content.min + vec2(0.0, caption), content.max - vec2(0.0, caption));
    let room = area.width().min(area.height()) * 0.5 - 2.0;
    let radius = room * GLOBE_SHARE;
    if radius < 8.0 {
        return;
    }
    let spin = (time / ROTATION_SECONDS * std::f64::consts::TAU) as f32;
    let view = View { spin, centre: area.center(), radius };
    // Orbits are fixed in space, so the satellites don't turn with the land.
    let sky = View { spin: 0.0, ..view };
    for orbit in ORBITS.iter().filter(|orbit| orbit.behind(&sky, time)) {
        orbit.draw(&inner, &sky, time, theme.foreground);
    }
    inner.add(land_mesh(&view, theme.foreground));
    for point in &snapshot.points {
        marker(&inner, &view, *point, theme.foreground);
    }
    for orbit in ORBITS.iter().filter(|orbit| !orbit.behind(&sky, time)) {
        orbit.draw(&inner, &sky, time, theme.foreground);
    }
}

/// The made-up "connected satellites" figure for this moment: steady for
/// `RECOUNT_SECONDS`, then a new roll. Purely decorative, nothing is measured.
fn satellite_count(unix_seconds: u64) -> u64 {
    let (low, high) = SATELLITE_RANGE;
    let mut dice = Dice::new(unix_seconds / RECOUNT_SECONDS);
    dice.next();
    low + dice.below(high - low + 1)
}

/// Random orbits, rolled once per start.
static ORBITS: LazyLock<Vec<Orbit>> = LazyLock::new(|| {
    let mut dice = Dice::seeded();
    (0..SATELLITES).map(|_| Orbit::roll(&mut dice)).collect()
});

/// A circular orbit around the globe's centre. Distances are in globe radii.
#[derive(Clone, Copy, Debug)]
struct Orbit {
    /// Tilt of the orbit away from the equator, radians.
    inclination: f32,
    /// Where the orbit crosses the equator going north, radians.
    node: f32,
    altitude: f32,
    /// Seconds per lap; negative goes the other way round.
    period: f64,
    phase: f64,
}

impl Orbit {
    fn roll(dice: &mut Dice) -> Self {
        let direction = if dice.below(2) == 0 { 1.0 } else { -1.0 };
        Self {
            inclination: dice.between(0.15, 1.4) as f32,
            node: dice.between(0.0, std::f64::consts::TAU) as f32,
            altitude: dice.between(1.08, 1.22) as f32,
            period: direction * dice.between(18.0, 40.0),
            phase: dice.between(0.0, 1.0),
        }
    }

    /// Position in space at `time` seconds, `lag` radians back along the orbit.
    fn position(&self, time: f64, lag: f32) -> [f32; 3] {
        let turns = (time / self.period + self.phase).fract();
        let angle = (turns * std::f64::consts::TAU) as f32 - lag * self.period.signum() as f32;
        let (sin, cos) = angle.sin_cos();
        // In the equator plane, then tilted about the x axis, then turned to the node.
        let (x, y, z) = (cos, 0.0_f32, sin);
        let (si, ci) = self.inclination.sin_cos();
        let (y, z) = (y * ci - z * si, y * si + z * ci);
        let (sn, cn) = self.node.sin_cos();
        let (x, z) = (x * cn + z * sn, z * cn - x * sn);
        [x * self.altitude, y * self.altitude, z * self.altitude]
    }

    /// True while the globe hides it: on the far side and inside the disc.
    fn behind(&self, view: &View, time: f64) -> bool {
        hidden(view, self.position(time, 0.0))
    }

    /// A bright point with a short fading trail; nothing where the globe covers it.
    fn draw(&self, painter: &Painter, view: &View, time: f64, colour: Color32) {
        const TRAIL: usize = 8;
        for step in (1..=TRAIL).rev() {
            let point = self.position(time, step as f32 * 0.045);
            if hidden(view, point) {
                continue;
            }
            let fade = 0.55 * (1.0 - step as f32 / (TRAIL as f32 + 1.0));
            painter.circle_filled(view.project(point).0, 1.2, colour.gamma_multiply(fade));
        }
        let point = self.position(time, 0.0);
        if hidden(view, point) {
            return;
        }
        let (at, _) = view.project(point);
        painter.circle_filled(at, 2.2, colour);
        // Solar panels either side.
        let wing = Stroke::new(1.0, colour.gamma_multiply(0.8));
        painter.line_segment([at - vec2(5.0, 0.0), at - vec2(3.0, 0.0)], wing);
        painter.line_segment([at + vec2(3.0, 0.0), at + vec2(5.0, 0.0)], wing);
    }
}

fn hidden(view: &View, point: [f32; 3]) -> bool {
    let (at, depth) = view.project(point);
    depth < 0.0 && at.distance(view.centre) < view.radius
}

#[derive(Clone, Copy)]
struct View {
    spin: f32,
    centre: Pos2,
    radius: f32,
}

impl View {
    /// Turn, tilt and project. The third value is depth, positive on the near side.
    fn project(&self, point: [f32; 3]) -> (Pos2, f32) {
        let [x, y, z] = point;
        let (sin, cos) = self.spin.sin_cos();
        let (x, z) = (x * cos + z * sin, z * cos - x * sin);
        let (sin, cos) = TILT_DEGREES.to_radians().sin_cos();
        let (y, z) = (y * cos - z * sin, y * sin + z * cos);
        (self.centre + vec2(x, -y) * self.radius, z)
    }
}

/// North up, longitude 0 facing the viewer before spinning.
fn to_unit_vector(latitude: f32, longitude: f32) -> [f32; 3] {
    let (lat, lon) = (latitude.to_radians(), longitude.to_radians());
    [lat.cos() * lon.sin(), lat.sin(), lat.cos() * lon.cos()]
}

/// All land dots in one mesh, fading towards the edge for a sense of depth.
fn land_mesh(view: &View, colour: Color32) -> Shape {
    let mut mesh = Mesh::default();
    let size = (view.radius / 70.0).clamp(1.0, 2.2);
    for point in LAND.iter() {
        let (at, depth) = view.project(*point);
        if depth <= 0.0 {
            continue;
        }
        let shade = colour.gamma_multiply(0.25 + 0.75 * depth);
        mesh.add_colored_rect(Rect::from_center_size(at, vec2(size, size)), shade);
    }
    Shape::mesh(std::sync::Arc::new(mesh))
}

/// A located host: a bright point and ring, or a faint one on the far side.
fn marker(painter: &Painter, view: &View, point: GeoPoint, colour: Color32) {
    let (at, depth) = view.project(to_unit_vector(point.latitude, point.longitude));
    if depth <= 0.0 {
        painter.circle_filled(at, 1.5, colour.gamma_multiply(0.25));
        return;
    }
    painter.circle_filled(at, 2.5, colour);
    painter.circle_stroke(at, 6.0, Stroke::new(1.0, colour.gamma_multiply(0.7)));
}

fn traffic(painter: &Painter, rect: Rect, snapshot: &NetSnapshot, scroll: f32, style: &Style<'_>) {
    let rates = format!("UP {} DOWN {}", format::mb_rate(snapshot.up_rate), format::mb_rate(snapshot.down_rate));
    let (inner, content) = panel::titled(painter, rect, PanelId::Traffic.title(), Some(&rates), style);
    let theme = style.theme;
    let font = style.caption();
    inner.text(content.left_top(), Align2::LEFT_TOP, "TOTAL", font.clone(), theme.muted);
    let totals = format!("{} OUT, {} IN", format::bytes(snapshot.total_up), format::bytes(snapshot.total_down));
    inner.text(content.right_top(), Align2::RIGHT_TOP, totals, font.clone(), theme.muted);

    let area = Rect::from_min_max(content.min + vec2(0.0, font.size * 1.5), content.max);
    if area.height() < 20.0 {
        return;
    }
    grid(&inner, area, style);

    let scale = graph_scale(&snapshot.up_history, &snapshot.down_history);
    let clip = inner.with_clip_rect(area);
    let half = Rect::from_min_max(area.min, pos2(area.max.x, area.center().y));
    let up: Vec<f32> = snapshot.up_history.iter().map(|value| value / scale * 100.0).collect();
    clip.add(Shape::line(graph_points(half, &up, scroll, TRAFFIC_HISTORY), Stroke::new(1.5, theme.foreground)));

    // Download mirrors below the centre line on the same scale.
    let lower = Rect::from_min_max(pos2(area.min.x, area.center().y), area.max);
    let down: Vec<f32> = snapshot.down_history.iter().map(|value| 100.0 - value / scale * 100.0).collect();
    clip.add(Shape::line(graph_points(lower, &down, scroll, TRAFFIC_HISTORY), Stroke::new(1.5, theme.muted)));

    let label = format!("{:.2}", scale / 1_000_000.0);
    let small = style.caption();
    inner.text(area.right_top(), Align2::RIGHT_TOP, &label, small.clone(), theme.muted);
    inner.text(area.right_bottom(), Align2::RIGHT_BOTTOM, &label, small, theme.muted);
}

/// Recessive reference lines, with the zero line brighter.
fn grid(painter: &Painter, area: Rect, style: &Style<'_>) {
    let theme = style.theme;
    let faint = Stroke::new(1.0, theme.grid);
    for i in 0..=4 {
        let x = area.min.x + area.width() * i as f32 / 4.0;
        painter.vline(x, area.y_range(), faint);
    }
    for i in 0..=6 {
        let y = area.min.y + area.height() * i as f32 / 6.0;
        painter.hline(area.x_range(), y, faint);
    }
    painter.hline(area.x_range(), area.center().y, Stroke::new(1.0, theme.rule));
}

/// The largest recent rate in either direction, shared so that up and down
/// are directly comparable.
fn graph_scale(up: &[f32], down: &[f32]) -> f32 {
    up.iter().chain(down).copied().fold(MIN_SCALE, f32::max)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::theme::Theme;
    use crate::ui::layout::compute;
    use egui::epaint::ClippedShape;
    use egui::{LayerId, Pos2};

    fn render(size: (f32, f32), snapshot: &NetSnapshot) -> Vec<String> {
        let ctx = egui::Context::default();
        let config = Config::default();
        let theme = Theme::tron();
        let window = Rect::from_min_size(Pos2::ZERO, vec2(size.0, size.1));
        let input = egui::RawInput { screen_rect: Some(window), ..Default::default() };
        let mut output = ctx.run_ui(input, |ui| {
            let painter = Painter::new(ui.ctx().clone(), LayerId::background(), window);
            let style = Style { theme: &theme, fonts: &config.fonts };
            draw(&painter, &compute(window, &config.fonts, &config.panels), snapshot, 12.5, true, &style);
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

    fn online() -> NetSnapshot {
        NetSnapshot {
            link: Link::Online,
            interface: Some("Ethernet".to_owned()),
            ipv4: Some("192.168.0.5".parse().unwrap()),
            external_ip: Some("203.0.113.7".parse().unwrap()),
            latency_ms: Some(22),
            up_history: vec![1_000_000.0; 40],
            down_history: vec![2_000_000.0; 40],
            up_rate: Some(1_000_000.0),
            down_rate: Some(2_000_000.0),
            total_up: Some(5_000_000),
            total_down: Some(3_000_000_000),
            connections: Some(14),
            points: vec![GeoPoint { latitude: 51.5, longitude: -0.1 }],
            geoip_loaded: true,
            sampled_at: None,
        }
    }

    #[test]
    fn an_empty_snapshot_reads_offline_or_na_without_failing() {
        let text = render((1600.0, 900.0), &NetSnapshot::default());
        assert!(text.iter().any(|t| t == "NO GEOIP DATABASE"), "{text:?}");
        assert!(text.iter().any(|t| t == "OFFLINE"), "{text:?}");
        assert!(text.iter().any(|t| t == "CONNECTIONS N/A"), "{text:?}");
        assert!(text.iter().any(|t| t == "UP N/A DOWN N/A"), "{text:?}");
    }

    #[test]
    fn a_full_snapshot_shows_its_values() {
        let text = render((1600.0, 900.0), &online());
        let expected = [
            "ONLINE",
            "Interface: Ethernet",
            "192.168.0.5",
            "203.0.113.7",
            "22ms",
            "CONNECTIONS 14",
            "LOCATED 1",
            "UP 1.00 DOWN 2.00",
            "5.0 MB OUT, 3.0 GB IN",
        ];
        for item in expected {
            assert!(text.iter().any(|t| t == item), "{item} missing from {text:?}");
        }
    }

    #[test]
    fn panels_survive_every_size_and_state() {
        let mut local = online();
        local.link = Link::LocalOnly;
        local.latency_ms = None;
        local.external_ip = None;
        let mut huge = online();
        huge.up_history = vec![f32::MAX; 3];
        for size in [(1.0, 1.0), (200.0, 150.0), (1024.0, 680.0), (3840.0, 2160.0)] {
            for snapshot in [&NetSnapshot::default(), &online(), &local, &huge] {
                render(size, snapshot);
            }
        }
    }

    #[test]
    fn built_in_land_data_is_where_the_continents_are() {
        assert!(LAND.len() > 3_000, "{}", LAND.len());
        let near = |latitude: f32, longitude: f32| {
            let target = to_unit_vector(latitude, longitude);
            let dot = |p: &[f32; 3]| p[0] * target[0] + p[1] * target[1] + p[2] * target[2];
            LAND.iter().any(|point| dot(point) > 3.0_f32.to_radians().cos())
        };
        assert!(near(40.0, -100.0), "central USA");
        assert!(near(0.0, 20.0), "central Africa");
        assert!(near(-25.0, 135.0), "central Australia");
        assert!(!near(0.0, -30.0), "mid Atlantic");
        assert!(!near(-40.0, -130.0), "south Pacific");
    }

    #[test]
    fn projection_keeps_points_on_the_disc_and_hides_the_far_side() {
        let view = View { spin: 0.0, centre: pos2(100.0, 100.0), radius: 50.0 };
        for point in LAND.iter() {
            let (at, _) = view.project(*point);
            assert!(at.distance(view.centre) <= 50.01);
        }
        let (_, near) = view.project(to_unit_vector(TILT_DEGREES, 0.0));
        let (_, far) = view.project(to_unit_vector(-TILT_DEGREES, 180.0));
        assert!(near > 0.99, "{near}");
        assert!(far < -0.99, "{far}");
    }

    #[test]
    fn spinning_moves_the_globe_eastward() {
        let turned = View { spin: 0.5, centre: Pos2::ZERO, radius: 1.0 };
        let still = View { spin: 0.0, centre: Pos2::ZERO, radius: 1.0 };
        let point = to_unit_vector(0.0, 0.0);
        assert!(turned.project(point).0.x > still.project(point).0.x);
    }

    #[test]
    fn the_globe_shows_a_satellite_count() {
        let text = render((1600.0, 900.0), &online());
        assert!(text.iter().any(|line| line.starts_with("CONNECTED SATELLITES ")), "{text:?}");
    }

    #[test]
    fn the_satellite_count_stays_in_range_and_holds_for_ten_minutes() {
        let mut seen = std::collections::BTreeSet::new();
        for slot in 0..2_000u64 {
            let start = 1_700_000_000 / RECOUNT_SECONDS * RECOUNT_SECONDS + slot * RECOUNT_SECONDS;
            let count = satellite_count(start);
            assert!((10..=40).contains(&count), "{count}");
            assert_eq!(satellite_count(start + RECOUNT_SECONDS - 1), count);
            seen.insert(count);
        }
        assert_eq!(seen.len(), 31, "every value turns up eventually");
    }

    #[test]
    fn orbits_stay_inside_the_room_left_around_the_globe() {
        let view = View { spin: 0.0, centre: Pos2::ZERO, radius: GLOBE_SHARE };
        for seed in 0..50 {
            let orbit = Orbit::roll(&mut Dice::new(seed));
            for tenth in 0..400 {
                let point = orbit.position(f64::from(tenth) / 10.0, 0.0);
                let height = point.iter().map(|v| v * v).sum::<f32>().sqrt();
                assert!((height - orbit.altitude).abs() < 1e-4);
                assert!(height > 1.0, "never inside the globe");
                assert!(view.project(point).0.distance(Pos2::ZERO) <= 1.0, "fits the space the full-size globe used");
            }
        }
    }

    #[test]
    fn satellites_hide_only_behind_the_globe() {
        let view = View { spin: 0.0, centre: Pos2::ZERO, radius: 1.0 };
        assert!(hidden(&view, to_unit_vector(-TILT_DEGREES, 180.0).map(|v| v * 1.1)));
        assert!(!hidden(&view, to_unit_vector(TILT_DEGREES, 0.0).map(|v| v * 1.1)));
        // Far side but out past the edge of the disc stays visible.
        assert!(!hidden(&view, [1.2, 0.0, -0.1]));
    }

    #[test]
    fn the_traffic_scale_is_shared_and_has_a_floor() {
        assert_eq!(graph_scale(&[], &[]), MIN_SCALE);
        assert_eq!(graph_scale(&[1.0], &[2.0]), MIN_SCALE);
        assert_eq!(graph_scale(&[9e6], &[3e6]), 9e6);
    }
}
