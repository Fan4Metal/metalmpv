//! The playlist, kept by the app rather than by mpv: one file is loaded at a
//! time (`loadfile … replace`), and the end of a file moves on to the next
//! one. A plain list is easy to show, edit and test.

use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
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

/// The largest playlist file read.
const MAX_LIST_BYTES: u64 = 256 << 20;

/// Playlist files, opened as their entries and written by Save the list.
pub const PLAYLIST_EXTENSIONS: &[&str] = &["m3u", "m3u8"];

/// Whether `path` is a playlist file (M3U).
pub fn is_playlist_file(path: &Path) -> bool {
    !is_url(path)
        && path.extension().and_then(|e| e.to_str()).is_some_and(|e| {
            PLAYLIST_EXTENSIONS
                .iter()
                .any(|m| m.eq_ignore_ascii_case(e))
        })
}

/// What an M3U list's `#EXTINF` line says about the entry after it: the
/// name after the comma (an IPTV list's channel), the `tvg-logo` picture's
/// address and the `group-title`. Only text: nothing in it is run, and the
/// logo is fetched by `logos.rs` under its own rules.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Channel {
    pub title: Option<String>,
    pub logo: Option<String>,
    pub group: Option<String>,
}

impl Channel {
    /// The `#EXTINF` line's text after the colon; `None` when it names
    /// nothing.
    fn parse(line: &str) -> Option<Self> {
        // `-1 tvg-id="…" tvg-logo="…" group-title="…",Title`: the title
        // follows the first comma outside quotes. Some lists quote values
        // with apostrophes (`tvg-logo='…'`); an apostrophe elsewhere is a
        // letter ("Children's").
        let mut quote: Option<char> = None;
        let mut previous = ' ';
        let comma = line.char_indices().find_map(|(i, c)| {
            let after = previous;
            previous = c;
            match quote {
                Some(q) if c == q => quote = None,
                Some(_) => {}
                None if c == '"' || (c == '\'' && after == '=') => quote = Some(c),
                None if c == ',' => return Some(i),
                None => {}
            }
            None
        });
        let (head, title) = match comma {
            Some(i) => (&line[..i], Some(line[i + 1..].trim())),
            None => (line, None),
        };
        let attribute = |name: &str| {
            ['"', '\''].into_iter().find_map(|q| {
                let start = head.find(&format!("{name}={q}"))? + name.len() + 2;
                let end = head[start..].find(q)? + start;
                Some(clean(&head[start..end])).filter(|v| !v.is_empty())
            })
        };
        let channel = Self {
            title: title.map(clean).filter(|t| !t.is_empty()),
            logo: attribute("tvg-logo"),
            group: attribute("group-title"),
        };
        (channel != Self::default()).then_some(channel)
    }

    /// The `#EXTINF` line written back for this entry.
    fn line(&self) -> String {
        let mut line = String::from("#EXTINF:-1");
        for (name, value) in [("tvg-logo", &self.logo), ("group-title", &self.group)] {
            if let Some(v) = value {
                line.push_str(&format!(" {name}=\"{}\"", v.replace('"', "'")));
            }
        }
        line.push(',');
        line.push_str(self.title.as_deref().unwrap_or_default());
        line
    }
}

/// A value of a list without control characters (a line break in a title
/// would break the line written back).
fn clean(text: &str) -> String {
    text.chars()
        .filter(|c| !c.is_control())
        .collect::<String>()
        .trim()
        .to_owned()
}

/// Whether an M3U `text` is an HLS stream's (its segments, or its
/// variants of quality): mpv plays it as one stream, where its lines as
/// entries were segments of a few seconds each.
pub fn is_hls(text: &str) -> bool {
    [
        "#EXT-X-TARGETDURATION",
        "#EXT-X-STREAM-INF",
        "#EXT-X-MEDIA-SEQUENCE",
    ]
    .iter()
    .any(|tag| text.contains(tag))
}

/// The path a `file:` address names (`file:///C:/My%20Films/a.mkv`,
/// `file://server/share/a.mkv`), as VLC writes its lists.
fn file_uri_path(line: &str) -> Option<PathBuf> {
    let scheme = line.get(..7)?;
    if !scheme.eq_ignore_ascii_case("file://") {
        return None;
    }
    let rest = &line[7..];
    // `file:///C:/…` a drive; `file://host/…` a share.
    let rest = match rest.strip_prefix('/') {
        Some(local) => local.to_owned(),
        None => format!("//{rest}"),
    };
    let bytes = rest.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let hex = |b: u8| (b as char).to_digit(16);
        match (bytes[i], bytes.get(i + 1), bytes.get(i + 2)) {
            (b'%', Some(&h), Some(&l)) if hex(h).is_some() && hex(l).is_some() => {
                decoded.push((hex(h)? * 16 + hex(l)?) as u8);
                i += 3;
            }
            (b, _, _) => {
                decoded.push(b);
                i += 1;
            }
        }
    }
    Some(PathBuf::from(
        String::from_utf8_lossy(&decoded).replace('/', "\\"),
    ))
}

/// `path` without `.` and `..` (a list's `..\Music\a.mp3`), as the same file
/// opened directly is named.
fn lexical(path: &Path) -> PathBuf {
    use std::path::Component;
    let mut out = PathBuf::new();
    for part in path.components() {
        match part {
            Component::CurDir => {}
            Component::ParentDir => {
                if !matches!(out.components().next_back(), Some(Component::Normal(_))) || !out.pop()
                {
                    out.push(part);
                }
            }
            other => out.push(other),
        }
    }
    out
}

