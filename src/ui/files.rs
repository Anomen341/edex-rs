//! The file browser. It shows the active terminal's folder. Clicking a folder
//! changes to it in the shell, and clicking a file types its path at the prompt.

use std::path::{Path, PathBuf};

use egui::{pos2, vec2, Align2, Context, CursorIcon, Event, FontId, MouseWheelUnit, Painter, PointerButton, Pos2, Rect, Stroke};

use super::layout::{Layout, PanelId};
use super::panel::{self, Style};
use crate::files::{Entry, FileWorker, Listing};
use crate::stats::format;
use crate::term::session::Session;
use crate::term::tabs::Tabs;
use crate::term::{cd_command, quote_path};

const CELL_WIDTH: f32 = 190.0;
const ICON: f32 = 13.0;
const CHAR_WIDTH: f32 = 0.6;

/// What a click on a cell does.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Item {
    Parent(PathBuf),
    Folder(PathBuf),
    File(PathBuf),
}

pub struct FilesView {
    worker: FileWorker,
    requested: Option<PathBuf>,
    listing: Option<Listing>,
    /// First row shown.
    scroll: usize,
    wheel_remainder: f32,
}

/// Where cells go inside the panel.
struct Grid {
    area: Rect,
    columns: usize,
    rows: usize,
    row_height: f32,
    cell_width: f32,
}

impl Grid {
    fn new(area: Rect, row_height: f32) -> Self {
        let columns = ((area.width() / CELL_WIDTH) as usize).max(1);
        let rows = ((area.height() / row_height) as usize).max(1);
        Self { area, columns, rows, row_height, cell_width: area.width() / columns as f32 }
    }

    fn cell_rect(&self, visible_index: usize) -> Rect {
        let (row, column) = (visible_index / self.columns, visible_index % self.columns);
        let min = self.area.min + vec2(column as f32 * self.cell_width, row as f32 * self.row_height);
        Rect::from_min_size(min, vec2(self.cell_width, self.row_height))
    }

    /// The position in the visible cells under `pos`, if any.
    fn index_at(&self, pos: Pos2) -> Option<usize> {
        if !self.area.contains(pos) {
            return None;
        }
        let column = ((pos.x - self.area.min.x) / self.cell_width) as usize;
        let row = ((pos.y - self.area.min.y) / self.row_height) as usize;
        (column < self.columns && row < self.rows).then_some(row * self.columns + column)
    }

    fn last_scroll(&self, items: usize) -> usize {
        items.div_ceil(self.columns).saturating_sub(self.rows)
    }
}

impl FilesView {
    pub fn new(ctx: &Context) -> Self {
        Self { worker: FileWorker::spawn(ctx), requested: None, listing: None, scroll: 0, wheel_remainder: 0.0 }
    }

    pub fn show(&mut self, ctx: &Context, painter: &Painter, layout: &Layout, tabs: &Tabs, style: &Style<'_>) {
        let Some(rect) = layout.panels.iter().find(|(id, _)| *id == PanelId::Files).map(|(_, r)| *r) else {
            return;
        };
        let session = tabs.active();
        let cwd = session.and_then(Session::cwd);
        self.follow(cwd.as_ref());

        let shown = cwd.as_deref().map(|path| path.display().to_string());
        let chars = ((rect.width() * 0.75) / (style.caption().size * 0.5)) as usize;
        let status = shown.as_deref().map(|path| shorten_path(path, chars));
        let (inner, content) = panel::titled(painter, rect, PanelId::Files.title(), status.as_deref(), style);

        let Some(path) = cwd else {
            let message = if session.is_some() { "NOT TRACKING THIS SHELL. SEE DOCS/SHELL-INTEGRATION.MD" } else { "NO SHELL" };
            message_at(&inner, content, message, style);
            return;
        };
        let Some(listing) = self.listing.as_ref().filter(|listing| listing.path == path) else {
            message_at(&inner, content, "READING...", style);
            return;
        };
        let entries = match &listing.entries {
            Ok(entries) => entries,
            Err(reason) => {
                message_at(&inner, content, reason, style);
                return;
            }
        };
        let items = build_items(&path, entries);
        let skipped = listing.skipped;
        let row_height = style.fonts.label * 1.55;
        let footer = if skipped > 0 { row_height } else { 0.0 };
        let area = Rect::from_min_max(content.min, content.max - vec2(8.0, footer));
        let grid = Grid::new(area, row_height);

        self.handle_input(ctx, &grid, &items, session);
        self.draw_items(ctx, &inner, &grid, &items, style);
        if skipped > 0 {
            let text = format!("{skipped} MORE NOT SHOWN");
            inner.text(content.right_bottom(), Align2::RIGHT_BOTTOM, text, style.caption(), style.theme.muted);
        }
    }

