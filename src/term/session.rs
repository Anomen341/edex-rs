//! One shell in one PTY.
//!
//! The UI thread never touches the PTY. Reading, writing, resizing and waiting
//! for exit each run on their own thread, because any of them can block: a
//! program that stops reading its input fills the PTY buffer, and a blocked
//! write on the UI thread would freeze the whole window.

use std::io::{Read, Write};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Instant;

use alacritty_terminal::event::{Event, EventListener, WindowSize};
use alacritty_terminal::grid::{Dimensions, Scroll};
use alacritty_terminal::sync::FairMutex;
use alacritty_terminal::term::{Config, Term, TermMode};
use alacritty_terminal::vte::ansi::Processor;
use portable_pty::{native_pty_system, ChildKiller, CommandBuilder, MasterPty, PtySize};

use super::frame::{self, Frame};
use super::input;
use crate::theme::TerminalColors;

const READ_BUFFER: usize = 64 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GridSize {
    pub cols: usize,
    pub rows: usize,
    pub cell_width: u16,
    pub cell_height: u16,
}

impl Dimensions for GridSize {
    fn total_lines(&self) -> usize {
        self.rows
    }

    fn screen_lines(&self) -> usize {
        self.rows
    }

    fn columns(&self) -> usize {
        self.cols
    }
}

impl From<GridSize> for PtySize {
    fn from(size: GridSize) -> Self {
        PtySize {
            rows: size.rows as u16,
            cols: size.cols as u16,
            pixel_width: size.cols as u16 * size.cell_width,
            pixel_height: size.rows as u16 * size.cell_height,
        }
    }
}

/// How a session ended.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Ended {
    /// True if the shell exited normally with code 0.
    pub clean: bool,
    pub message: String,
}

/// State written by background threads and read by the UI.
struct Shared {
    ended: Mutex<Option<Ended>>,
    clipboard: Mutex<Option<String>>,
    size: Mutex<GridSize>,
}

/// Receives requests the terminal makes while parsing output. Runs on the
/// reader thread with the terminal locked, so it only queues things.
pub struct Listener {
    input: Sender<Vec<u8>>,
    shared: Arc<Shared>,
    colours: TerminalColors,
}

impl EventListener for Listener {
    fn send_event(&self, event: Event) {
        let reply = match event {
            Event::PtyWrite(text) => text,
            Event::ColorRequest(index, format) => {
                let c = self.colours.by_index(index);
                format(alacritty_terminal::vte::ansi::Rgb { r: c.r(), g: c.g(), b: c.b() })
            }
            Event::TextAreaSizeRequest(format) => {
                let size = *lock(&self.shared.size);
                format(WindowSize {
                    num_lines: size.rows as u16,
                    num_cols: size.cols as u16,
                    cell_width: size.cell_width,
                    cell_height: size.cell_height,
                })
            }
            // Programs may set the clipboard (OSC 52). Reading it is refused
            // by the terminal config, so a remote host cannot steal it.
            Event::ClipboardStore(_, text) => {
                *lock(&self.shared.clipboard) = Some(text);
                return;
            }
            _ => return,
        };
        let _ = self.input.send(reply.into_bytes());
    }
}

/// A panicked background thread must not take the UI down with it.
fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

type SharedTerm = Arc<FairMutex<Term<Listener>>>;

pub struct Session {
    term: SharedTerm,
    input: Sender<Vec<u8>>,
    resize: Sender<GridSize>,
    shared: Arc<Shared>,
    killer: Option<Box<dyn ChildKiller + Send + Sync>>,
    size: GridSize,
}

impl Session {
    /// Start `command`. A failure to start is reported through [`Self::ended`]
    /// rather than an error, so the tab can show it and offer a retry.
    pub fn spawn(
        command: CommandBuilder,
        size: GridSize,
        scrollback: usize,
        colours: &TerminalColors,
        ctx: &egui::Context,
    ) -> Self {
        let (input, input_rx) = channel();
        let (resize, resize_rx) = channel();
        let shared = Arc::new(Shared {
            ended: Mutex::new(None),
            clipboard: Mutex::new(None),
            size: Mutex::new(size),
        });
        let listener = Listener {
            input: input.clone(),
            shared: shared.clone(),
            colours: colours.clone(),
        };
        let config = Config { scrolling_history: scrollback, ..Config::default() };
        let term = Arc::new(FairMutex::new(Term::new(config, &size, listener)));

        let threads = Threads { term: term.clone(), input_rx, resize_rx, shared: shared.clone(), ctx: ctx.clone() };
        let killer = match start(command, size, threads) {
            Ok(killer) => Some(killer),
            Err(error) => {
                let message = format!("COULD NOT START SHELL: {error}");
                *lock(&shared.ended) = Some(Ended { clean: false, message });
                None
            }
        };
        Self { term, input, resize, shared, killer, size }
    }