/// The entries of an M3U playlist's `text`: every line but blank ones and
/// `#` comments; a relative path is from `dir`, a `file:` address is a
/// path, another address is kept as it is. An `#EXTINF` line describes
/// the entry after it ([`Channel`]).
pub fn m3u_entries(text: &str, dir: &Path) -> Vec<(PathBuf, Option<Channel>)> {
    let mut entries = Vec::new();
    let mut described = None;
    for line in text.trim_start_matches('\u{feff}').lines().map(str::trim) {
        if line.is_empty() {
            continue;
        }
        if let Some(info) = line.strip_prefix("#EXTINF:") {
            described = Channel::parse(info);
            continue;
        }
        if line.starts_with('#') {
            continue;
        }
        let path = match file_uri_path(line) {
            Some(path) => path,
            None => PathBuf::from(line),
        };
        let path = if is_url(&path) {
            if !safe_entry(&path, false) {
                log::warn!("list entry skipped: {}", path.display());
                described = None;
                continue;
            }
            path
        } else if path.is_absolute() {
            lexical(&path)
        } else {
            lexical(&dir.join(path))
        };
        entries.push((path, described.take()));
    }
    entries
}

/// `paths` with each playlist file replaced by its entries (UTF-8, else
/// an `.m3u` in the system's code page, as Winamp wrote them; one that
/// cannot be read is dropped; an HLS stream's is kept as a file to play),
/// and what the lists say about their entries.
pub fn expand_lists(paths: Vec<PathBuf>) -> (Vec<PathBuf>, HashMap<PathBuf, Channel>) {
    let mut channels = HashMap::new();
    let paths = paths
        .into_iter()
        .flat_map(|p| {
            if !is_playlist_file(&p) {
                return vec![p];
            }
            // A list of a million channels is some 100 MB; more is no list
            // (out of memory the program would end).
            if std::fs::metadata(&p).is_ok_and(|m| m.len() > MAX_LIST_BYTES) {
                log::warn!("{} is too large for a list", p.display());
                return Vec::new();
            }
            match std::fs::read(&p) {
                Ok(bytes) => {
                    let utf8 = p
                        .extension()
                        .is_some_and(|e| e.eq_ignore_ascii_case("m3u8"));
                    let text = match String::from_utf8(bytes) {
                        Ok(text) => text,
                        Err(e) if utf8 => String::from_utf8_lossy(e.as_bytes()).into_owned(),
                        Err(e) => crate::win::ansi_text(e.as_bytes())
                            .unwrap_or_else(|| String::from_utf8_lossy(e.as_bytes()).into_owned()),
                    };
                    if is_hls(&text) {
                        return vec![p];
                    }
                    m3u_entries(&text, p.parent().unwrap_or(Path::new("")))
                        .into_iter()
                        .map(|(path, channel)| {
                            // The first of a path listed twice (one stream
                            // under two names): the entry kept is the first.
                            if let Some(c) = channel {
                                channels.entry(path.clone()).or_insert(c);
                            }
                            path
                        })
                        .collect()
                }
                Err(e) => {
                    log::warn!("cannot read the playlist {}: {e}", p.display());
                    Vec::new()
                }
            }
        })
        .collect();
    (paths, channels)
}

/// An M3U8 playlist of the list's entries (UTF-8, full paths, CRLF), each
/// with its `#EXTINF` line when it came with one.
pub fn m3u_text(list: &Playlist) -> String {
    let mut text = String::from("#EXTM3U\r\n");
    for item in &list.items {
        if let Some(channel) = list.channels.get(item) {
            text.push_str(&channel.line());
            text.push_str("\r\n");
        }
        text.push_str(&item.display().to_string());
        text.push_str("\r\n");
    }
    text
}

/// Whether `path` is an address (`https://…`, `rtsp://…`) rather than a
/// file: a scheme of letters, digits, `+`, `-` or `.` before `://`.
pub fn is_url(path: &Path) -> bool {
    let text = path.to_string_lossy();
    text.split_once("://").is_some_and(|(scheme, _)| {
        scheme.len() > 1
            && scheme.starts_with(|c: char| c.is_ascii_alphabetic())
            && scheme
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || "+-.".contains(c))
    })
}

/// The schemes a list's entry may have: streams, not FFmpeg's devices
/// (`av://dshow:…` turns a camera on) or sockets (`tcp://`): the app loads
/// each entry itself, so mpv's `load-unsafe-playlists` does not apply.
const LIST_SCHEMES: &[&str] = &[
    "http", "https", "rtsp", "rtsps", "rtmp", "rtmps", "rtmpe", "rtmpt", "rtmpte", "rtmpts", "rtp",
    "srt", "rist", "udp", "mms", "mmsh", "mmst", "ftp",
];

/// Whether an entry of a list may be played: an address of a stream's
/// scheme, or a path of a list on this computer (a list from an address
/// names none: mpv's own rule, and a share would be asked for the user's
/// sign-in).
pub fn safe_entry(entry: &Path, remote_list: bool) -> bool {
    if !is_url(entry) {
        return !remote_list;
    }
    let text = entry.to_string_lossy();
    text.split_once("://")
        .is_some_and(|(scheme, _)| LIST_SCHEMES.iter().any(|s| s.eq_ignore_ascii_case(scheme)))
}

