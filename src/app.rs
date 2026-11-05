//! Top-level application state and per-frame drawing.

use std::sync::mpsc::{Receiver, TryRecvError};
use std::sync::Arc;
use std::time::Duration;

use chrono::{Local, Timelike};
use eframe::glow::HasContext;
use egui::{FontData, FontDefinitions, FontFamily};

use crate::config::Config;
use crate::stats::{self, Snapshot};
use crate::term::tabs::Tabs;
use crate::theme::Theme;
use crate::ui::panel::{Style, DISPLAY_FONT};
use crate::ui::terminal::{TerminalView, BOLD, BOLD_ITALIC, ITALIC};
use crate::ui::{self, layout, monitor};

/// Smooth graphs redraw about 30 times a second. That is nothing on a GPU, but
/// heavy on a software renderer, so they are off there. Frame cost is also
/// watched, for slow machines with a real GPU. It only counts this thread, not
/// a renderer's own threads, so it cannot replace the renderer check. The two
/// limits keep it from flipping back and forth around a single value.
const ANIMATE_DELAY: Duration = Duration::from_millis(33);
const STILL_DELAY: Duration = Duration::from_millis(500);
const FRAME_COST_TOO_HIGH: f32 = 0.012;
const FRAME_COST_FINE: f32 = 0.006;

pub struct EdexApp {
    config: Config,
    theme: Theme,
    tabs: Tabs,
    terminal: TerminalView,
    stats: Receiver<Snapshot>,
    snapshot: Snapshot,
    software_renderer: bool,
    /// Recent CPU seconds per frame, smoothed.
    frame_cost: f32,
    animate: bool,
}

impl EdexApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        install_fonts(&cc.egui_ctx);
        // Ctrl+plus and Ctrl+minus belong to the shell.
        cc.egui_ctx.options_mut(|options| options.zoom_with_keyboard = false);
        let config = Config::default();
        let theme = Theme::tron();
        let tabs = Tabs::new(&config, &theme.terminal, &cc.egui_ctx);
        let terminal = TerminalView::new(config.fonts.terminal);
        let software_renderer = uses_software_renderer(cc);
        Self { config, theme, tabs, terminal, stats: stats::spawn(), snapshot: Snapshot::default(), software_renderer, frame_cost: 0.0, animate: !software_renderer }
    }

    /// Update the smoothed frame cost and decide whether to animate.
    fn track_frame_cost(&mut self, cost: f32) {
        self.frame_cost = self.frame_cost * 0.9 + cost * 0.1;
        self.animate = !self.software_renderer && next_animate(self.animate, self.frame_cost);
    }

    /// Take the newest snapshot without waiting. If the stats thread has gone,
    /// stale numbers would mislead, so the panels fall back to N/A.
    fn receive_stats(&mut self) {
        loop {
            match self.stats.try_recv() {
                Ok(snapshot) => self.snapshot = snapshot,
                Err(TryRecvError::Empty) => return,
                Err(TryRecvError::Disconnected) => {
                    self.snapshot = Snapshot::default();
                    return;
                }
            }
        }
    }
}

impl eframe::App for EdexApp {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.receive_stats();
        let now = Local::now();
        if let Some(cost) = frame.info().cpu_usage {
            self.track_frame_cost(cost);
        }
        // Whatever the rate, wake on the next second so the clock never skips.
        let to_next_second = Duration::from_millis(1_000 - u64::from(now.nanosecond() / 1_000_000).min(999));
        let delay = if self.animate { ANIMATE_DELAY } else { STILL_DELAY };
        ctx.request_repaint_after(delay.min(to_next_second));

        let window = ctx.content_rect();
        let painter = ui.painter();
        painter.rect_filled(window, 0.0, self.theme.background);

        let layout = layout::compute(window, &self.config.fonts);
        let style = Style { theme: &self.theme, fonts: &self.config.fonts };
        ui::draw_panels(painter, &layout, &style);
        monitor::draw(painter, &layout, &self.snapshot, now, self.config.twelve_hour_clock, self.animate, &style);
        self.terminal.show(&ctx, painter, &layout, &mut self.tabs, &style);
    }

    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        self.theme.background.to_normalized_gamma_f32()
    }
}

/// True when drawing runs on the CPU, as with Mesa's llvmpipe or Windows' WARP.
fn uses_software_renderer(cc: &eframe::CreationContext<'_>) -> bool {
    if let Some(state) = &cc.wgpu_render_state {
        return state.adapter.get_info().device_type == eframe::wgpu::DeviceType::Cpu;
    }
    cc.gl.as_ref().is_some_and(|gl| {
        // SAFETY: eframe made this context current before creating the app.
        let name = unsafe { gl.get_parameter_string(eframe::glow::RENDERER) };
        is_software_gl(&name)
    })
}

fn is_software_gl(renderer: &str) -> bool {
    let name = renderer.to_ascii_lowercase();
    ["llvmpipe", "softpipe", "software", "swrast", "gdi generic"].iter().any(|word| name.contains(word))
}

fn next_animate(animate: bool, frame_cost: f32) -> bool {
    if animate {
        frame_cost < FRAME_COST_TOO_HIGH
    } else {
        frame_cost < FRAME_COST_FINE
    }
}

/// Use the bundled font everywhere so both platforms look identical. Each style
/// keeps egui's fallbacks behind it for symbols the main font lacks.
fn install_fonts(ctx: &egui::Context) {
    const REGULAR: &str = "jetbrains-mono";
    let mut fonts = FontDefinitions::default();
    let add = |fonts: &mut FontDefinitions, name: &str, bytes: &'static [u8]| {
        fonts.font_data.insert(name.to_owned(), Arc::new(FontData::from_static(bytes)));
    };
    add(&mut fonts, REGULAR, include_bytes!("../assets/fonts/JetBrainsMonoNL-Regular.ttf"));
    add(&mut fonts, BOLD, include_bytes!("../assets/fonts/JetBrainsMonoNL-Bold.ttf"));
    add(&mut fonts, ITALIC, include_bytes!("../assets/fonts/JetBrainsMonoNL-Italic.ttf"));
    add(&mut fonts, BOLD_ITALIC, include_bytes!("../assets/fonts/JetBrainsMonoNL-BoldItalic.ttf"));
    add(&mut fonts, DISPLAY_FONT, include_bytes!("../assets/fonts/ShareTech-Regular.ttf"));

    let fallbacks = fonts.families.get(&FontFamily::Monospace).cloned().unwrap_or_default();
    for family in [FontFamily::Monospace, FontFamily::Proportional] {
        fonts.families.entry(family).or_default().insert(0, REGULAR.to_owned());
    }
    for name in [BOLD, ITALIC, BOLD_ITALIC, DISPLAY_FONT] {
        let mut list = vec![name.to_owned()];
        list.extend(fallbacks.iter().cloned());
        fonts.families.insert(FontFamily::Name(name.into()), list);
    }
    ctx.set_fonts(fonts);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn software_opengl_drivers_are_recognised() {
        assert!(is_software_gl("llvmpipe (LLVM 19.1.7, 256 bits)"));
        assert!(is_software_gl("GDI Generic"));
        assert!(!is_software_gl("NVIDIA GeForce RTX 3080/PCIe/SSE2"));
        assert!(!is_software_gl("AMD Radeon Graphics"));
    }

    #[test]
    fn animation_stops_on_slow_frames_and_needs_a_clear_margin_to_return() {
        assert!(next_animate(true, 0.002));
        assert!(!next_animate(true, 0.020));
        assert!(!next_animate(false, 0.009));
        assert!(next_animate(false, 0.003));
    }
}