    fn follow(&mut self, cwd: Option<&PathBuf>) {
        if let Some(listing) = self.worker.latest() {
            self.listing = Some(listing);
        }
        if cwd != self.requested.as_ref() {
            self.requested = cwd.cloned();
            self.scroll = 0;
            if let Some(path) = cwd {
                self.worker.request(path.clone());
            }
        }
    }

    fn handle_input(&mut self, ctx: &Context, grid: &Grid, items: &[Item], session: Option<&Session>) {
        let (events, pointer) = ctx.input(|i| (i.events.clone(), i.pointer.latest_pos()));
        self.scroll = self.scroll.min(grid.last_scroll(items.len()));
        for event in events {
            match event {
                Event::PointerButton { pos, button: PointerButton::Primary, pressed: true, .. } => {
                    let item = grid.index_at(pos).and_then(|i| items.get(i + self.scroll * grid.columns));
                    if let (Some(item), Some(session)) = (item, session) {
                        activate(item, session);
                    }
                }
                Event::MouseWheel { unit, delta, .. } if pointer.is_some_and(|p| grid.area.contains(p)) => {
                    let rows = match unit {
                        MouseWheelUnit::Line => delta.y,
                        MouseWheelUnit::Point => delta.y / grid.row_height,
                        MouseWheelUnit::Page => delta.y * grid.rows as f32,
                    } + self.wheel_remainder;
                    self.wheel_remainder = rows.fract();
                    let moved = self.scroll as i64 - rows.trunc() as i64;
                    self.scroll = moved.clamp(0, grid.last_scroll(items.len()) as i64) as usize;
                }
                _ => {}
            }
        }
    }

    fn draw_items(&self, ctx: &Context, painter: &Painter, grid: &Grid, items: &[Item], style: &Style<'_>) {
        let theme = style.theme;
        let hovered = ctx.input(|i| i.pointer.latest_pos()).and_then(|pos| grid.index_at(pos));
        let font = FontId::monospace(style.fonts.label);
        let first = self.scroll * grid.columns;
        let chars = ((grid.cell_width - ICON - 14.0) / (style.fonts.label * CHAR_WIDTH)) as usize;

        for (index, item) in items.iter().skip(first).take(grid.columns * grid.rows).enumerate() {
            let cell = grid.cell_rect(index);
            if hovered == Some(index) {
                painter.rect_filled(cell.shrink2(vec2(2.0, 1.0)), 0.0, theme.grid);
                ctx.set_cursor_icon(CursorIcon::PointingHand);
            }
            let is_folder = !matches!(item, Item::File(_));
            icon(painter, cell, is_folder, style);
            let colour = if is_folder { theme.foreground } else { theme.muted };
            let at = pos2(cell.min.x + ICON + 8.0, cell.center().y);
            let name = format::truncate(&item_name(item), chars);
            painter.text(at, Align2::LEFT_CENTER, name, font.clone(), colour);
        }

        // Where this page sits in a long folder.
        let last = grid.last_scroll(items.len());
        if last > 0 {
            let total = (last + grid.rows) as f32;
            let height = (grid.area.height() * grid.rows as f32 / total).max(14.0);
            let top = grid.area.min.y + (grid.area.height() - height) * self.scroll as f32 / last as f32;
            let bar = Rect::from_min_size(pos2(grid.area.max.x + 5.0, top), vec2(3.0, height));
            painter.rect_filled(bar, 0.0, theme.foreground);
        }
    }
}

fn build_items(path: &Path, entries: &[Entry]) -> Vec<Item> {
    let parent = path.parent().map(|parent| Item::Parent(parent.to_owned()));
    let children = entries.iter().map(|entry| {
        let full = path.join(&entry.name);
        if entry.is_dir { Item::Folder(full) } else { Item::File(full) }
    });
    parent.into_iter().chain(children).collect()
}

fn item_name(item: &Item) -> String {
    match item {
        Item::Parent(_) => "..".to_owned(),
        Item::Folder(path) | Item::File(path) => {
            path.file_name().map_or_else(String::new, |name| name.to_string_lossy().into_owned())
        }
    }
}

