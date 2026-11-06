//! The startup sequence: a Linux-style boot log, then the title. Real stats
//! from this machine sit among random nonsense, so every start reads differently.
//! Any key or click skips it, and the config can turn it off.

use egui::text::{LayoutJob, TextFormat, TextWrapping};
use egui::{pos2, Align2, Color32, Context, Event, FontFamily, FontId, Painter, Rect};
use sysinfo::System;

use super::boot_lines::{FAILURES, JOBS, KERNEL, RARE, SERVICES, WARNINGS};
use super::panel::{rule, Style, DISPLAY_FONT};
use crate::stats::{format, Snapshot};
use crate::theme::{STATUS_FAILED, STATUS_OK, STATUS_RARE, STATUS_WARN};

const TITLE_SECONDS: f64 = 1.2;
const TITLE: &str = "EDEX-RS";
/// How long the finished log stays up before the title.
const FINAL_HOLD: f64 = 0.9;
/// Each start draws this many lines from each pool.
const KERNEL_JOKES: usize = 6;
const SERVICE_JOKES: usize = 8;
const WARNING_JOKES: usize = 2;
/// One start in this many gets a rare line.
const RARE_ODDS: u64 = 10;
/// The spinning job's clock runs fast, because it is lying.
const JOB_CLOCK_SPEED: f64 = 37.0;
const SPINNER_WIDTH: usize = 6;

#[derive(Clone, Copy, Debug, PartialEq)]
enum Tag {
    /// A kernel message, led by its timestamp.
    Kernel,
    Ok,
    Warn,
    Failed,
    Rare,
    /// No prefix at all.
    Plain,
}

/// A line's words, or how to read them from the stats once it is due, since
/// the first sample lands a moment after the log starts.
enum Text {
    Fixed(String),
    Live(fn(&Snapshot) -> Option<String>),
}

struct Line {
    tag: Tag,
    text: Text,
    /// Shown with a spinner until the next line arrives.
    running: Option<String>,
    /// Seconds before the next line.
    dwell: f64,
}

/// A line that has reached the screen.
struct Shown {
    tag: Tag,
    text: String,
    running: Option<String>,
    /// When it arrived, in seconds from the start of the log.
    at: f64,
}

/// What the log says about this machine.
struct Facts {
    user: String,
    host: String,
    system: Option<String>,
    cores: usize,
}

impl Facts {
    fn read() -> Self {
        let user = std::env::var("USER").or_else(|_| std::env::var("USERNAME"));
        let name = System::long_os_version().ok();
        let kernel = System::kernel_version().ok();
        Self {
            user: user.unwrap_or_else(|_| "operator".to_owned()),
            host: System::host_name().unwrap_or_else(|_| "localhost".to_owned()),
            system: name.map(|name| kernel.map_or_else(|| name.clone(), |kernel| format!("{name}, kernel {kernel}"))),
            cores: std::thread::available_parallelism().map_or(1, usize::from),
        }
    }
}

/// A small xorshift generator: this only shuffles jokes, so no crate is needed.
struct Dice(u64);

impl Dice {
    fn seeded() -> Self {
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH);
        Self::new(now.map_or(1, |time| time.as_nanos() as u64))
    }

    fn new(seed: u64) -> Self {
        Self(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }

    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn below(&mut self, limit: u64) -> u64 {
        self.next() % limit
    }

    fn between(&mut self, low: f64, high: f64) -> f64 {
        low + (self.next() >> 11) as f64 / (1u64 << 53) as f64 * (high - low)
    }

    /// `count` different items from `pool`.
    fn draw<T: Copy>(&mut self, pool: &[T], count: usize) -> Vec<T> {
        let mut items = pool.to_vec();
        for i in (1..items.len()).rev() {
            items.swap(i, self.below(i as u64 + 1) as usize);
        }
        items.truncate(count);
        items
    }
}

