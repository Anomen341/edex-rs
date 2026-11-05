//! The startup sequence: a scrolling log, then the title. Any key or click
//! skips it, and the config can turn it off.

use egui::{pos2, Align2, Context, Event, FontFamily, FontId, Painter, Rect};

use super::panel::{rule, Style, DISPLAY_FONT};

const LOG_SECONDS: f64 = 2.6;
const TITLE_SECONDS: f64 = 1.2;
const TITLE: &str = "EDEX-RS";

const LOG: [&str; 26] = [
    "[    0.000000] boot: edex-rs core starting",
    "[    0.000412] cpu: enumerating logical processors",
    "[    0.001873] mem: mapping dot grid allocator",
    "[    0.004120] gpu: negotiating render surface",
    "[    0.009954] gpu: compositor online",
    "[    0.012207] font: loading glyph atlas",
    "[    0.018330] theme: applying colour tables",
    "[    0.021764] pty: opening pseudo terminal pool",
    "[    0.024081] pty: 5 session slots reserved",
    "[    0.031409] term: vt parser ready",
    "[    0.034672] term: scrollback ring allocated",
    "[    0.042118] input: keyboard matrix calibrated",
    "[    0.048855] input: pointer tracking enabled",
    "[    0.057311] stats: sampler thread started",
    "[    0.061940] stats: sensor bus scan complete",
    "[    0.070256] net: route table loaded",
    "[    0.074803] net: traffic meter started",
    "[    0.083447] net: connection table attached",
    "[    0.090125] geo: land model decoded",
    "[    0.096610] geo: projecting globe",
    "[    0.104372] fs: directory watcher armed",
    "[    0.112098] fs: tracking active shell",
    "[    0.120634] ui: panels aligned",
    "[    0.127451] ui: grid backdrop drawn",
    "[    0.135209] shell: handing over",
    "[    0.141006] boot: all systems nominal",
];

#[derive(Default)]
pub struct BootScreen {
    started: Option<f64>,
}

impl BootScreen {
    /// Draw a frame. Returns true when the sequence is over.
    pub fn show(&mut self, ctx: &Context, painter: &Painter, window: Rect, style: &Style<'_>) -> bool {
        let (time, skipped) = ctx.input(|i| (i.time, i.events.iter().any(is_skip)));
        let elapsed = time - *self.started.get_or_insert(time);
        if skipped || elapsed >= LOG_SECONDS + TITLE_SECONDS {
            return true;
        }
        if elapsed < LOG_SECONDS {
            draw_log(painter, window, visible_lines(elapsed), style);
        } else {
            draw_title(painter, window, (elapsed - LOG_SECONDS) / TITLE_SECONDS, style);
        }
        ctx.request_repaint();
        false
    }
}

fn is_skip(event: &Event) -> bool {
    matches!(event, Event::Key { pressed: true, .. } | Event::PointerButton { pressed: true, .. })
}

fn visible_lines(elapsed: f64) -> usize {
    (((elapsed / LOG_SECONDS) * LOG.len() as f64).ceil() as usize).clamp(1, LOG.len())
}

/// Newest line at the bottom, pushing the others up.
fn draw_log(painter: &Painter, window: Rect, count: usize, style: &Style<'_>) {
    let size = style.fonts.terminal;
    let line = size * 1.4;
    let area = window.shrink(24.0);
    let fits = ((area.height() / line) as usize).max(1);
    let first = count.saturating_sub(fits);
    let font = FontId::monospace(size);
    let bottom = area.max.y - style.fonts.label * 2.0;
    for (back, text) in LOG[first..count].iter().rev().enumerate() {
        let at = pos2(area.min.x, bottom - back as f32 * line);
        painter.text(at, Align2::LEFT_BOTTOM, *text, font.clone(), style.theme.foreground);
    }
    let hint = area.right_bottom();
    painter.text(hint, Align2::RIGHT_BOTTOM, "PRESS ANY KEY TO SKIP", style.caption(), style.theme.muted);
}

fn draw_title(painter: &Painter, window: Rect, progress: f64, style: &Style<'_>) {
    let size = (window.width() * 0.09).clamp(40.0, 140.0);
    let colour = style.theme.foreground.gamma_multiply(title_fade(progress));
    let font = FontId::new(size, FontFamily::Name(DISPLAY_FONT.into()));
    let spacing = size * 0.78;
    let width = spacing * TITLE.chars().count() as f32;
    let centre = window.center();
    let left = centre.x - width * 0.5;
    for (i, c) in TITLE.chars().enumerate() {
        let at = pos2(left + (i as f32 + 0.5) * spacing, centre.y);
        painter.text(at, Align2::CENTER_CENTER, c, font.clone(), colour);
    }
    let y = size * 0.75;
    let reach = width * 0.5 * (progress as f32 * 2.5).min(1.0);
    rule(painter, centre.x - reach, centre.x + reach, centre.y - y, style);
    rule(painter, centre.x - reach, centre.x + reach, centre.y + y, style);
}

/// Up quickly, hold, then down.
fn title_fade(progress: f64) -> f32 {
    let rise = (progress / 0.2).min(1.0);
    let fall = ((1.0 - progress) / 0.25).min(1.0);
    rise.min(fall).clamp(0.0, 1.0) as f32
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Key, Modifiers, PointerButton, Pos2};

    #[test]
    fn the_log_reveals_every_line_on_time() {
        assert_eq!(visible_lines(0.0), 1);
        assert_eq!(visible_lines(LOG_SECONDS / 2.0), LOG.len() / 2);
        assert_eq!(visible_lines(LOG_SECONDS), LOG.len());
        assert_eq!(visible_lines(LOG_SECONDS * 5.0), LOG.len());
    }

    #[test]
    fn the_title_fades_in_and_out() {
        assert_eq!(title_fade(0.0), 0.0);
        assert_eq!(title_fade(0.5), 1.0);
        assert_eq!(title_fade(1.0), 0.0);
    }

    #[test]
    fn a_press_or_click_skips_but_releases_and_movement_do_not() {
        let key = |pressed| Event::Key { key: Key::A, physical_key: None, pressed, repeat: false, modifiers: Modifiers::NONE };
        assert!(is_skip(&key(true)));
        assert!(!is_skip(&key(false)));
        let click = Event::PointerButton {
            pos: Pos2::ZERO,
            button: PointerButton::Primary,
            pressed: true,
            modifiers: Modifiers::NONE,
        };
        assert!(is_skip(&click));
        assert!(!is_skip(&Event::PointerMoved(Pos2::ZERO)));
    }
}