    pub fn send(&self, bytes: Vec<u8>) {
        let _ = self.input.send(bytes);
    }

    pub fn paste(&self, text: &str) {
        self.send(input::encode_paste(text, self.mode()));
    }

    pub fn mode(&self) -> TermMode {
        *self.term.lock().mode()
    }

    pub fn resize(&mut self, size: GridSize) {
        if size == self.size {
            return;
        }
        self.size = size;
        *lock(&self.shared.size) = size;
        self.term.lock().resize(size);
        let _ = self.resize.send(size);
    }

    pub fn frame(&self, colours: &TerminalColors, focused: bool) -> Frame {
        frame::build(&self.term.lock(), colours, focused)
    }

    pub fn scroll(&self, scroll: Scroll) {
        self.term.lock().scroll_display(scroll);
    }

    /// Run `action` on the locked terminal. Keep it short.
    pub fn with_term<R>(&self, action: impl FnOnce(&mut Term<Listener>) -> R) -> R {
        action(&mut self.term.lock())
    }

    pub fn ended(&self) -> Option<Ended> {
        lock(&self.shared.ended).clone()
    }

    /// Text a program asked to put on the clipboard, if any.
    pub fn take_clipboard(&self) -> Option<String> {
        lock(&self.shared.clipboard).take()
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        // Once the shell has gone its process id may belong to something else.
        if self.ended().is_some() {
            return;
        }
        if let Some(killer) = &mut self.killer {
            let _ = killer.kill();
        }
    }
}

struct Threads {
    term: SharedTerm,
    input_rx: Receiver<Vec<u8>>,
    resize_rx: Receiver<GridSize>,
    shared: Arc<Shared>,
    ctx: egui::Context,
}

fn start(
    command: CommandBuilder,
    size: GridSize,
    threads: Threads,
) -> Result<Box<dyn ChildKiller + Send + Sync>, String> {
    fn text(error: impl std::fmt::Display) -> String {
        error.to_string()
    }
    let pair = native_pty_system().openpty(size.into()).map_err(text)?;
    let mut child = pair.slave.spawn_command(command).map_err(text)?;
    // Keeping the slave open here would stop the reader ever seeing end of file.
    drop(pair.slave);
    let killer = child.clone_killer();
    let reader = pair.master.try_clone_reader().map_err(text)?;
    let writer = pair.master.take_writer().map_err(text)?;
    let master = pair.master;

    let Threads { term, input_rx, resize_rx, shared, ctx } = threads;

    let read_ctx = ctx.clone();
    thread::spawn(move || read_loop(reader, term, read_ctx));
    thread::spawn(move || write_loop(writer, input_rx));
    thread::spawn(move || resize_loop(master, resize_rx));
    thread::spawn(move || {
        let ended = match child.wait() {
            Ok(status) if status.success() => Ended { clean: true, message: "SHELL EXITED".to_owned() },
            Ok(status) => Ended {
                clean: false,
                message: format!("SHELL EXITED WITH CODE {}", status.exit_code()),
            },
            Err(error) => Ended { clean: false, message: format!("SHELL LOST: {error}") },
        };
        *lock(&shared.ended) = Some(ended);
        ctx.request_repaint();
    });
    Ok(killer)
}

fn read_loop(mut reader: Box<dyn Read + Send>, term: SharedTerm, ctx: egui::Context) {
    let mut parser: Processor = Processor::new();
    let mut buffer = vec![0; READ_BUFFER];
    loop {
        let count = match reader.read(&mut buffer) {
            Ok(0) => return,
            Ok(count) => count,
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            // Linux reports a closed PTY as an error rather than end of file.
            Err(_) => return,
        };
        {
            let mut term = term.lock();
            parser.advance(&mut *term, &buffer[..count]);
            // A program that opened a synchronized update and went quiet must
            // not hold the screen back forever.
            if parser.sync_timeout().sync_timeout().is_some_and(|end| Instant::now() >= end) {
                parser.stop_sync(&mut *term);
            }
        }
        ctx.request_repaint();
    }
}