/// Swap the placeholders for numbers and names. Each `{n}` rolls its own number.
fn fill(template: &str, facts: &Facts, dice: &mut Dice) -> String {
    let mut text = template
        .replace("{cores}", &facts.cores.to_string())
        .replace("{user}", &facts.user)
        .replace("{host}", &facts.host);
    while text.contains("{n}") {
        text = text.replacen("{n}", &(dice.below(998) + 2).to_string(), 1);
    }
    while text.contains("{p}") {
        text = text.replacen("{p}", &dice.below(101).to_string(), 1);
    }
    text
}

fn hardware_line(snapshot: &Snapshot) -> Option<String> {
    snapshot.sampled_at?;
    let hardware = &snapshot.hardware;
    let parts: Vec<&str> = [&hardware.manufacturer, &hardware.model].into_iter().flatten().map(String::as_str).collect();
    let name = if parts.is_empty() { "Unbranded Mystery Box".to_owned() } else { parts.join(" ") };
    Some(format!("DMI: {name}"))
}

fn cpu_line(snapshot: &Snapshot) -> Option<String> {
    let cpu = &snapshot.cpu;
    let model = cpu.model.as_ref()?;
    let clock = cpu.max_frequency_mhz.or(cpu.frequency_mhz);
    Some(clock.map_or_else(|| format!("CPU: {model}"), |mhz| format!("CPU: {model} @ {} GHz", format::ghz(Some(mhz)))))
}

fn memory_line(snapshot: &Snapshot) -> Option<String> {
    let memory = snapshot.memory.as_ref()?;
    Some(format!("Memory: {} in use, the rest is emotional support", format::gib_pair(memory.used, memory.total)))
}

fn thermal_line(snapshot: &Snapshot) -> Option<String> {
    snapshot.sampled_at?;
    Some(snapshot.cpu.temperature.map_or_else(
        || "Started Thermal Monitor (the sensor is shy).".to_owned(),
        |celsius| format!("Started Thermal Monitor (CPU at {}, allegedly).", format::temperature(Some(celsius))),
    ))
}

fn census_line(snapshot: &Snapshot) -> Option<String> {
    let tasks = snapshot.cpu.tasks?;
    Some(format!("Finished Process Census ({tasks} tasks, {} acting weird).", tasks % 7 + 1))
}

fn uptime_line(snapshot: &Snapshot) -> Option<String> {
    let uptime = snapshot.uptime_seconds?;
    Some(format!("Host has been up for {}. Nobody said thank you.", format::uptime(Some(uptime))))
}