/// Whether `path` has one of [`MEDIA_EXTENSIONS`].
pub fn is_media(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| MEDIA_EXTENSIONS.iter().any(|m| m.eq_ignore_ascii_case(e)))
}

/// Whether `path` is a sound file by its extension (a type of
/// `filetypes.rs` that holds sound: MKA, AC3 and DTS too).
pub fn is_audio(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .and_then(crate::filetypes::of_extension)
        .is_some_and(|t| t.kind == crate::filetypes::Kind::Audio)
}

/// What plays after the end of a file.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Repeat {
    /// The next file, none after the last.
    #[default]
    Off,
    /// The same file again (mpv's `loop-file`).
    One,
    /// The next file, the first after the last.
    All,
}

impl Repeat {
    pub const ALL: [Repeat; 3] = [Repeat::Off, Repeat::One, Repeat::All];

    /// The stored name.
    pub fn name(self) -> &'static str {
        match self {
            Repeat::Off => "off",
            Repeat::One => "one",
            Repeat::All => "all",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|r| r.name() == name)
    }

    /// Off, then the file, then the list (`R`).
    pub fn next(self) -> Self {
        match self {
            Repeat::Off => Repeat::One,
            Repeat::One => Repeat::All,
            Repeat::All => Repeat::Off,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Repeat::Off => tr!("Off", "Выкл."),
            Repeat::One => tr!("The file", "Файл"),
            Repeat::All => tr!("The list", "Плейлист"),
        }
    }
}

#[derive(Clone, Default)]
pub struct Playlist {
    items: Vec<PathBuf>,
    /// The entry loaded in the player.
    current: Option<usize>,
    /// The entries selected in the list, by their files (each is listed
    /// once): the selection follows them through sorting and removal.
    selected: HashSet<PathBuf>,
    /// Where a range selected with Shift starts.
    anchor: Option<PathBuf>,
    /// What the M3U lists said about their entries.
    channels: HashMap<PathBuf, Channel>,
    /// The file playing, taken out of the list: still the current path
    /// (its title, its cover), and the entry that took its place, which
    /// comes next.
    gone: Option<PathBuf>,
    after_gone: Option<usize>,
    /// Counts the changes of the entries, their order and what the lists
    /// said, for views made of them (the panel's rows).
    revision: u64,
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

    pub fn revision(&self) -> u64 {
        self.revision
    }

    fn changed(&mut self) {
        self.revision = self.revision.wrapping_add(1);
    }

    /// Keep what a list said about its entries (`expand_lists`).
    pub fn describe(&mut self, channels: HashMap<PathBuf, Channel>) {
        self.channels.extend(channels);
        self.changed();
    }

    /// What the list said about `path`.
    pub fn channel(&self, path: &Path) -> Option<&Channel> {
        self.channels.get(path)
    }

    /// The name the list gave `path` (a channel's), else its file name,
    /// else all of it.
    pub fn title(&self, path: &Path) -> String {
        self.channel(path)
            .and_then(|c| c.title.clone())
            .or_else(|| path.file_name().map(|n| n.to_string_lossy().into_owned()))
            .unwrap_or_else(|| path.to_string_lossy().into_owned())
    }

    /// The file of the current entry, or the one playing that was taken
    /// out of the list.
    pub fn current_path(&self) -> Option<&Path> {
        self.current
            .and_then(|i| self.items.get(i))
            .or(self.gone.as_ref())
            .map(PathBuf::as_path)
    }

    /// Make entry `index` the current one; its path, to be loaded.
    pub fn set_current(&mut self, index: usize) -> Option<&Path> {
        if index >= self.items.len() {
            return None;
        }
        self.current = Some(index);
        // What the list said of the playing entry taken out is let go now.
        if let Some(gone) = self.gone.take()
            && !self.items.contains(&gone)
        {
            self.channels.remove(&gone);
        }
        self.after_gone = None;
        self.items.get(index).map(PathBuf::as_path)
    }

    pub fn is_selected(&self, index: usize) -> bool {
        self.items
            .get(index)
            .is_some_and(|p| self.selected.contains(p))
    }

    /// The selected entries in the list's order.
    pub fn selection(&self) -> Vec<usize> {
        if self.selected.is_empty() {
            return Vec::new();
        }
        (0..self.items.len())
            .filter(|i| self.selected.contains(&self.items[*i]))
            .collect()
    }

    pub fn has_selection(&self) -> bool {
        !self.selected.is_empty()
    }

    /// Select entry `index` alone (a click).
    pub fn select(&mut self, index: usize) {
        self.selected.clear();
        if let Some(path) = self.items.get(index) {
            self.selected.insert(path.clone());
            self.anchor = Some(path.clone());
        }
    }

    /// Select or unselect entry `index`, the rest staying (Ctrl+click).
    pub fn toggle_selected(&mut self, index: usize) {
        if let Some(path) = self.items.get(index) {
            if !self.selected.remove(path) {
                self.selected.insert(path.clone());
            }
            self.anchor = Some(path.clone());
        }
    }

