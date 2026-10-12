//! The playlist, kept by the app rather than by mpv: one file is loaded at a
//! time (`loadfile … replace`), and the end of a file moves on to the next
//! one. A plain list is easy to show, edit and test.

use std::cmp::Ordering;
use std::path::{Path, PathBuf};

/// Extensions of the files a folder contributes (a file opened by name is
/// tried whatever its extension: mpv plays far more).
pub const MEDIA_EXTENSIONS: &[&str] = &[
    // Video.
    "mp4", "m4v", "mkv", "webm", "avi", "mov", "qt", "wmv", "asf", "flv", "mpg", "mpeg", "m2v",
    "vob", "ts", "m2ts", "mts", "3gp", "3g2", "ogv", "divx", "rm", "rmvb", "f4v", // Audio.
    "mp3", "flac", "m4a", "aac", "ogg", "oga", "opus", "wav", "wma", "ape", "wv", "mka", "ac3",
    "dts",
];

/// Whether `path` has one of [`MEDIA_EXTENSIONS`].
pub fn is_media(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| MEDIA_EXTENSIONS.iter().any(|m| m.eq_ignore_ascii_case(e)))
}

#[derive(Default)]
pub struct Playlist {
    items: Vec<PathBuf>,
    /// The entry loaded in the player.
    current: Option<usize>,
    /// The entry selected in the list (for Delete).
    pub selected: Option<usize>,
}

impl Playlist {
    pub fn items(&self) -> &[PathBuf] {
        &self.items
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn current(&self) -> Option<usize> {
        self.current
    }

    pub fn current_path(&self) -> Option<&Path> {
        self.current
            .and_then(|i| self.items.get(i))
            .map(PathBuf::as_path)
    }

    /// Make entry `index` the current one; its path, to be loaded.
    pub fn set_current(&mut self, index: usize) -> Option<&Path> {
        let path = self.items.get(index)?;
        self.current = Some(index);
        Some(path)
    }

    /// Replace the list with `paths` (folders give their media files);
    /// the first becomes current.
    pub fn replace(&mut self, paths: Vec<PathBuf>) {
        self.items.clear();
        self.current = None;
        self.selected = None;
        self.add(paths);
        if !self.items.is_empty() {
            self.current = Some(0);
        }
    }

    /// Append `paths` (folders give their media files), leaving out those
    /// already listed; the index of the first one added.
    pub fn add(&mut self, paths: Vec<PathBuf>) -> Option<usize> {
        let first = self.items.len();
        for path in expand(paths) {
            if !self.items.contains(&path) {
                self.items.push(path);
            }
        }
        (self.items.len() > first).then_some(first)
    }

    /// Remove entry `index`; the current and the selected entries keep
    /// pointing at the same files (the current one becomes none if it was
    /// removed).
    pub fn remove(&mut self, index: usize) {
        if index >= self.items.len() {
            return;
        }
        self.items.remove(index);
        let shift = |i: Option<usize>| match i {
            Some(i) if i == index => None,
            Some(i) if i > index => Some(i - 1),
            other => other,
        };
        self.current = shift(self.current);
        self.selected =
            shift(self.selected).or_else(|| (index < self.items.len()).then_some(index));
        if self.items.is_empty() {
            self.selected = None;
        }
    }

    pub fn clear(&mut self) {
        *self = Self::default();
    }

    /// Sort by file name, numbers by value ("2" before "10"); the current
    /// and the selected entries follow their files.
    pub fn sort(&mut self) {
        let current = self.current_path().map(Path::to_path_buf);
        let selected = self.selected.and_then(|i| self.items.get(i)).cloned();
        self.items
            .sort_by(|a, b| natural_cmp(&name_of(a), &name_of(b)).then_with(|| a.cmp(b)));
        let find = |p: Option<PathBuf>| p.and_then(|p| self.items.iter().position(|i| *i == p));
        self.current = find(current);
        self.selected = find(selected);
    }

    /// The entry after the current one, if any.
    pub fn next(&self) -> Option<usize> {
        let i = self.current.map_or(0, |c| c + 1);
        (i < self.items.len()).then_some(i)
    }

    /// The entry before the current one, if any.
    pub fn prev(&self) -> Option<usize> {
        self.current.and_then(|c| c.checked_sub(1))
    }
}

/// The media files of the folder `file` is in, sorted, `file` among them
/// (whatever its extension).
pub fn folder_of(file: &Path) -> Vec<PathBuf> {
    let Some(dir) = file.parent() else {
        return vec![file.to_path_buf()];
    };
    let mut out = media_in(dir);
    if !out.iter().any(|p| p == file) {
        out.push(file.to_path_buf());
        out.sort_by(|a, b| natural_cmp(&name_of(a), &name_of(b)));
    }
    out
}

/// Folders replaced by their media files (sorted), files kept as they are.
fn expand(paths: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for path in paths {
        if path.is_dir() {
            out.extend(media_in(&path));
        } else {
            out.push(path);
        }
    }
    out
}

/// The media files directly in `dir`, in natural order.
fn media_in(dir: &Path) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .map(|entries| {
            entries
                .filter_map(Result::ok)
                .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
                .map(|e| e.path())
                .filter(|p| is_media(p))
                .collect()
        })
        .unwrap_or_default();
    files.sort_by(|a, b| natural_cmp(&name_of(a), &name_of(b)));
    files
}