/// One start's log, kernel messages first, then the services with the jokes
/// dealt between them.
fn script(facts: &Facts, dice: &mut Dice) -> Vec<Line> {
    let mut lines = Vec::new();
    let kernel = |text: Text, dice: &mut Dice| Line { tag: Tag::Kernel, text, running: None, dwell: dice.between(0.04, 0.11) };
    let fixed = |template: &str, dice: &mut Dice| Text::Fixed(fill(template, facts, dice));

    let banner = format!("edex-rs {} ({{user}}@{{host}}) #1 SMP PREEMPT_DYNAMIC", env!("CARGO_PKG_VERSION"));
    let opening = [banner.as_str(), "Command line: BOOT_IMAGE=/vmlinuz-vibes root=/dev/hopes ro quiet splash=aggressive"];
    for template in opening {
        let text = fixed(template, dice);
        lines.push(kernel(text, dice));
    }
    if let Some(system) = &facts.system {
        lines.push(kernel(Text::Fixed(format!("OS: {system}")), dice));
    }
    let jokes = dice.draw(&KERNEL, KERNEL_JOKES);
    let (early, late) = jokes.split_at(KERNEL_JOKES / 2);
    for joke in early {
        let text = fixed(joke, dice);
        lines.push(kernel(text, dice));
    }
    // By now the first stats sample has landed.
    for read in [hardware_line, cpu_line, memory_line] {
        lines.push(kernel(Text::Live(read), dice));
    }
    for joke in late {
        let text = fixed(joke, dice);
        lines.push(kernel(text, dice));
    }
    lines.push(Line { tag: Tag::Plain, text: Text::Fixed("Welcome to EDEX-RS!".to_owned()), running: None, dwell: 0.3 });

    let mut real: Vec<Text> = vec![
        fixed("Started Terminal Session Pool.", dice),
        fixed("Started Stats Sampler ({cores} threads).", dice),
        Text::Live(thermal_line),
        Text::Live(census_line),
        fixed("Reached target Network.", dice),
        Text::Live(uptime_line),
        fixed("Started Globe Projection.", dice),
        fixed("Mounted /home/{user}.", dice),
    ];
    let services = dice.draw(&SERVICES, SERVICE_JOKES);
    let warnings = dice.draw(&WARNINGS, WARNING_JOKES);
    let failure = dice.draw(&FAILURES, 1)[0];
    let job = dice.draw(&JOBS, 1)[0];
    for (i, (text, joke)) in real.drain(..).zip(services).enumerate() {
        lines.push(Line { tag: Tag::Ok, text, running: None, dwell: dice.between(0.06, 0.16) });
        let text = fixed(joke, dice);
        lines.push(Line { tag: Tag::Ok, text, running: None, dwell: dice.between(0.22, 0.36) });
        let special = match i {
            1 | 6 => Some((Tag::Warn, warnings[i / 6])),
            3 => Some((Tag::Failed, failure)),
            _ => None,
        };
        if let Some((tag, template)) = special {
            let text = fixed(template, dice);
            lines.push(Line { tag, text, running: None, dwell: dice.between(0.35, 0.45) });
        }
        if i == 4 {
            let (running, finished) = (fill(job.0, facts, dice), fixed(job.1, dice));
            lines.push(Line { tag: Tag::Ok, text: finished, running: Some(running), dwell: 1.1 });
        }
    }
    if dice.below(RARE_ODDS) == 0 {
        let text = fixed(dice.draw(&RARE, 1)[0], dice);
        lines.push(Line { tag: Tag::Rare, text, running: None, dwell: 0.7 });
    }
    let text = fixed("Reached target Graphical Interface.", dice);
    lines.push(Line { tag: Tag::Ok, text, running: None, dwell: 0.2 });
    let login = fixed("{host} login: {user} (automatic login)", dice);
    lines.push(Line { tag: Tag::Plain, text: login, running: None, dwell: FINAL_HOLD });
    lines
}

fn log_seconds(lines: &[Line]) -> f64 {
    lines.iter().map(|line| line.dwell).sum()
}

pub struct BootScreen {
    started: Option<f64>,
    script: Vec<Line>,
    log_seconds: f64,
    /// The next line of the script to show.
    next: usize,
    /// When that line is due.
    next_at: f64,
    shown: Vec<Shown>,
}

impl Default for BootScreen {
    fn default() -> Self {
        let script = script(&Facts::read(), &mut Dice::seeded());
        let log_seconds = log_seconds(&script);
        Self { started: None, script, log_seconds, next: 0, next_at: 0.0, shown: Vec::new() }
    }
}

impl BootScreen {
    /// Draw a frame. Returns true when the sequence is over.
    pub fn show(&mut self, ctx: &Context, painter: &Painter, window: Rect, snapshot: &Snapshot, style: &Style<'_>) -> bool {
        let (time, skipped) = ctx.input(|i| (i.time, i.events.iter().any(is_skip)));
        let elapsed = time - *self.started.get_or_insert(time);
        if skipped || elapsed >= self.log_seconds + TITLE_SECONDS {
            return true;
        }
        if elapsed < self.log_seconds {
            self.reveal(elapsed, snapshot);
            draw_log(painter, window, &self.shown, elapsed, style);
        } else {
            draw_title(painter, window, (elapsed - self.log_seconds) / TITLE_SECONDS, style);
        }
        ctx.request_repaint();
        false
    }

    /// Bring every line that is due onto the screen. A stat with nothing to
    /// report is left out, rather than printing a blank.
    fn reveal(&mut self, elapsed: f64, snapshot: &Snapshot) {
        while self.next < self.script.len() && self.next_at <= elapsed {
            let line = &mut self.script[self.next];
            let text = match &mut line.text {
                Text::Fixed(text) => Some(std::mem::take(text)),
                Text::Live(read) => read(snapshot),
            };
            if let Some(text) = text {
                self.shown.push(Shown { tag: line.tag, text, running: line.running.take(), at: self.next_at });
            }
            self.next_at += line.dwell;
            self.next += 1;
        }
    }
}