    /// Select the entries from the anchor to `index` (Shift+click) as
    /// `shown` lists them (the view's entries in its order: a search leaves
    /// some out, groups put them in another order), or `index` alone when
    /// the anchor is not shown.
    pub fn select_range(&mut self, index: usize, shown: &[usize]) {
        let anchor = self.anchor.as_ref();
        let from = shown
            .iter()
            .position(|&i| anchor.is_some_and(|a| self.items.get(i) == Some(a)));
        let to = shown.iter().position(|&i| i == index);
        let (Some(from), Some(to)) = (from, to) else {
            self.select(index);
            return;
        };
        self.selected = shown[from.min(to)..=from.max(to)]
            .iter()
            .filter_map(|&i| self.items.get(i).cloned())
            .collect();
    }

    /// Select the entries `shown`.
    pub fn select_all(&mut self, shown: &[usize]) {
        self.selected = shown
            .iter()
            .filter_map(|&i| self.items.get(i).cloned())
            .collect();
    }

    /// Unselect what `shown` says is not in sight (a search, a group
    /// folded): Delete would remove what cannot be seen.
    pub fn keep_selected(&mut self, shown: impl Fn(&Path) -> bool) {
        self.selected.retain(|p| shown(p));
    }

    /// Replace the list with `paths` (folders give their media files);
    /// the first becomes current.
    pub fn replace(&mut self, paths: Vec<PathBuf>) {
        self.items.clear();
        self.channels.clear();
        self.current = None;
        self.selected.clear();
        self.anchor = None;
        self.gone = None;
        self.after_gone = None;
        self.add(paths);
        if !self.items.is_empty() {
            self.current = Some(0);
        }
        self.changed();
    }

    /// Append `paths` (folders give their media files), leaving out those
    /// already listed; the index of the first of them, added or already
    /// there (a file dropped again plays when nothing does).
    pub fn add(&mut self, paths: Vec<PathBuf>) -> Option<usize> {
        // A map, not `position`: an IPTV list has thousands of entries.
        let mut listed: HashMap<PathBuf, usize> = self
            .items
            .iter()
            .enumerate()
            .map(|(i, p)| (p.clone(), i))
            .collect();
        let mut first = None;
        for path in expand(paths) {
            let at = *listed.entry(path.clone()).or_insert_with(|| {
                self.items.push(path);
                self.items.len() - 1
            });
            first.get_or_insert(at);
        }
        self.changed();
        first
    }

    /// Put `entries` (a list mpv opened, with the titles it gave them) in
    /// place of the current entry; entry `playing` of them becomes the
    /// current one. Those already listed elsewhere are left out.
    pub fn expand_current(&mut self, entries: Vec<(PathBuf, Option<String>)>, playing: usize) {
        let Some(at) = self.current else {
            return;
        };
        let playing_path = entries.get(playing).map(|(p, _)| p.clone());
        let listed: HashSet<PathBuf> = self
            .items
            .iter()
            .enumerate()
            .filter(|(i, _)| *i != at)
            .map(|(_, p)| p.clone())
            .collect();
        let mut new = Vec::new();
        // A set as well: a list of thousands of channels.
        let mut seen = HashSet::new();
        for (path, title) in entries {
            if listed.contains(&path) || !seen.insert(path.clone()) {
                continue;
            }
            if let Some(title) = title.map(|t| clean(&t)).filter(|t| !t.is_empty()) {
                let channel = self.channels.entry(path.clone()).or_default();
                channel.title.get_or_insert(title);
            }
            new.push(path);
        }
        let count = new.len();
        let first = new.first().cloned();
        let old: Vec<PathBuf> = self.items.splice(at..=at, new).collect();
        // The list selected: its first entry is.
        if let Some(old) = old.first()
            && self.selected.remove(old)
            && let Some(first) = first
        {
            self.selected.insert(first);
        }
        self.current = playing_path
            .and_then(|p| self.items.iter().position(|i| *i == p))
            .or((count > 0).then_some(at));
        self.changed();
    }

    /// Take entry `index` out; the current one taken out stays the current
    /// path while it plays, and the entry after it comes next.
    fn take_out(&mut self, index: usize) {
        let removed = self.items.remove(index);
        self.selected.remove(&removed);
        if self.current == Some(index) {
            // Its title is still the window's while it plays.
            self.gone = Some(removed);
            self.after_gone = Some(index);
        } else {
            if let Some(after) = self.after_gone
                && after > index
            {
                self.after_gone = Some(after - 1);
            }
            self.channels.remove(&removed);
        }
        self.current = match self.current {
            Some(i) if i == index => None,
            Some(i) if i > index => Some(i - 1),
            other => other,
        };
    }

    /// Remove entry `index`; the current entry keeps pointing at the same
    /// file, and the entry that takes its place is selected.
    #[cfg(test)]
    pub fn remove(&mut self, index: usize) {
        self.remove_many(&[index]);
    }

    /// Remove the entries `indices`; the entry that takes the place of the
    /// first of them is selected.
    pub fn remove_many(&mut self, indices: &[usize]) {
        let mut indices: Vec<usize> = indices
            .iter()
            .copied()
            .filter(|i| *i < self.items.len())
            .collect();
        if indices.is_empty() {
            return;
        }
        indices.sort_unstable();
        indices.dedup();
        let first = indices[0];
        for &i in indices.iter().rev() {
            self.take_out(i);
        }
        self.selected.clear();
        if !self.items.is_empty() {
            self.select(first.min(self.items.len() - 1));
        }
        self.changed();
    }

