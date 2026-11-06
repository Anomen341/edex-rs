//! The jokes in the startup log. Each start draws a random handful, so adding
//! to these pools is all it takes to add more.
//!
//! Placeholders: `{n}` a number, `{p}` a percentage, `{cores}` the thread count,
//! `{user}` and `{host}` the real names.

/// Timestamped kernel messages. These are the stats that make no sense.
pub const KERNEL: [&str; 32] = [
    "Fan speed: yes",
    "Bogomips: {n}.00, bogomoods: 2",
    "Entropy pool: 3 ducks, 1 of them wet",
    "Clock drift: +2 Tuesdays",
    "Stack depth: 3 pancakes",
    "Gravity: enabled (legacy mode)",
    "Threads: {cores} logical, 0 emotional",
    "RAM check: {p}% remembered, the rest remembered fondly",
    "Disk usage: 140% of a small horse",
    "Swap: traded 2 GiB for a sandwich",
    "Cache hits: {n}, near misses: 4, police notified",
    "Packets lost: {n}, found: {n}, not ready to talk about it: 1",
    "Pixels counted: all of them, twice",
    "Latency to the moon: 1.3 s (the moon declined to comment)",
    "CPU flags: fpu vme sse2 avx2 snacks",
    "Keyboard: 104 keys, 1 with a grudge",
    "Mouse cursor morale: {p}%",
    "Semicolons: {n},112 (Rust kept all of them)",
    "Battery: {p}% vibes, the rest lies",
    "Voltage: spicy",
    "PCI: found {n} devices, believed 3 of them",
    "Rounding pi to 3 for performance",
    "Screen brightness: emotionally unavailable",
    "USB: inserted correctly on attempt 3 of 3",
    "Thermal paste: {p}% paste, {p}% thermal",
    "Calibrating the flux of nothing in particular: {p}%",
    "Bits: {n} ones, {n} zeros, 1 undecided",
    "Timezone: vaguely west of here",
    "Dust bunnies in chassis: {n}, unionised",
    "Sectors read: {n}, sectors that read back: 2",
    "Coffee level: below safe operating threshold",
    "Spare electrons returned to the wall",
];

/// `[  OK  ]` lines that read like services.
pub const SERVICES: [&str; 34] = [
    "Started Regret Daemon.",
    "Started Sock Reunification Service.",
    "Started Cat-on-Keyboard Detection.",
    "Started Hourly Apology to the GPU.",
    "Started Daily Cleanup of Yesterday's Mistakes.",
    "Started Passive Aggressive Notification Service.",
    "Started Rubber Duck Debugging Bridge.",
    "Started Background Sighing.",
    "Started Tab Hoarding Service ({n} tabs, all important).",
    "Started Password Forgetting Scheduler.",
    "Started Fan Noise Harmonisation.",
    "Started Lost Semicolon Search Party.",
    "Started Remote Control Finder (couch.service).",
    "Started Plausible Deniability Service.",
    "Created slice Pizza.slice.",
    "Created slice Leftover-Pizza.slice.",
    "Mounted /mnt/everest.",
    "Mounted /dev/null (it held up fine).",
    "Mounted /home/{user}/definitely_not_games.",
    "Listening on /run/gossip.socket.",
    "Listening on the Neighbour's Wi-Fi.",
    "Listening to Reason (timeout, skipped).",
    "Reached target Slightly Past Target.",
    "Reached target Mid-Life Crisis.",
    "Reached target Good Enough.",
    "Finished Counting to Infinity (twice).",
    "Finished Pretending to Read the Terms.",
    "Finished Explaining It to {user}'s Parents.",
    "Finished Defragmenting the Vibes.",
    "Finished Untangling {n} Cables.",
    "Stopped Caring About Trailing Whitespace.",
    "Activated Swap of Coffee for Decaf.",
    "Set hostname to {host}, and it seems happy about it.",
    "Updated {user}'s Horoscope (Mercury: read-only).",
];

/// `[ WARN ]` lines.
pub const WARNINGS: [&str; 14] = [
    "Time since last backup: yes.",
    "{n} ducks not in a row.",
    "Printer detected nearby. Do not make eye contact.",
    "Your shell has been talking about you.",
    "Low on disk, high on hopes.",
    "Unexpected Tuesday at line {n}.",
    "CPU asked {user} to stop looking at it.",
    "{p}% of this log is not true. Unable to determine which.",
    "Something is in the walls of /var.",
    "Bluetooth is pairing with something. Nobody knows what.",
    "This machine is {p}% sure it left the oven on.",
    "Too many open files. Too many open feelings.",
    "Clock says it is fine. Clock looks tired.",
    "The fans are whispering about {host}.",
];

/// `[FAILED]` lines.
pub const FAILURES: [&str; 10] = [
    "Failed to start Feelings Manager. See 'journalctl -u feelings'. Actually, don't.",
    "Failed to find the any key.",
    "Failed to start Monday. Will retry, sadly.",
    "Failed to unmount Emotional Baggage: target is busy.",
    "Failed to load module 'motivation': no such device.",
    "Failed to remember why {user} opened this.",
    "Failed to close {n} browser tabs: permission denied by {user}.",
    "Failed to reach Inner Peace: destination host unreachable.",
    "Failed to start Weekend (dependency on Friday not met).",
    "Failed to parse the vibe at line {n}.",
];

/// A `[ *** ]` line, shown with a moving spinner, then what it turns into.
pub const JOBS: [(&str, &str); 8] = [
    ("A start job is running for Existential Dread", "Finished Existential Dread (no action required)."),
    ("A stop job is running for That Thing You Said in 2014", "Stopped That Thing You Said in 2014 (it will be back)."),
    ("A start job is running for Finding the Other Monitor Cable", "Found the Other Monitor Cable (it was the same one)."),
    ("A start job is running for Someone Else's Problem", "Finished Someone Else's Problem (now yours)."),
    ("A start job is running for Warming Up the Pixels", "Warmed Up the Pixels (medium rare)."),
    ("A stop job is running for Overthinking", "Stopped Overthinking (will restart at 3 a.m.)."),
    ("A start job is running for Waiting on {host} to Finish Its Sentence", "Finished Waiting (it never did)."),
    ("A start job is running for Asking the Disk Nicely", "Finished Asking the Disk Nicely (it said maybe)."),
];

/// `[ RARE ]` lines, on about one start in ten. These are why you open it again.
pub const RARE: [&str; 8] = [
    "You found a rare line. Nobody is going to believe you.",
    "This line appears on one boot in ten. Tell no one.",
    "The goose is loose. Repeat: the goose is loose.",
    "{user}, the panels have been asking about you.",
    "Boot sequence achieved sentience, chose to keep working here anyway.",
    "A tiny door has opened in /proc. Do not go in. Okay, go in a little.",
    "Congratulations, {user}. Your prize is this line.",
    "Somewhere, a CPU fan just spun up for no reason. That was us.",
];