fn is_skip(event: &Event) -> bool {
    matches!(event, Event::Key { pressed: true, .. } | Event::PointerButton { pressed: true, .. })
}

/// The stars bouncing inside `[      ]`, as in a real start job.
fn spinner(seconds: f64) -> String {
    const SPAN: usize = SPINNER_WIDTH + 2;
    let step = (seconds * 10.0) as usize % (SPAN * 2 - 2);
    let head = if step < SPAN { step } else { SPAN * 2 - 2 - step };
    (0..SPINNER_WIDTH).map(|i| if i + 3 > head && i < head + 1 { '*' } else { ' ' }).collect()
}

/// Newest line at the bottom, pushing the others up.
fn draw_log(painter: &Painter, window: Rect, lines: &[Shown], elapsed: f64, style: &Style<'_>) {
    let size = style.fonts.terminal;
    let height = size * 1.4;
    let area = window.shrink(24.0);
    let bottom = area.max.y - style.fonts.label * 2.0;
    let fits = (((bottom - area.min.y) / height) as usize).max(1);
    let first = lines.len().saturating_sub(fits);
    for (back, (index, line)) in lines.iter().enumerate().skip(first).rev().enumerate() {
        let newest = index + 1 == lines.len();
        let job = log_line(line, newest, elapsed, area.width(), style);
        let galley = painter.layout_job(job);
        let at = pos2(area.min.x, bottom - back as f32 * height - galley.size().y);
        painter.galley(at, galley, style.theme.terminal.foreground);
    }
    let hint = area.right_bottom();
    painter.text(hint, Align2::RIGHT_BOTTOM, "PRESS ANY KEY TO SKIP", style.caption(), style.theme.muted);
}

