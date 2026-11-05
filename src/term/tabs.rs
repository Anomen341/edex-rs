//! The shell sessions behind the tab bar.

use crate::config::Config;
use crate::theme::TerminalColors;
use crate::ui::layout::TAB_COUNT;

use super::platform::shell_command;
use super::session::{GridSize, Session};

pub struct Tabs {
    slots: [Option<Session>; TAB_COUNT],
    active: usize,
    size: Option<GridSize>,
    shell: Option<Vec<String>>,
    scrollback: usize,
    colours: TerminalColors,
    ctx: egui::Context,
}

impl Tabs {
    pub fn new(config: &Config, colours: &TerminalColors, ctx: &egui::Context) -> Self {
        Self {
            slots: Default::default(),
            active: 0,
            size: None,
            shell: config.shell.clone(),
            scrollback: config.scrollback,
            colours: colours.clone(),
            ctx: ctx.clone(),
        }
    }

    /// Apply the current grid size. The main shell starts on the first call,
    /// because a shell should be born knowing its real size.
    pub fn resize(&mut self, size: GridSize) {
        self.size = Some(size);
        if self.slots[0].is_none() {
            self.open(0);
        }
        for session in self.slots.iter_mut().flatten() {
            session.resize(size);
        }
    }

    pub fn open(&mut self, index: usize) {
        let Some(size) = self.size else { return };
        let launch = shell_command(self.shell.as_deref());
        let session = Session::spawn(launch, size, self.scrollback, &self.colours, &self.ctx);
        self.slots[index] = Some(session);
        self.active = index;
    }

    pub fn open_next_free(&mut self) {
        if let Some(index) = self.slots.iter().position(Option::is_none) {
            self.open(index);
        }
    }

    /// The main shell cannot be closed, only restarted.
    pub fn close(&mut self, index: usize) {
        if index == 0 {
            return;
        }
        self.slots[index] = None;
        if self.active == index {
            self.active = (0..index).rev().find(|i| self.slots[*i].is_some()).unwrap_or(0);
        }
    }

    /// Remove extra tabs whose shell exited cleanly, as after typing `exit`.
    /// A failure stays open so its message can be read.
    pub fn close_finished(&mut self) {
        for index in 1..TAB_COUNT {
            let finished = self.slots[index].as_ref().and_then(Session::ended).is_some_and(|e| e.clean);
            if finished {
                self.close(index);
            }
        }
    }

    pub fn select(&mut self, index: usize) {
        if self.slots[index].is_some() {
            self.active = index;
        }
    }

    /// Move through the open tabs, wrapping at the ends.
    pub fn cycle(&mut self, forward: bool) {
        let step = if forward { 1 } else { TAB_COUNT - 1 };
        let mut index = self.active;
        for _ in 0..TAB_COUNT {
            index = (index + step) % TAB_COUNT;
            if self.slots[index].is_some() {
                self.active = index;
                return;
            }
        }
    }

    pub fn is_open(&self, index: usize) -> bool {
        self.slots[index].is_some()
    }

    pub fn active_index(&self) -> usize {
        self.active
    }

    pub fn active(&self) -> Option<&Session> {
        self.slots[self.active].as_ref()
    }

    /// Restart the active tab's shell after it ended.
    pub fn restart_active(&mut self) {
        self.open(self.active);
    }

    pub fn sessions(&self) -> impl Iterator<Item = &Session> {
        self.slots.iter().flatten()
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use crate::theme::Theme;

    fn tabs() -> Tabs {
        let config = Config { shell: Some(vec!["/bin/sh".to_owned()]), ..Config::default() };
        let colours = Theme::tron().terminal;
        let mut tabs = Tabs::new(&config, &colours, &egui::Context::default());
        tabs.resize(GridSize { cols: 80, rows: 24, cell_width: 10, cell_height: 20 });
        tabs
    }

    #[test]
    fn main_shell_opens_with_the_first_size() {
        let tabs = tabs();
        assert!(tabs.is_open(0));
        assert_eq!(tabs.active_index(), 0);
    }

    #[test]
    fn there_are_at_most_five_tabs() {
        let mut tabs = tabs();
        for _ in 0..10 {
            tabs.open_next_free();
        }
        assert_eq!(tabs.sessions().count(), TAB_COUNT);
        assert_eq!(tabs.active_index(), TAB_COUNT - 1);
    }

    #[test]
    fn closing_falls_back_to_a_neighbour_and_main_stays() {
        let mut tabs = tabs();
        tabs.open_next_free();
        tabs.open_next_free();
        tabs.close(2);
        assert_eq!(tabs.active_index(), 1);
        tabs.close(0);
        assert!(tabs.is_open(0));
    }

    #[test]
    fn cycling_skips_empty_slots_and_wraps() {
        let mut tabs = tabs();
        tabs.open(3);
        tabs.cycle(true);
        assert_eq!(tabs.active_index(), 0);
        tabs.cycle(false);
        assert_eq!(tabs.active_index(), 3);
    }
}
