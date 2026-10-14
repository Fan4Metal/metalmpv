//! Bookmarks: a file and a place in it, kept between runs (the setting
//! `bookmarks`, not reset with the settings). Added from the picture's
//! context menu or `Ctrl+B`, the latest first; a click on one goes there,
//! opening its file when another plays.

use std::path::{Path, PathBuf};

/// The most bookmarks kept: adding one more drops the oldest.
pub const MAX: usize = 30;
/// A bookmark this close to another of the same file takes its place.
const SAME_PLACE: f64 = 1.0;

#[derive(Clone, Debug, PartialEq)]
pub struct Bookmark {
    pub path: PathBuf,
    /// Seconds from the start.
    pub time: f64,
}

impl Bookmark {
    /// What the menu shows: the file's name (an address's last part) and
    /// the time.
    pub fn label(&self) -> String {
        let name = self
            .path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| self.path.to_string_lossy().into_owned());
        format!("{name} — {}", crate::format::time(Some(self.time), false))
    }

    pub fn is_of(&self, path: &Path) -> bool {
        same_path(&self.path, path)
    }
}

/// Paths compared as Windows does, without regard to case.
pub fn same_path(a: &Path, b: &Path) -> bool {
    a.as_os_str().eq_ignore_ascii_case(b.as_os_str())
}

/// Add a bookmark at `time` of `path` first in `list`, in place of one of
/// that file within [`SAME_PLACE`]; at most [`MAX`].
pub fn add(list: &mut Vec<Bookmark>, path: &Path, time: f64) {
    list.retain(|b| !(b.is_of(path) && (b.time - time).abs() < SAME_PLACE));
    list.insert(
        0,
        Bookmark {
            path: path.to_path_buf(),
            time: time.max(0.0),
        },
    );
    list.truncate(MAX);
}

/// The setting: a line each, `seconds<TAB>path`.
pub fn to_text(list: &[Bookmark]) -> String {
    list.iter()
        .map(|b| format!("{:.3}\t{}", b.time, b.path.display()))
        .collect::<Vec<_>>()
        .join("\n")
}

/// The bookmarks of the setting; lines not understood are left out.
pub fn from_text(text: &str) -> Vec<Bookmark> {
    text.lines()
        .filter_map(|line| {
            let (time, path) = line.split_once('\t')?;
            let time = time.parse::<f64>().ok().filter(|t| t.is_finite())?;
            (!path.is_empty()).then(|| Bookmark {
                path: PathBuf::from(path),
                time: time.max(0.0),
            })
        })
        .take(MAX)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn added_first_and_kept() {
        let mut list = Vec::new();
        let film = Path::new(r"D:\Films\Film.mkv");
        add(&mut list, film, 600.0);
        add(&mut list, Path::new(r"D:\Films\Other.mkv"), 30.0);
        assert_eq!(list[0].time, 30.0);
        // The same place again, in another case: one bookmark, first.
        add(&mut list, Path::new(r"d:\films\film.mkv"), 600.4);
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].time, 600.4);
        assert_eq!(list[1].label(), "Other.mkv — 00:30");
        let text = to_text(&list);
        assert_eq!(from_text(&text), list);
        assert_eq!(from_text("x\tD:\\a.mkv\n12\t\nbroken"), vec![]);
        for i in 0..40 {
            add(&mut list, film, f64::from(i) * 10.0);
        }
        assert_eq!(list.len(), MAX);
    }
}
