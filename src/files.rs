//! Reads directories on a background thread, because a slow disk or a dead
//! network share must not freeze the window.

use std::cmp::Ordering;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError, Sender};
use std::thread;
use std::time::Duration;

/// Listings refresh this often, to pick up files created by the shell.
const REFRESH: Duration = Duration::from_secs(2);
/// Past this, the rest of a directory is skipped and the panel says so.
const MAX_ENTRIES: usize = 2_000;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub name: String,
    pub is_dir: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Listing {
    pub path: PathBuf,
    /// Entries, or a short reason they can't be shown.
    pub entries: Result<Vec<Entry>, String>,
    /// Entries left out for being past the limit.
    pub skipped: usize,
}

pub struct FileWorker {
    requests: Sender<PathBuf>,
    listings: Receiver<Listing>,
}

impl FileWorker {
    pub fn spawn(ctx: &egui::Context) -> Self {
        let (requests, request_rx) = channel();
        let (listing_tx, listings) = channel();
        let ctx = ctx.clone();
        let _ = thread::Builder::new()
            .name("files".to_owned())
            .spawn(move || run(&request_rx, &listing_tx, &ctx));
        Self { requests, listings }
    }

    pub fn request(&self, path: PathBuf) {
        let _ = self.requests.send(path);
    }

    /// The newest finished listing, without waiting.
    pub fn latest(&self) -> Option<Listing> {
        self.listings.try_iter().last()
    }
}

fn run(requests: &Receiver<PathBuf>, listings: &Sender<Listing>, ctx: &egui::Context) {
    let mut current: Option<PathBuf> = None;
    let mut last_sent: Option<Listing> = None;
    loop {
        match requests.recv_timeout(REFRESH) {
            Ok(path) => {
                // Only the newest request matters.
                current = Some(requests.try_iter().last().unwrap_or(path));
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return,
        }
        let Some(path) = &current else { continue };
        let listing = read_listing(path);
        if last_sent.as_ref() != Some(&listing) {
            if listings.send(listing.clone()).is_err() {
                return;
            }
            last_sent = Some(listing);
            ctx.request_repaint();
        }
    }
}

fn read_listing(path: &Path) -> Listing {
    let mut skipped = 0;
    let entries = fs::read_dir(path).map_err(|error| describe(&error)).map(|reader| {
        let mut entries = Vec::new();
        for item in reader.flatten() {
            if entries.len() == MAX_ENTRIES {
                skipped += 1;
                continue;
            }
            // Follows links, so a link to a folder opens like a folder.
            let is_dir = fs::metadata(item.path()).is_ok_and(|meta| meta.is_dir());
            entries.push(Entry { name: item.file_name().to_string_lossy().into_owned(), is_dir });
        }
        entries.sort_by(compare);
        entries
    });
    Listing { path: path.to_owned(), entries, skipped }
}

fn describe(error: &std::io::Error) -> String {
    match error.kind() {
        ErrorKind::PermissionDenied => "ACCESS DENIED".to_owned(),
        ErrorKind::NotFound => "FOLDER NOT FOUND".to_owned(),
        _ => "CANNOT READ FOLDER".to_owned(),
    }
}

/// Folders first, then names without regard to case.
fn compare(a: &Entry, b: &Entry) -> Ordering {
    b.is_dir.cmp(&a.is_dir).then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    struct Dir(PathBuf);

    impl Dir {
        fn new(name: &str) -> Self {
            let path = std::env::temp_dir().join(format!("edex-files-{name}-{}", std::process::id()));
            let _ = fs::remove_dir_all(&path);
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }
    }

    impl Drop for Dir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn folders_come_first_and_names_ignore_case() {
        let dir = Dir::new("sort");
        fs::write(dir.0.join("b.txt"), "").unwrap();
        fs::write(dir.0.join("A.txt"), "").unwrap();
        fs::create_dir(dir.0.join("zebra")).unwrap();
        let listing = read_listing(&dir.0);
        let names: Vec<String> = listing.entries.unwrap().into_iter().map(|e| e.name).collect();
        assert_eq!(names, ["zebra", "A.txt", "b.txt"]);
    }

    #[test]
    fn a_missing_folder_is_reported_not_fatal() {
        let listing = read_listing(Path::new("/nonexistent/folder"));
        assert_eq!(listing.entries, Err("FOLDER NOT FOUND".to_owned()));
    }

    #[test]
    fn huge_folders_are_capped() {
        let dir = Dir::new("huge");
        for i in 0..MAX_ENTRIES + 3 {
            fs::write(dir.0.join(i.to_string()), "").unwrap();
        }
        let listing = read_listing(&dir.0);
        assert_eq!(listing.entries.unwrap().len(), MAX_ENTRIES);
        assert_eq!(listing.skipped, 3);
    }

    #[cfg(unix)]
    #[test]
    fn a_link_to_a_folder_counts_as_a_folder_and_a_broken_one_as_a_file() {
        let dir = Dir::new("links");
        fs::create_dir(dir.0.join("real")).unwrap();
        std::os::unix::fs::symlink(dir.0.join("real"), dir.0.join("good")).unwrap();
        std::os::unix::fs::symlink(dir.0.join("gone"), dir.0.join("bad")).unwrap();
        let entries = read_listing(&dir.0).entries.unwrap();
        assert!(entries.iter().find(|e| e.name == "good").unwrap().is_dir);
        assert!(!entries.iter().find(|e| e.name == "bad").unwrap().is_dir);
    }

    #[test]
    fn the_worker_delivers_and_picks_up_changes() {
        let dir = Dir::new("worker");
        let worker = FileWorker::spawn(&egui::Context::default());
        worker.request(dir.0.clone());
        let wait = |want: usize| {
            let start = Instant::now();
            loop {
                if let Some(listing) = worker.latest() {
                    if listing.entries.as_ref().is_ok_and(|e| e.len() == want) {
                        return;
                    }
                }
                assert!(start.elapsed() < REFRESH * 3, "no listing with {want} entries");
                thread::sleep(Duration::from_millis(20));
            }
        };
        wait(0);
        fs::write(dir.0.join("new.txt"), "").unwrap();
        wait(1);
    }
}