    /// Remove the selected entries.
    pub fn remove_selected(&mut self) {
        let selection = self.selection();
        self.remove_many(&selection);
    }

    /// Move the selected entries, in their order, to `slot` (0 before the
    /// first entry, `len` after the last; counted before the move).
    pub fn move_selected(&mut self, slot: usize) {
        let moved = self.selection();
        if moved.is_empty() || slot > self.items.len() {
            return;
        }
        let current = self.current.and_then(|i| self.items.get(i)).cloned();
        let before = moved.iter().filter(|i| **i < slot).count();
        let mut taken = Vec::with_capacity(moved.len());
        for &i in moved.iter().rev() {
            taken.push(self.items.remove(i));
        }
        taken.reverse();
        let at = slot - before;
        self.items.splice(at..at, taken);
        self.current = current.and_then(|p| self.items.iter().position(|i| *i == p));
        // The place of a playing entry taken out means nothing now.
        self.after_gone = None;
        self.changed();
    }

    pub fn clear(&mut self) {
        let revision = self.revision;
        *self = Self::default();
        self.revision = revision;
        self.changed();
    }

    /// Sort by name (a channel's, else the file's), numbers by value ("2"
    /// before "10"); the current entry follows its file.
    pub fn sort(&mut self) {
        let current = self.current_path().map(Path::to_path_buf);
        // The names made once, not at each comparison (lists of thousands).
        let mut keyed: Vec<(String, PathBuf)> = std::mem::take(&mut self.items)
            .into_iter()
            .map(|p| (self.title(&p).to_lowercase(), p))
            .collect();
        keyed.sort_by(|(ka, a), (kb, b)| natural_cmp(ka, kb).then_with(|| a.cmp(b)));
        self.items = keyed.into_iter().map(|(_, p)| p).collect();
        self.current = current.and_then(|p| self.items.iter().position(|i| *i == p));
        // The place of a playing entry taken out means nothing now.
        self.after_gone = None;
        self.changed();
    }

    /// The entry after the current one (after the place of the playing
    /// one taken out), if any.
    pub fn next(&self) -> Option<usize> {
        let i = match (self.current, self.after_gone) {
            (Some(c), _) => c + 1,
            (None, Some(after)) => after,
            (None, None) => 0,
        };
        (i < self.items.len()).then_some(i)
    }

    /// The entry before the current one, if any.
    pub fn prev(&self) -> Option<usize> {
        self.current
            .or(self.after_gone)
            .and_then(|c| c.checked_sub(1))
    }

    /// The entry after the current one; with the list repeated, the first
    /// after the last.
    pub fn next_in(&self, repeat: Repeat) -> Option<usize> {
        self.next()
            .or_else(|| (repeat == Repeat::All && !self.items.is_empty()).then_some(0))
    }

    /// The entry before the current one; with the list repeated, the last
    /// before the first.
    pub fn prev_in(&self, repeat: Repeat) -> Option<usize> {
        self.prev().or_else(|| {
            (repeat == Repeat::All && !self.items.is_empty()).then(|| self.items.len() - 1)
        })
    }

    /// Put the entries in a random order (`seed` picks it); the current
    /// entry goes first, so that all the others play after it.
    pub fn shuffle(&mut self, seed: u64) {
        let current = self.current_path().map(Path::to_path_buf);
        // xorshift64*, enough to deal a list.
        let mut state = seed | 1;
        let mut random = || {
            state ^= state >> 12;
            state ^= state << 25;
            state ^= state >> 27;
            state.wrapping_mul(0x2545_F491_4F6C_DD1D)
        };
        for i in (1..self.items.len()).rev() {
            let j = (random() % (i as u64 + 1)) as usize;
            self.items.swap(i, j);
        }
        // Left where it fell, the entries before it would not play.
        if let Some(i) = current.and_then(|p| self.items.iter().position(|i| *i == p)) {
            self.items[..=i].rotate_right(1);
            self.current = Some(0);
        }
        // The place of a playing entry taken out means nothing now.
        self.after_gone = None;
        self.changed();
    }
}

/// The media files of the folder `file` is in, sorted, `file` among them
/// (whatever its extension); with `audio_only`, only the sound files (an
/// album's tracks, not the clip beside them).
pub fn folder_of(file: &Path, audio_only: bool) -> Vec<PathBuf> {
    let Some(dir) = file.parent() else {
        return vec![file.to_path_buf()];
    };
    let mut out = media_in(dir);
    if audio_only {
        out.retain(|p| is_audio(p));
    }
    // Names in another case are the same file on Windows: the entry takes
    // the spelling the file was opened with (it is looked for by it).
    let wanted = file.to_string_lossy().to_lowercase();
    match out
        .iter_mut()
        .find(|p| p.to_string_lossy().to_lowercase() == wanted)
    {
        Some(entry) => *entry = file.to_path_buf(),
        None => {
            out.push(file.to_path_buf());
            out.sort_by(|a, b| natural_cmp(&name_of(a), &name_of(b)));
        }
    }
    out
}

