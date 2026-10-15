//! Bookmarks: a file and a place in it, kept between runs (the setting
//! `bookmarks`, not reset with the settings). Added from the picture's
//! context menu or `Ctrl+B`, the latest first; a click on one goes there,
//! opening its file when another plays. The file's duration is kept with
//! it: the menu shows where in the file the bookmark is.

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
    /// The file's duration when it was added.
    pub duration: Option<f64>,
}

impl Bookmark {
    /// What the menu shows: `title` (an IPTV channel's name, while its list
    /// is the playlist), else the file's name (an address's last part), and
    /// the time.
    pub fn label(&self, title: Option<&str>) -> String {
        let name = title.map(str::to_owned).unwrap_or_else(|| {
            self.path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| self.path.to_string_lossy().into_owned())
        });
        format!("{name} — {}", crate::format::time(Some(self.time), false))
    }

    /// Where in the file it is, 0 to 1, when the duration is known.
    pub fn share(&self) -> Option<f32> {
        let duration = self.duration.filter(|d| *d > 0.0)?;
        Some((self.time / duration).clamp(0.0, 1.0) as f32)
    }

    pub fn is_of(&self, path: &Path) -> bool {
        same_path(&self.path, path)
    }
}

/// Paths compared as Windows does, without regard to case.
pub fn same_path(a: &Path, b: &Path) -> bool {
    a.as_os_str().eq_ignore_ascii_case(b.as_os_str())
}

/// Add a bookmark at `time` of `path` (of `duration`) first in `list`, in
/// place of one of that file within [`SAME_PLACE`]; at most [`MAX`].
pub fn add(list: &mut Vec<Bookmark>, path: &Path, time: f64, duration: Option<f64>) {
    list.retain(|b| !(b.is_of(path) && (b.time - time).abs() < SAME_PLACE));
    list.insert(
        0,
        Bookmark {
            path: path.to_path_buf(),
            time: time.max(0.0),
            duration: duration.filter(|d| d.is_finite() && *d > 0.0),
        },
    );
    list.truncate(MAX);
}

/// The setting: a line each, `seconds<TAB>duration<TAB>path`, `-` for a
/// duration not known.
pub fn to_text(list: &[Bookmark]) -> String {
    list.iter()
        .map(|b| {
            let duration = b.duration.map_or("-".into(), |d| format!("{d:.1}"));
            format!("{:.3}\t{duration}\t{}", b.time, b.path.display())
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// The bookmarks of the setting (lines of `seconds<TAB>path` too); lines
/// not understood are left out.
pub fn from_text(text: &str) -> Vec<Bookmark> {
    text.lines()
        .filter_map(|line| {
            let fields: Vec<&str> = line.splitn(3, '\t').collect();
            let (time, duration, path) = match fields[..] {
                [time, path] => (time, "-", path),
                [time, duration, path] => (time, duration, path),
                _ => return None,
            };
            let time = time.parse::<f64>().ok().filter(|t| t.is_finite())?;
            let duration = match duration {
                "-" => None,
                d => Some(
                    d.parse::<f64>()
                        .ok()
                        .filter(|d| d.is_finite() && *d > 0.0)?,
                ),
            };
            (!path.is_empty()).then(|| Bookmark {
                path: PathBuf::from(path),
                time: time.max(0.0),
                duration,
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
        add(&mut list, film, 600.0, Some(2400.0));
        add(&mut list, Path::new(r"D:\Films\Other.mkv"), 30.0, None);
        assert_eq!(list[0].time, 30.0);
        // The same place again, in another case: one bookmark, first.
        add(
            &mut list,
            Path::new(r"d:\films\film.mkv"),
            600.4,
            Some(2400.0),
        );
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].time, 600.4);
        assert_eq!(list[1].label(None), "Other.mkv — 00:30");
        assert_eq!(list[1].label(Some("Channel One")), "Channel One — 00:30");
        assert!((list[0].share().unwrap() - 0.25).abs() < 0.001);
        assert_eq!(list[1].share(), None);
        let text = to_text(&list);
        assert_eq!(from_text(&text), list);
        assert_eq!(from_text("x\tD:\\a.mkv\n12\t\nbroken"), vec![]);
        // The first format, without the duration.
        assert_eq!(
            from_text("7.5\tD:\\b.mkv"),
            vec![Bookmark {
                path: PathBuf::from(r"D:\b.mkv"),
                time: 7.5,
                duration: None
            }]
        );
        for i in 0..40 {
            add(&mut list, film, f64::from(i) * 10.0, None);
        }
        assert_eq!(list.len(), MAX);
    }
}
