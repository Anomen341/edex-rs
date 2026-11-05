//! Top-level application state and per-frame drawing.

use std::sync::mpsc::{Receiver, TryRecvError};
use std::sync::Arc;
use std::time::Duration;

use chrono::{Local, Timelike};
use egui::{FontData, FontDefinitions, FontFamily};

use crate::config::Config;
use crate::stats::{self, Snapshot};
use crate::term::tabs::Tabs;
use crate::theme::Theme;
use crate::ui::panel::{Style, DISPLAY_FONT};
use crate::ui::terminal::{TerminalView, BOLD, BOLD_ITALIC, ITALIC};
use crate::ui::{self, layout, monitor};

pub struct EdexApp {
    config: Config,
    theme: Theme,
    tabs: Tabs,
    terminal: TerminalView,
    stats: Receiver<Snapshot>,
    snapshot: Snapshot,
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
        Self { config, theme, tabs, terminal, stats: stats::spawn(), snapshot: Snapshot::default() }
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
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.receive_stats();
        let now = Local::now();
        // Wake exactly on the next second so the clock never skips a digit.
        let to_next_second = 1_000 - u64::from(now.nanosecond() / 1_000_000).min(999);
        ctx.request_repaint_after(Duration::from_millis(to_next_second));

        let window = ctx.content_rect();
        let painter = ui.painter();
        painter.rect_filled(window, 0.0, self.theme.background);

        let layout = layout::compute(window, &self.config.fonts);
        let style = Style { theme: &self.theme, fonts: &self.config.fonts };
        ui::draw_panels(painter, &layout, &style);
        monitor::draw(painter, &layout, &self.snapshot, now, self.config.twelve_hour_clock, &style);
        self.terminal.show(&ctx, painter, &layout, &mut self.tabs, &style);
    }

    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        self.theme.background.to_normalized_gamma_f32()
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