/// Folders change directory, but not inside a full-screen program, where the
/// command would be typed into it. A file's path is typed for the user to use.
fn activate(item: &Item, session: &Session) {
    use alacritty_terminal::term::TermMode;
    if session.ended().is_some() {
        return;
    }
    match item {
        Item::Parent(path) | Item::Folder(path) => {
            if !session.mode().contains(TermMode::ALT_SCREEN) {
                session.send(cd_command(session.shell(), path));
            }
        }
        Item::File(path) => {
            let text = format!("{} ", quote_path(session.shell(), path));
            session.send(text.into_bytes());
        }
    }
}

fn icon(painter: &Painter, cell: Rect, is_folder: bool, style: &Style<'_>) {
    let theme = style.theme;
    let area = Rect::from_center_size(pos2(cell.min.x + 4.0 + ICON * 0.5, cell.center().y), vec2(ICON, ICON * 0.8));
    if is_folder {
        let tab = Rect::from_min_size(area.min - vec2(0.0, 2.0), vec2(ICON * 0.45, 2.0));
        painter.rect_filled(tab, 0.0, theme.foreground);
        painter.rect_filled(area, 0.0, theme.foreground);
    } else {
        let page = Rect::from_center_size(area.center(), vec2(ICON * 0.7, ICON));
        painter.rect_stroke(page, 0.0, Stroke::new(1.0, theme.muted), egui::StrokeKind::Inside);
    }
}

fn message_at(painter: &Painter, area: Rect, text: &str, style: &Style<'_>) {
    painter.text(area.center(), Align2::CENTER_CENTER, text, style.heading(), style.theme.muted);
}

/// Keep the end of a long path, which is the part that identifies it.
fn shorten_path(path: &str, max: usize) -> String {
    let count = path.chars().count();
    if count <= max {
        return path.to_owned();
    }
    let tail: String = path.chars().skip(count + 1 - max.max(1)).collect();
    format!("…{tail}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(name: &str, is_dir: bool) -> Entry {
        Entry { name: name.to_owned(), is_dir }
    }

    fn grid() -> Grid {
        Grid::new(Rect::from_min_size(pos2(100.0, 100.0), vec2(400.0, 100.0)), 25.0)
    }

    #[test]
    fn folders_and_files_become_items_with_a_parent_first() {
        let items = build_items(Path::new("/home/me"), &[entry("docs", true), entry("a.txt", false)]);
        assert_eq!(items[0], Item::Parent("/home".into()));
        assert_eq!(items[1], Item::Folder("/home/me/docs".into()));
        assert_eq!(items[2], Item::File("/home/me/a.txt".into()));
    }

    #[test]
    fn the_root_has_no_parent_entry() {
        assert!(build_items(Path::new("/"), &[]).is_empty());
    }

    #[test]
    fn names_show_the_last_part_only() {
        assert_eq!(item_name(&Item::Parent("/home".into())), "..");
        assert_eq!(item_name(&Item::Folder("/home/me/docs".into())), "docs");
    }

    #[test]
    fn cells_map_to_clicks_and_back() {
        let grid = grid();
        assert_eq!((grid.columns, grid.rows), (2, 4));
        for index in 0..8 {
            assert_eq!(grid.index_at(grid.cell_rect(index).center()), Some(index));
        }
        assert_eq!(grid.index_at(pos2(0.0, 0.0)), None);
    }

    #[test]
    fn scrolling_stops_at_the_last_page() {
        let grid = grid();
        assert_eq!(grid.last_scroll(0), 0);
        assert_eq!(grid.last_scroll(8), 0);
        assert_eq!(grid.last_scroll(9), 1);
        assert_eq!(grid.last_scroll(20), 6);
    }

    #[test]
    fn a_tiny_panel_still_has_a_cell() {
        let grid = Grid::new(Rect::from_min_size(Pos2::ZERO, vec2(1.0, 1.0)), 25.0);
        assert_eq!((grid.columns, grid.rows), (1, 1));
    }

    #[test]
    fn long_paths_keep_their_end() {
        assert_eq!(shorten_path("/short", 20), "/short");
        assert_eq!(shorten_path("/a/very/long/path/to/here", 10), "…h/to/here");
        assert_eq!(shorten_path("/日本語/フォルダ", 5), "…フォルダ");
    }
}