/// One line of the log with its coloured prefix.
fn log_line(line: &Shown, newest: bool, elapsed: f64, width: f32, style: &Style<'_>) -> LayoutJob {
    let font = FontId::monospace(style.fonts.terminal);
    let text_colour = style.theme.terminal.foreground;
    let format = |color: Color32| TextFormat { font_id: font.clone(), color, ..TextFormat::default() };
    let mut job = LayoutJob::default();
    job.wrap = TextWrapping::truncate_at_width(width);
    let mut bracket = |label: &str, colour: Color32| {
        job.append("[", 0.0, format(text_colour));
        job.append(label, 0.0, format(colour));
        job.append("] ", 0.0, format(text_colour));
    };
    let running = line.running.as_ref().filter(|_| newest);
    if let Some(text) = running {
        let seconds = elapsed - line.at;
        bracket(&spinner(seconds), STATUS_FAILED);
        job.append(&format!("{text} ({:.0}s / no limit)", seconds * JOB_CLOCK_SPEED), 0.0, format(text_colour));
        return job;
    }
    match line.tag {
        Tag::Kernel => job.append(&format!("[{:>12.6}] ", line.at), 0.0, format(style.theme.muted)),
        Tag::Ok => bracket("  OK  ", STATUS_OK),
        Tag::Warn => bracket(" WARN ", STATUS_WARN),
        Tag::Failed => bracket("FAILED", STATUS_FAILED),
        Tag::Rare => bracket(" RARE ", STATUS_RARE),
        Tag::Plain => {}
    }
    job.append(&line.text, 0.0, format(text_colour));
    job
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

    fn facts() -> Facts {
        Facts { user: "ada".to_owned(), host: "engine".to_owned(), system: Some("Debian 13".to_owned()), cores: 8 }
    }

    fn fixed_text(line: &Line) -> Option<&str> {
        match &line.text {
            Text::Fixed(text) => Some(text),
            Text::Live(_) => None,
        }
    }

    #[test]
    fn placeholders_never_reach_the_screen() {
        for seed in 0..300 {
            for line in script(&facts(), &mut Dice::new(seed)) {
                let text = fixed_text(&line).unwrap_or_default();
                assert!(!text.contains(['{', '}']), "{text}");
                assert!(!line.running.unwrap_or_default().contains(['{', '}']));
            }
        }
    }

    #[test]
    fn every_placeholder_rolls_its_own_number() {
        let mut dice = Dice::new(5);
        let rolled: Vec<String> = (0..20).map(|_| fill("{n}/{n}", &facts(), &mut dice)).collect();
        assert!(rolled.iter().any(|text| text.split('/').next() != text.split('/').nth(1)));
    }

    #[test]
    fn starts_read_differently() {
        let words = |seed| -> Vec<String> {
            script(&facts(), &mut Dice::new(seed)).iter().filter_map(fixed_text).map(str::to_owned).collect()
        };
        assert_ne!(words(1), words(2));
    }

    #[test]
    fn the_log_runs_long_enough_to_read_but_not_too_long() {
        for seed in 0..300 {
            let seconds = log_seconds(&script(&facts(), &mut Dice::new(seed)));
            assert!((5.0..10.0).contains(&seconds), "{seconds}");
        }
    }

    #[test]
    fn the_log_has_every_status_and_ends_at_the_login() {
        let lines = script(&facts(), &mut Dice::new(9));
        for tag in [Tag::Kernel, Tag::Ok, Tag::Warn, Tag::Failed] {
            assert!(lines.iter().any(|line| line.tag == tag), "{tag:?}");
        }
        assert_eq!(lines.iter().filter(|line| line.running.is_some()).count(), 1);
        let last = lines.last().unwrap();
        assert_eq!(fixed_text(last), Some("engine login: ada (automatic login)"));
    }

    #[test]
    fn rare_lines_are_rare() {
        let starts = 2000;
        let rare = (0..starts)
            .filter(|seed| script(&facts(), &mut Dice::new(*seed)).iter().any(|line| line.tag == Tag::Rare))
            .count();
        assert!((100..320).contains(&rare), "{rare}");
    }

    #[test]
    fn jokes_are_not_repeated_within_a_pool() {
        fn check(pool: &[&str]) {
            let mut seen = pool.to_vec();
            seen.sort_unstable();
            seen.dedup();
            assert_eq!(seen.len(), pool.len());
        }
        for pool in [&KERNEL[..], &SERVICES[..], &WARNINGS[..], &FAILURES[..], &RARE[..]] {
            check(pool);
        }
    }

    #[test]
    fn stats_appear_when_due_and_missing_ones_are_left_out() {
        let lines = vec![
            Line { tag: Tag::Kernel, text: Text::Live(memory_line), running: None, dwell: 1.0 },
            Line { tag: Tag::Ok, text: Text::Live(census_line), running: None, dwell: 1.0 },
        ];
        let mut boot = BootScreen { started: None, log_seconds: log_seconds(&lines), script: lines, next: 0, next_at: 0.0, shown: Vec::new() };
        let mut snapshot = Snapshot::default();
        boot.reveal(0.5, &snapshot);
        assert!(boot.shown.is_empty());
        snapshot.cpu.tasks = Some(10);
        boot.reveal(1.5, &snapshot);
        assert_eq!(boot.shown.len(), 1);
        assert_eq!(boot.shown[0].text, "Finished Process Census (10 tasks, 4 acting weird).");
        assert_eq!(boot.shown[0].at, 1.0);
    }

    #[test]
    fn an_early_frame_makes_no_claims_about_the_hardware() {
        let empty = Snapshot::default();
        assert_eq!(hardware_line(&empty), None);
        assert_eq!(thermal_line(&empty), None);
        let sampled = Snapshot { sampled_at: Some(std::time::Instant::now()), ..Snapshot::default() };
        assert_eq!(hardware_line(&sampled).as_deref(), Some("DMI: Unbranded Mystery Box"));
    }

    #[test]
    fn the_spinner_keeps_its_width_and_moves() {
        let frames: Vec<String> = (0..14).map(|i| spinner(f64::from(i) / 10.0)).collect();
        assert!(frames.iter().all(|frame| frame.chars().count() == SPINNER_WIDTH));
        assert!(frames.iter().any(|frame| frame.contains("***")));
        assert_ne!(frames[1], frames[4]);
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
