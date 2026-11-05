//! A plain-data picture of the terminal grid, made while briefly holding the
//! terminal lock so drawing happens with the lock released.

use alacritty_terminal::event::EventListener;
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::Point;
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::term::color::Colors;
use alacritty_terminal::term::Term;
use alacritty_terminal::vte::ansi::{Color, CursorShape, NamedColor, Rgb};
use egui::Color32;

use crate::theme::TerminalColors;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextStyle {
    Regular,
    Bold,
    Italic,
    BoldItalic,
}

/// Text sharing one colour and style. Non-ASCII characters get a run of their
/// own so a fallback font's width cannot push later cells off the grid.
#[derive(Clone, Debug, PartialEq)]
pub struct Run {
    pub line: usize,
    pub col: usize,
    pub cells: usize,
    pub text: String,
    pub colour: Color32,
    pub style: TextStyle,
}

/// A horizontal span of cells: a background fill or a line through the text.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Span {
    pub line: usize,
    pub col: usize,
    pub cells: usize,
    pub colour: Color32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CursorKind {
    Block,
    Underline,
    Beam,
    Hollow,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Cursor {
    pub line: usize,
    pub col: usize,
    pub cells: usize,
    pub kind: CursorKind,
    pub colour: Color32,
}

#[derive(Clone, Debug, Default)]
pub struct Frame {
    pub fills: Vec<Span>,
    pub runs: Vec<Run>,
    pub underlines: Vec<Span>,
    pub strikes: Vec<Span>,
    pub cursor: Option<Cursor>,
    /// Lines scrolled back from the bottom.
    pub offset: usize,
    /// Lines of history available.
    pub history: usize,
    pub rows: usize,
}

fn push_span(spans: &mut Vec<Span>, span: Span) {
    if let Some(last) = spans.last_mut() {
        let touching = last.line == span.line && last.col + last.cells == span.col;
        if touching && last.colour == span.colour {
            last.cells += span.cells;
            return;
        }
    }
    spans.push(span);
}

pub fn build<T: EventListener>(term: &Term<T>, colours: &TerminalColors, focused: bool) -> Frame {
    let content = term.renderable_content();
    let offset = content.display_offset;
    let cursor_at = content.cursor.point;
    let cursor_shape = content.cursor.shape;
    let block_cursor = cursor_shape == CursorShape::Block && focused;
    let overrides = content.colors;
    let selection = content.selection;

    let mut frame = Frame {
        offset,
        history: term.grid().history_size(),
        rows: term.screen_lines(),
        ..Frame::default()
    };
    let mut cursor_cells = 1;

    for indexed in content.display_iter {
        let cell = indexed.cell;
        let point: Point = indexed.point;
        let Ok(line) = usize::try_from(point.line.0 + offset as i32) else { continue };
        let col = point.column.0;
        let flags = cell.flags;
        let width = if flags.contains(Flags::WIDE_CHAR) { 2 } else { 1 };
        let bold = flags.contains(Flags::BOLD);

        let mut fg = resolve(cell.fg, colours, overrides, bold);
        let mut bg = resolve(cell.bg, colours, overrides, false);
        if flags.contains(Flags::INVERSE) {
            std::mem::swap(&mut fg, &mut bg);
        }
        if flags.contains(Flags::DIM) {
            fg = blend(fg, bg, 0.34);
        }
        if flags.contains(Flags::HIDDEN) {
            fg = bg;
        }
        if selection.is_some_and(|range| range.contains(point)) {
            bg = colours.selection;
        }
        if point == cursor_at {
            cursor_cells = width;
            if block_cursor {
                fg = bg;
            }
        }

        if bg != colours.background {
            push_span(&mut frame.fills, Span { line, col, cells: width, colour: bg });
        }
        if flags.intersects(Flags::ALL_UNDERLINES) {
            let colour = cell
                .underline_color()
                .map_or(fg, |c| resolve(c, colours, overrides, false));
            push_span(&mut frame.underlines, Span { line, col, cells: width, colour });
        }
        if flags.contains(Flags::STRIKEOUT) {
            push_span(&mut frame.strikes, Span { line, col, cells: width, colour: fg });
        }

        let spacer = flags.intersects(Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER);
        if spacer || flags.contains(Flags::HIDDEN) {
            continue;
        }
        let style = match (bold, flags.contains(Flags::ITALIC)) {
            (false, false) => TextStyle::Regular,
            (true, false) => TextStyle::Bold,
            (false, true) => TextStyle::Italic,
            (true, true) => TextStyle::BoldItalic,
        };
        push_char(&mut frame.runs, line, col, width, cell.c, cell.zerowidth(), fg, style);
    }

    for run in &mut frame.runs {
        run.cells -= run.text.len() - run.text.trim_end_matches(' ').len();
        run.text.truncate(run.text.trim_end_matches(' ').len());
    }

    if cursor_shape != CursorShape::Hidden {
        let line = usize::try_from(cursor_at.line.0 + offset as i32).ok();
        if let Some(line) = line.filter(|line| *line < frame.rows) {
            let kind = match cursor_shape {
                _ if !focused => CursorKind::Hollow,
                CursorShape::Underline => CursorKind::Underline,
                CursorShape::Beam => CursorKind::Beam,
                CursorShape::HollowBlock => CursorKind::Hollow,
                _ => CursorKind::Block,
            };
            frame.cursor = Some(Cursor {
                line,
                col: cursor_at.column.0,
                cells: cursor_cells,
                kind,
                colour: resolve(Color::Named(NamedColor::Cursor), colours, overrides, false),
            });
        }
    }
    frame
}

#[allow(clippy::too_many_arguments)]
fn push_char(
    runs: &mut Vec<Run>,
    line: usize,
    col: usize,
    width: usize,
    c: char,
    zerowidth: Option<&[char]>,
    colour: Color32,
    style: TextStyle,
) {
    let plain = (' '..='~').contains(&c) && zerowidth.is_none();
    let last = runs.last_mut().filter(|run| {
        let touching = run.line == line && run.col + run.cells == col;
        touching && run.text.is_ascii() && plain
    });
    if let Some(run) = last {
        // A space has no ink, so it can join a run of any colour.
        if c == ' ' || (run.colour == colour && run.style == style) {
            run.text.push(c);
            run.cells += 1;
            return;
        }
    }
    if c == ' ' && zerowidth.is_none() {
        return;
    }
    let mut text = String::from(c);
    text.extend(zerowidth.unwrap_or_default());
    runs.push(Run { line, col, cells: width, text, colour, style });
}

fn from_rgb(rgb: Rgb) -> Color32 {
    Color32::from_rgb(rgb.r, rgb.g, rgb.b)
}

fn lookup(index: usize, colours: &TerminalColors, overrides: &Colors) -> Color32 {
    overrides[index].map_or_else(|| colours.by_index(index), from_rgb)
}

/// Bold on a basic colour means its bright version, as in xterm. Without this
/// `\e[1;30m` is black on black.
fn resolve(colour: Color, colours: &TerminalColors, overrides: &Colors, bold: bool) -> Color32 {
    let index = match colour {
        Color::Spec(rgb) => return from_rgb(rgb),
        Color::Indexed(i) => i as usize,
        Color::Named(name) => name as usize,
    };
    let dim_black = NamedColor::DimBlack as usize;
    let dim_white = NamedColor::DimWhite as usize;
    match index {
        0..=7 if bold => lookup(index + 8, colours, overrides),
        i if (dim_black..=dim_white).contains(&i) => {
            let base = lookup(i - dim_black, colours, overrides);
            blend(base, colours.background, 0.34)
        }
        i if i == NamedColor::DimForeground as usize => {
            blend(lookup(256, colours, overrides), colours.background, 0.34)
        }
        i if i == NamedColor::BrightForeground as usize => lookup(256, colours, overrides),
        i => lookup(i, colours, overrides),
    }
}

/// Mix `amount` of `other` into `colour`.
fn blend(colour: Color32, other: Color32, amount: f32) -> Color32 {
    let mix = |a: u8, b: u8| (f32::from(a) * (1.0 - amount) + f32::from(b) * amount) as u8;
    Color32::from_rgb(mix(colour.r(), other.r()), mix(colour.g(), other.g()), mix(colour.b(), other.b()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::term::session::GridSize;
    use crate::theme::Theme;
    use alacritty_terminal::event::VoidListener;
    use alacritty_terminal::term::Config;
    use alacritty_terminal::vte::ansi::Processor;

    fn frame_of(bytes: &[u8]) -> (Frame, TerminalColors) {
        let size = GridSize { cols: 20, rows: 5, cell_width: 10, cell_height: 20 };
        let mut term = Term::new(Config::default(), &size, VoidListener);
        let mut parser: Processor = Processor::new();
        parser.advance(&mut term, bytes);
        let colours = Theme::tron().terminal;
        (build(&term, &colours, true), colours)
    }

    fn run_with<'a>(frame: &'a Frame, text: &str) -> &'a Run {
        frame.runs.iter().find(|r| r.text == text).unwrap_or_else(|| panic!("{:?}", frame.runs))
    }

    #[test]
    fn text_with_one_style_becomes_one_run() {
        let (frame, colours) = frame_of(b"hello world");
        let run = run_with(&frame, "hello world");
        assert_eq!((run.line, run.col, run.cells), (0, 0, 11));
        assert_eq!(run.colour, colours.foreground);
        assert_eq!(run.style, TextStyle::Regular);
    }

    #[test]
    fn ansi_256_and_truecolour_foregrounds_resolve() {
        let (frame, colours) = frame_of(b"\x1b[31mA\x1b[38;5;196mB\x1b[38;2;1;2;3mC");
        assert_eq!(run_with(&frame, "A").colour, colours.ansi[1]);
        assert_eq!(run_with(&frame, "B").colour, Color32::from_rgb(255, 0, 0));
        assert_eq!(run_with(&frame, "C").colour, Color32::from_rgb(1, 2, 3));
    }

    #[test]
    fn bold_black_stays_visible() {
        let (frame, colours) = frame_of(b"\x1b[1;30mX");
        let run = run_with(&frame, "X");
        assert_eq!(run.colour, colours.ansi[8]);
        assert_eq!(run.style, TextStyle::Bold);
    }

    #[test]
    fn inverse_swaps_colours_and_fills_the_background() {
        let (frame, colours) = frame_of(b"\x1b[7mAB");
        assert_eq!(run_with(&frame, "AB").colour, colours.background);
        assert_eq!(frame.fills[0], Span { line: 0, col: 0, cells: 2, colour: colours.foreground });
    }

    #[test]
    fn wide_characters_take_two_cells_and_keep_the_grid_aligned() {
        let (frame, _) = frame_of("漢a".as_bytes());
        let wide = run_with(&frame, "漢");
        assert_eq!((wide.col, wide.cells), (0, 2));
        assert_eq!(run_with(&frame, "a").col, 2);
    }

    #[test]
    fn combining_marks_stay_with_their_character() {
        let (frame, _) = frame_of("e\u{301}x".as_bytes());
        assert_eq!(run_with(&frame, "e\u{301}").cells, 1);
        assert_eq!(run_with(&frame, "x").col, 1);
    }

    #[test]
    fn cursor_is_reported_and_follows_the_requested_shape() {
        let (frame, _) = frame_of(b"ab");
        let cursor = frame.cursor.unwrap();
        assert_eq!((cursor.line, cursor.col, cursor.kind), (0, 2, CursorKind::Block));
        let (frame, _) = frame_of(b"\x1b[6 q");
        assert_eq!(frame.cursor.unwrap().kind, CursorKind::Beam);
        let (frame, _) = frame_of(b"\x1b[?25l");
        assert!(frame.cursor.is_none());
    }

    #[test]
    fn underline_and_strikeout_are_collected() {
        let (frame, _) = frame_of(b"\x1b[4mab\x1b[0m\x1b[9mc");
        assert_eq!(frame.underlines.len(), 1);
        assert_eq!(frame.underlines[0].cells, 2);
        assert_eq!(frame.strikes[0].col, 2);
    }

    #[test]
    fn history_and_rows_are_tracked() {
        let (frame, _) = frame_of(b"1\r\n2\r\n3\r\n4\r\n5\r\n6\r\n7");
        assert_eq!(frame.rows, 5);
        assert_eq!(frame.history, 2);
        assert_eq!(run_with(&frame, "7").line, 4);
    }
}