/// Folders replaced by their media files (sorted), files kept as they are.
fn expand(paths: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for path in paths {
        // An address asks nothing of the disk (an IPTV list has thousands),
        // nor does a media file (a list's entries on a share switched off
        // waited for the network each).
        if !is_url(&path) && !is_media(&path) && path.is_dir() {
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
    fn m3u_read_and_written() {
        let text = "\u{feff}#EXTM3U\r\n#EXTINF:120,One\r\none.mkv\r\n\r\nsub\\two.mp4\r\nD:\\Films\\three.avi\r\nhttps://example.com/s.m3u8\r\n";
        let dir = Path::new(r"C:\Lists");
        let one = Channel {
            title: Some("One".into()),
            ..Channel::default()
        };
        assert_eq!(
            m3u_entries(text, dir),
            vec![
                (PathBuf::from(r"C:\Lists\one.mkv"), Some(one)),
                (PathBuf::from(r"C:\Lists\sub\two.mp4"), None),
                (PathBuf::from(r"D:\Films\three.avi"), None),
                (PathBuf::from("https://example.com/s.m3u8"), None),
            ]
        );
        let items = vec![PathBuf::from(r"D:\a.mkv"), PathBuf::from("https://x/y")];
        let mut list = Playlist::default();
        list.replace(items.clone());
        assert_eq!(m3u_text(&list), "#EXTM3U\r\nD:\\a.mkv\r\nhttps://x/y\r\n");
        let read: Vec<PathBuf> = m3u_entries(&m3u_text(&list), dir)
            .into_iter()
            .map(|(p, _)| p)
            .collect();
        assert_eq!(read, items);
        assert!(is_playlist_file(Path::new(r"C:\a.M3U8")));
        assert!(!is_playlist_file(Path::new(
            "https://example.com/live.m3u8"
        )));
    }

    #[test]
    fn m3u_file_addresses_and_dots() {
        let text = "file:///C:/My%20Films/%D0%A4%D0%B8%D0%BB%D1%8C%D0%BC.mkv\n\
                    FILE://server/share/a%2Bb.mp3\n\
                    ..\\Music\\.\\c.mp3\n\
                    D:\\x\\..\\y.mkv\n";
        let read: Vec<PathBuf> = m3u_entries(text, Path::new(r"C:\Lists\Old"))
            .into_iter()
            .map(|(p, _)| p)
            .collect();
        assert_eq!(
            read,
            [
                PathBuf::from(r"C:\My Films\Фильм.mkv"),
                PathBuf::from(r"\\server\share\a+b.mp3"),
                PathBuf::from(r"C:\Lists\Music\c.mp3"),
                PathBuf::from(r"D:\y.mkv"),
            ]
        );
        // A device or a socket is no stream; its description goes with it.
        let text = "#EXTINF:-1,Camera\nav://dshow:video=Camera\ntcp://10.0.0.1:22\n\
                    #EXTINF:-1,One\nUDP://@239.0.0.1:1234\n";
        let read = m3u_entries(text, Path::new(r"C:\Lists"));
        assert_eq!(read.len(), 1);
        assert_eq!(read[0].0, PathBuf::from("UDP://@239.0.0.1:1234"));
        assert_eq!(
            read[0].1.as_ref().and_then(|c| c.title.as_deref()),
            Some("One")
        );
        // A list from an address names no paths.
        assert!(safe_entry(Path::new(r"\\host\s\a.mkv"), false));
        assert!(!safe_entry(Path::new(r"\\host\s\a.mkv"), true));
        assert!(!safe_entry(Path::new(r"C:\a.mkv"), true));
        assert!(safe_entry(Path::new("https://a/b.ts"), true));
    }

    #[test]
    fn hls_lists_are_streams() {
        assert!(is_hls(
            "#EXTM3U\n#EXT-X-TARGETDURATION:6\n#EXTINF:6.0,\nseg0.ts\n"
        ));
        assert!(is_hls("#EXTM3U\n#EXT-X-STREAM-INF:BANDWIDTH=1\nhi.m3u8\n"));
        assert!(!is_hls("#EXTM3U\n#EXTINF:-1,One\nhttp://a/b\n"));
        let dir = std::env::temp_dir().join(format!("metalmpv_hls_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let hls = dir.join("index.m3u8");
        std::fs::write(&hls, "#EXTM3U\n#EXT-X-TARGETDURATION:6\nseg0.ts\nseg1.ts\n").unwrap();
        // An old list in the system's code page (Windows-1251 here, a
        // Russian system: "Песня").
        let old = dir.join("old.m3u");
        let mut bytes = b"#EXTINF:1,".to_vec();
        bytes.extend([0xCF, 0xE5, 0xF1, 0xED, 0xFF]);
        bytes.extend(b"\nsong.mp3\n");
        std::fs::write(&old, bytes).unwrap();
        let (paths, channels) = expand_lists(vec![hls.clone(), old]);
        assert_eq!(paths, [hls, dir.join("song.mp3")]);
        let title = channels[&dir.join("song.mp3")].title.clone().unwrap();
        if crate::win::ansi_text(&[0xCF]).as_deref() == Some("П") {
            assert_eq!(title, "Песня");
        }
        assert!(!title.contains('\u{fffd}'));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn iptv_channels() {
        let text = "#EXTM3U x-tvg-url=\"https://e/epg.xml.gz\"\r\n\
            #EXTINF:-1 tvg-id=\"one\" tvg-name=\"Один, канал\" tvg-logo=\"https://e/one.png\" group-title=\"Новости\", Один, канал\r\n\
            http://10.0.0.1:16060/one/index.m3u8\r\n\
            \r\n\
            #EXTINF:-1,Два\r\n\
            #EXTVLCOPT:http-user-agent=x\r\n\
            http://10.0.0.1:16060/two/index.m3u8\r\n";
        let entries = m3u_entries(text, Path::new(""));
        let one = Channel {
            title: Some("Один, канал".into()),
            logo: Some("https://e/one.png".into()),
            group: Some("Новости".into()),
        };
        let two = Channel {
            title: Some("Два".into()),
            ..Channel::default()
        };
        assert_eq!(entries[0].1.as_ref(), Some(&one));
        assert_eq!(entries[1].1.as_ref(), Some(&two));
        let mut list = Playlist::default();
        list.replace(entries.iter().map(|(p, _)| p.clone()).collect());
        list.describe(
            entries
                .into_iter()
                .filter_map(|(p, c)| Some((p, c?)))
                .collect(),
        );
        assert_eq!(list.title(&list.items()[0].clone()), "Один, канал");
        // A channel taken out of the list is forgotten; the one playing
        // only once another plays.
        let mut cut = list.clone();
        let (first, second) = (cut.items()[0].clone(), cut.items()[1].clone());
        cut.remove(1);
        assert_eq!(cut.channel(&second), None);
        cut.remove(0);
        assert_eq!(cut.title(&first), "Один, канал");
        cut.add(vec![second.clone()]);
        cut.set_current(0);
        assert_eq!(cut.channel(&first), None);
        // Written back and read again, the same.
        let again = m3u_entries(&m3u_text(&list), Path::new(""));
        assert_eq!(again[0].1.as_ref(), Some(&one));
        assert_eq!(again[1].1.as_ref(), Some(&two));
        // A line break or a quote in a title cannot break the line.
        let odd = Channel::parse("-1 tvg-logo=\"a\"b\",x\ny").unwrap();
        assert!(!odd.line().contains('\n'));
        assert_eq!(Channel::parse("-1,"), None);
        // Values in apostrophes; an apostrophe in a value or a title is a
        // letter.
        let single =
            Channel::parse("-1 tvg-logo='https://e/a, b.png' group-title=\"Children's\",Kid's TV")
                .unwrap();
        assert_eq!(single.logo.as_deref(), Some("https://e/a, b.png"));
        assert_eq!(single.group.as_deref(), Some("Children's"));
        assert_eq!(single.title.as_deref(), Some("Kid's TV"));
    }

    #[test]
    fn urls() {
        assert!(is_url(Path::new("https://example.com/a.mp4")));
        assert!(is_url(Path::new("rtsp://10.0.0.1:554/stream")));
        assert!(!is_url(Path::new(r"C:\Films\a.mkv")));
        assert!(!is_url(Path::new(r"\\server\share\a.mkv")));
        assert!(!is_url(Path::new("C://a")));
    }

    #[test]
    fn repeat_wraps_the_list() {
        let mut p = list(&["a", "b", "c"]);
        p.set_current(2);
        assert_eq!(p.next_in(Repeat::Off), None);
        assert_eq!(p.next_in(Repeat::One), None);
        assert_eq!(p.next_in(Repeat::All), Some(0));
        p.set_current(0);
        assert_eq!(p.prev_in(Repeat::All), Some(2));
        assert_eq!(p.prev_in(Repeat::Off), None);
        assert_eq!(Repeat::Off.next().next().next(), Repeat::Off);
    }

    #[test]
    fn shuffle_keeps_the_entries_and_the_current() {
        let names: Vec<String> = (0..20).map(|i| format!("f{i}")).collect();
        let mut p = list(&names.iter().map(String::as_str).collect::<Vec<_>>());
        p.set_current(5);
        p.shuffle(12345);
        let mut items: Vec<String> = p.items().iter().map(|i| i.display().to_string()).collect();
        assert_ne!(items, names);
        assert_eq!(p.current_path(), Some(Path::new("f5")));
        assert_eq!(p.current(), Some(0));
        items.sort();
        let mut sorted = names.clone();
        sorted.sort();
        assert_eq!(items, sorted);
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
        // The first of them, already listed: its place.
        assert_eq!(p.add(vec!["b.mkv".into(), "c.mkv".into()]), Some(1));
        assert_eq!(p.len(), 3);
        assert_eq!(p.add(vec!["d.mkv".into(), "a.mkv".into()]), Some(3));
        assert_eq!(p.add(vec!["a.mkv".into()]), Some(0));
        assert_eq!(p.len(), 4);
        assert_eq!(p.add(Vec::new()), None);
        p.set_current(3);
        assert_eq!(p.next(), None);
        assert_eq!(p.prev(), Some(2));
    }

    #[test]
    fn remove_keeps_current() {
        let mut p = list(&["a", "b", "c"]);
        p.set_current(2);
        p.remove(0);
        assert_eq!(p.current_path(), Some(Path::new("c")));
        p.remove(1);
        assert_eq!(p.current(), None);
        // The playing file taken out is still the current path; it was
        // the last, so nothing comes after it.
        assert_eq!(p.current_path(), Some(Path::new("c")));
        assert_eq!(p.next(), None);
        assert_eq!(p.prev(), Some(0));
        p.remove(5);
        assert_eq!(p.len(), 1);
    }

    #[test]
    fn a_list_mpv_opened_takes_its_place() {
        let mut p = list(&["a", "https://x/list.m3u", "b"]);
        p.set_current(1);
        p.select(2);
        p.expand_current(
            vec![
                (PathBuf::from("https://x/1"), Some("One".into())),
                (PathBuf::from("a"), None),
                (PathBuf::from("https://x/2"), None),
            ],
            2,
        );
        let items: Vec<_> = p.items().iter().map(|i| i.to_string_lossy()).collect();
        assert_eq!(items, ["a", "https://x/1", "https://x/2", "b"]);
        assert_eq!(p.current_path(), Some(Path::new("https://x/2")));
        assert_eq!(p.selection(), [3]);
        assert_eq!(p.title(Path::new("https://x/1")), "One");
    }

    #[test]
    fn the_entry_after_a_removed_current_comes_next() {
        let mut p = list(&["a", "b", "c", "d"]);
        p.set_current(1);
        p.remove(1);
        assert_eq!(p.next(), Some(1));
        assert_eq!(p.items()[1], Path::new("c"));
        assert_eq!(p.prev(), Some(0));
        // Removing one before its place moves the place.
        p.remove(0);
        assert_eq!(p.items()[p.next().unwrap()], Path::new("c"));
        // Playing another entry forgets it.
        p.set_current(1);
        assert_eq!(p.current_path(), Some(Path::new("d")));
        assert_eq!(p.next(), None);
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
    fn several_selected() {
        let mut p = list(&["a", "b", "c", "d", "e"]);
        let all: Vec<usize> = (0..5).collect();
        p.select(1);
        p.select_range(3, &all);
        assert_eq!(p.selection(), [1, 2, 3]);
        // As a view shows them: a search left "c" out.
        p.select(1);
        p.select_range(3, &[0, 1, 3, 4]);
        assert_eq!(p.selection(), [1, 3]);
        // In a view's own order (groups).
        p.select(4);
        p.select_range(0, &[4, 2, 0, 1, 3]);
        assert_eq!(p.selection(), [0, 2, 4]);
        p.keep_selected(|path| path != Path::new("c"));
        assert_eq!(p.selection(), [0, 4]);
        p.select(1);
        p.select_range(3, &all);
        p.toggle_selected(2);
        assert_eq!(p.selection(), [1, 3]);
        // The selection follows its files.
        p.sort();
        p.shuffle(7);
        let names: Vec<_> = p
            .selection()
            .iter()
            .map(|i| p.items()[*i].clone())
            .collect();
        assert!(names.contains(&PathBuf::from("b")) && names.contains(&PathBuf::from("d")));
        p.sort();
        p.remove_selected();
        let items: Vec<_> = p.items().iter().map(|i| i.to_string_lossy()).collect();
        assert_eq!(items, ["a", "c", "e"]);
        // The entry in the place of the first removed is selected.
        assert_eq!(p.selection(), [1]);
        p.select_all(&[0, 1, 2]);
        assert_eq!(p.selection(), [0, 1, 2]);
        p.select_all(&[2]);
        assert_eq!(p.selection(), [2]);
    }

    #[test]
    fn selected_entries_move() {
        let mut p = list(&["a", "b", "c", "d", "e"]);
        p.set_current(3);
        p.select(0);
        p.toggle_selected(2);
        // Before "e": both after "d", in their order.
        p.move_selected(4);
        let items: Vec<_> = p.items().iter().map(|i| i.to_string_lossy()).collect();
        assert_eq!(items, ["b", "d", "a", "c", "e"]);
        assert_eq!(p.current_path(), Some(Path::new("d")));
        assert_eq!(p.selection(), [2, 3]);
        // To the top, and to the end.
        p.move_selected(0);
        assert_eq!(p.items()[0], Path::new("a"));
        p.move_selected(5);
        assert_eq!(p.items()[3..], [PathBuf::from("a"), PathBuf::from("c")]);
        assert_eq!(p.current_path(), Some(Path::new("d")));
        let before = p.revision();
        p.clear();
        assert_ne!(p.revision(), before, "a view of the old list is not taken");
    }

    #[test]
    fn folder_contents() {
        let dir = std::env::temp_dir().join(format!("metalmpv_playlist_{}", std::process::id()));
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
            names(folder_of(&dir.join("ep10.mkv"), false)),
            ["ep2.MP4", "ep10.mkv", "song.flac"]
        );
        assert_eq!(
            names(folder_of(&dir.join("notes.txt"), false)),
            ["ep2.MP4", "ep10.mkv", "notes.txt", "song.flac"]
        );
        // Opened by a name in another case: listed once, by that name.
        assert_eq!(
            names(folder_of(&dir.join("EP10.MKV"), false)),
            ["ep2.MP4", "EP10.MKV", "song.flac"]
        );
        // An album: its sound files only.
        std::fs::write(dir.join("Track 1.MP3"), b"").unwrap();
        assert_eq!(
            names(folder_of(&dir.join("song.flac"), true)),
            ["song.flac", "Track 1.MP3"]
        );
        std::fs::remove_file(dir.join("Track 1.MP3")).unwrap();
        assert!(is_audio(Path::new("a.MKA")) && !is_audio(Path::new("a.mkv")));
        let mut p = Playlist::default();
        p.replace(vec![dir.clone()]);
        assert_eq!(p.len(), 3);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
