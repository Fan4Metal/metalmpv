//! The playlist kept between runs (`settings.restore_playlist`): an M3U8
//! next to the settings, [`FILE`], as **Save the playlist** writes it (so
//! an IPTV list keeps its names, logos and groups), with the current
//! entry's place on a comment line of its own. Written by `App::save` when
//! the list or its current entry changed, read at a start without files.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::playlist::{self, Channel, Playlist};

/// The file's name, next to the settings (`app.ron`, or `metalmpv.ron` of
/// a portable copy beside the exe, hence the program's name in it).
pub const FILE: &str = "metalmpv_playlist.m3u8";

/// The comment line naming the current entry by its path (not its place:
/// reading leaves out entries a list may not hold, `playlist::safe_entry`,
/// and the places after them move).
const CURRENT: &str = "#METALMPV-CURRENT:";

/// A list larger than this is not read back (an IPTV list of a million
/// channels is some 100 MB).
const MAX_BYTES: u64 = 256 << 20;

/// Where the playlist is kept, if the settings are kept somewhere.
pub fn file() -> Option<PathBuf> {
    let settings = crate::settings::location().file.as_ref()?;
    Some(settings.parent()?.join(FILE))
}

/// What is written: the list's M3U8 with the current entry's path.
pub fn text(list: &Playlist) -> String {
    let mut text = playlist::m3u_text(list);
    if let Some(current) = list.current().and_then(|i| list.items().get(i)) {
        // After `#EXTM3U`: a player reading the file skips the comment.
        let at = text.find('\n').map_or(text.len(), |i| i + 1);
        text.insert_str(at, &format!("{CURRENT}{}\r\n", current.display()));
    }
    text
}

/// A kept list: its entries, what it said about them, the current entry.
pub struct Kept {
    pub paths: Vec<PathBuf>,
    pub channels: HashMap<PathBuf, Channel>,
    pub current: Option<usize>,
}

/// The list in `text` (as [`text`] writes it).
pub fn parse(text: &str) -> Kept {
    let mut channels = HashMap::new();
    let paths: Vec<PathBuf> = playlist::m3u_entries(text, Path::new(""))
        .into_iter()
        .map(|(path, channel)| {
            if let Some(c) = channel {
                channels.entry(path.clone()).or_insert(c);
            }
            path
        })
        .collect();
    // Read as an entry is, so that it is named as the entries are.
    let current = text
        .lines()
        .find_map(|line| line.trim().strip_prefix(CURRENT))
        .and_then(|line| playlist::m3u_entries(line, Path::new("")).pop())
        .and_then(|(path, _)| paths.iter().position(|p| *p == path));
    Kept {
        paths,
        channels,
        current,
    }
}

/// The list kept by the last run, if there is one.
pub fn load() -> Option<Kept> {
    let file = file()?;
    if std::fs::metadata(&file).ok()?.len() > MAX_BYTES {
        log::warn!("{} is too large to restore", file.display());
        return None;
    }
    let bytes = std::fs::read(&file).ok()?;
    let kept = parse(&String::from_utf8_lossy(&bytes));
    log::debug!(
        "playlist restored: {} entries, current {:?}",
        kept.paths.len(),
        kept.current
    );
    (!kept.paths.is_empty()).then_some(kept)
}

/// What the file holds, as far as this run knows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Stored {
    /// Not written nor deleted by this run.
    #[default]
    Unknown,
    /// Deleted (the setting is off) or never written: nothing to keep.
    Nothing,
    /// The list of this revision with this current entry.
    List(u64, Option<usize>),
}

/// Keep `list` when `on`, else delete the file, unless `stored` says it
/// holds that already; what it holds after.
pub fn follow(list: &Playlist, on: bool, stored: Stored) -> Stored {
    let wanted = if on && !list.is_empty() {
        Stored::List(list.revision(), list.current())
    } else {
        Stored::Nothing
    };
    if wanted != stored {
        match wanted {
            Stored::List(..) => store(list),
            _ => forget(),
        }
    }
    wanted
}

/// Keep `list` for the next run (an empty one deletes the file): written
/// beside it and renamed over it once whole.
fn store(list: &Playlist) {
    let Some(file) = file() else {
        return;
    };
    if list.is_empty() {
        forget();
        return;
    }
    let part = file.with_extension("m3u8.part");
    let written = std::fs::write(&part, text(list)).and_then(|()| std::fs::rename(&part, &file));
    if let Err(e) = written {
        log::warn!("cannot keep the playlist in {}: {e}", file.display());
        let _ = std::fs::remove_file(&part);
    }
}

/// Delete the kept list (the setting turned off, the list emptied).
fn forget() {
    if let Some(file) = file() {
        match std::fs::remove_file(&file) {
            Ok(()) => log::debug!("kept playlist deleted"),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => log::warn!("cannot delete {}: {e}", file.display()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let mut list = Playlist::default();
        list.replace(vec![
            PathBuf::from(r"C:\Films\a.mkv"),
            PathBuf::from("https://example.com/one.m3u8"),
            PathBuf::from(r"C:\Films\b.mkv"),
        ]);
        let mut channels = HashMap::new();
        channels.insert(
            PathBuf::from("https://example.com/one.m3u8"),
            Channel {
                title: Some("One".into()),
                logo: Some("https://example.com/one.png".into()),
                group: Some("News".into()),
            },
        );
        list.describe(channels);
        list.set_current(2);
        let text = text(&list);
        assert!(text.starts_with("#EXTM3U\r\n#METALMPV-CURRENT:C:\\Films\\b.mkv\r\n"));
        let kept = parse(&text);
        assert_eq!(kept.paths, list.items());
        assert_eq!(kept.current, Some(2));
        let one = &kept.channels[Path::new("https://example.com/one.m3u8")];
        assert_eq!(one.title.as_deref(), Some("One"));
        assert_eq!(one.group.as_deref(), Some("News"));
    }

    #[test]
    fn without_a_current_entry() {
        let kept = parse("#EXTM3U\r\nC:\\a.mkv\r\n");
        assert_eq!(kept.paths, vec![PathBuf::from(r"C:\a.mkv")]);
        assert_eq!(kept.current, None);
    }

    #[test]
    fn current_by_path_when_an_entry_is_left_out() {
        // A device opened by its address is no entry of a list read back:
        // the current entry is still the one that played.
        let text = "#EXTM3U\r\n#METALMPV-CURRENT:C:\\b.mkv\r\nav://dshow:video=cam\r\n\
                    C:\\a.mkv\r\nC:\\b.mkv\r\n";
        let kept = parse(text);
        assert_eq!(
            kept.paths,
            vec![PathBuf::from(r"C:\a.mkv"), PathBuf::from(r"C:\b.mkv")]
        );
        assert_eq!(kept.current, Some(1));
        // One no longer listed: none.
        let kept = parse("#EXTM3U\r\n#METALMPV-CURRENT:C:\\gone.mkv\r\nC:\\a.mkv\r\n");
        assert_eq!(kept.current, None);
    }
}