fn name_of(path: &Path) -> String {
    path.file_name()
        .map_or_else(|| path.to_string_lossy(), |n| n.to_string_lossy())
        .to_lowercase()
}

/// Compare as Explorer does: runs of digits by their value, the rest by
/// character.
pub fn natural_cmp(a: &str, b: &str) -> Ordering {
    let (mut a, mut b) = (a.chars().peekable(), b.chars().peekable());
    loop {
        match (a.peek().copied(), b.peek().copied()) {
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(x), Some(y)) if x.is_ascii_digit() && y.is_ascii_digit() => {
                let take = |it: &mut std::iter::Peekable<std::str::Chars>| {
                    let mut digits = String::new();
                    while let Some(c) = it.next_if(char::is_ascii_digit) {
                        digits.push(c);
                    }
                    digits
                };
                let (da, db) = (take(&mut a), take(&mut b));
                let (ta, tb) = (da.trim_start_matches('0'), db.trim_start_matches('0'));
                let order = ta
                    .len()
                    .cmp(&tb.len())
                    .then_with(|| ta.cmp(tb))
                    .then_with(|| da.len().cmp(&db.len()));
                if order != Ordering::Equal {
                    return order;
                }
            }
            (Some(x), Some(y)) => {
                if x != y {
                    return x.cmp(&y);
                }
                a.next();
                b.next();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn list(names: &[&str]) -> Playlist {
        let mut p = Playlist::default();
        p.replace(names.iter().map(PathBuf::from).collect());
        p
    }

    #[test]
    fn natural_order() {
        assert_eq!(natural_cmp("ep2.mkv", "ep10.mkv"), Ordering::Less);
        assert_eq!(natural_cmp("ep10.mkv", "ep2.mkv"), Ordering::Greater);
        assert_eq!(natural_cmp("a", "a"), Ordering::Equal);
        assert_eq!(natural_cmp("a1", "a01"), Ordering::Less);
        assert_eq!(natural_cmp("abc", "abd"), Ordering::Less);
        assert_eq!(natural_cmp("ab", "abc"), Ordering::Less);
    }

    #[test]
    fn replace_add_and_move() {
        let mut p = list(&["a.mkv", "b.mkv"]);
        assert_eq!(p.current(), Some(0));
        assert_eq!(p.prev(), None);
        assert_eq!(p.next(), Some(1));
        assert_eq!(p.add(vec!["b.mkv".into(), "c.mkv".into()]), Some(2));
        assert_eq!(p.len(), 3);
        assert_eq!(p.add(vec!["a.mkv".into()]), None);
        p.set_current(2);
        assert_eq!(p.next(), None);
        assert_eq!(p.prev(), Some(1));
    }

    #[test]
    fn remove_keeps_current() {
        let mut p = list(&["a", "b", "c"]);
        p.set_current(2);
        p.remove(0);
        assert_eq!(p.current_path(), Some(Path::new("c")));
        p.remove(1);
        assert_eq!(p.current(), None);
        assert_eq!(p.next(), Some(0));
        p.remove(5);
        assert_eq!(p.len(), 1);
    }

    #[test]
    fn sort_follows_current() {
        let mut p = list(&["ep10.mkv", "ep2.mkv", "ep1.mkv"]);
        p.sort();
        let names: Vec<_> = p
            .items()
            .iter()
            .map(|i| i.to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, ["ep1.mkv", "ep2.mkv", "ep10.mkv"]);
        assert_eq!(p.current_path(), Some(Path::new("ep10.mkv")));
    }

    #[test]
    fn folder_contents() {
        let dir =
            std::env::temp_dir().join(format!("metal_player_playlist_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        for name in ["ep10.mkv", "ep2.MP4", "notes.txt", "song.flac"] {
            std::fs::write(dir.join(name), b"").unwrap();
        }
        let names = |v: Vec<PathBuf>| -> Vec<String> {
            v.iter()
                .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
                .collect()
        };
        assert_eq!(
            names(folder_of(&dir.join("ep10.mkv"))),
            ["ep2.MP4", "ep10.mkv", "song.flac"]
        );
        assert_eq!(
            names(folder_of(&dir.join("notes.txt"))),
            ["ep2.MP4", "ep10.mkv", "notes.txt", "song.flac"]
        );
        let mut p = Playlist::default();
        p.replace(vec![dir.clone()]);
        assert_eq!(p.len(), 3);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