fn write_loop(mut writer: Box<dyn Write + Send>, input: Receiver<Vec<u8>>) {
    while let Ok(bytes) = input.recv() {
        if writer.write_all(&bytes).and_then(|()| writer.flush()).is_err() {
            return;
        }
    }
}

/// Only the newest size matters, which keeps window dragging cheap.
fn resize_loop(master: Box<dyn MasterPty + Send>, sizes: Receiver<GridSize>) {
    while let Ok(mut size) = sizes.recv() {
        while let Ok(newer) = sizes.try_recv() {
            size = newer;
        }
        let _ = master.resize(size.into());
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use crate::theme::Theme;
    use std::time::Duration;

    fn size(cols: usize, rows: usize) -> GridSize {
        GridSize { cols, rows, cell_width: 10, cell_height: 20 }
    }

    fn sh(script: &str) -> Session {
        let mut command = CommandBuilder::new("/bin/sh");
        command.args(["-c", script]);
        command.env("TERM", "xterm-256color");
        let colours = Theme::tron().terminal;
        Session::spawn(command, size(40, 10), 100, &colours, &egui::Context::default())
    }

    fn screen(session: &Session) -> String {
        let colours = Theme::tron().terminal;
        let frame = session.frame(&colours, true);
        frame.runs.iter().map(|r| r.text.as_str()).collect::<Vec<_>>().join("\n")
    }

    fn wait_for(what: &str, mut done: impl FnMut() -> bool) {
        let start = Instant::now();
        while !done() {
            assert!(start.elapsed() < Duration::from_secs(5), "timed out waiting for {what}");
            thread::sleep(Duration::from_millis(10));
        }
    }

    #[test]
    fn output_from_a_real_shell_reaches_the_grid() {
        let session = sh("printf 'hello from pty'; sleep 5");
        wait_for("output", || screen(&session).contains("hello from pty"));
    }

    #[test]
    fn input_reaches_the_program() {
        let session = sh("read line; echo \"got:$line\"; sleep 5");
        session.send(b"typed\r".to_vec());
        wait_for("echo", || screen(&session).contains("got:typed"));
    }

    #[test]
    fn exit_codes_are_reported() {
        let clean = sh("exit 0");
        wait_for("exit", || clean.ended().is_some());
        assert!(clean.ended().unwrap().clean);

        let failed = sh("exit 3");
        wait_for("exit", || failed.ended().is_some());
        let ended = failed.ended().unwrap();
        assert!(!ended.clean);
        assert!(ended.message.contains('3'));
    }

    #[test]
    fn a_missing_program_is_reported_not_fatal() {
        let colours = Theme::tron().terminal;
        let command = CommandBuilder::new("/nonexistent/shell");
        let session = Session::spawn(command, size(40, 10), 100, &colours, &egui::Context::default());
        let ended = session.ended().expect("spawn failure is reported straight away");
        assert!(ended.message.contains("COULD NOT START"), "{}", ended.message);
        session.send(b"ignored".to_vec());
    }

    #[test]
    fn resize_reaches_the_program() {
        let mut session = sh("read line; stty size; sleep 5");
        session.resize(size(77, 23));
        thread::sleep(Duration::from_millis(100));
        session.send(b"\r".to_vec());
        wait_for("stty", || screen(&session).contains("23 77"));
    }

    #[test]
    fn terminal_queries_get_answers() {
        // Asks for the cursor position, then prints what came back.
        let script = "stty -echo raw; printf '\\033[6n'; dd bs=1 count=6 2>/dev/null | od -c | head -1; sleep 5";
        let session = sh(script);
        wait_for("reply", || screen(&session).contains("033   ["));
    }

    #[test]
    fn a_program_that_ignores_input_cannot_block_the_caller() {
        let session = sh("sleep 5");
        let start = Instant::now();
        for _ in 0..200 {
            session.send(vec![b'x'; 64 * 1024]);
        }
        assert!(start.elapsed() < Duration::from_secs(1), "send blocked for {:?}", start.elapsed());
    }

    #[test]
    fn dropping_a_session_stops_its_shell() {
        let session = sh("echo $$; sleep 30");
        wait_for("pid", || !screen(&session).is_empty());
        let pid: i32 = screen(&session).trim().parse().unwrap();
        drop(session);
        wait_for("shell to stop", || !std::path::Path::new(&format!("/proc/{pid}")).exists());
    }
}
