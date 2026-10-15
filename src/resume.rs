//! Where each file was left and which tracks it played, to go on from
//! there the next time it is opened (the `resume_playback` and
//! `remember_tracks` settings). The app's own list rather than mpv's
//! watch-later files, which would also bring back the volume and must be
//! written by a command before every file change.
//!
//! A position is kept only for files of [`MIN_DURATION`] or more (a song
//! starts from the beginning), and only between [`MIN_START`] after the
//! start and [`END_MARGIN`] before the end: a file watched to its end, or
//! barely begun, starts from the beginning. The tracks are kept whatever
//! the position. A file of [`MIN_DURATION`] or more left within
//! [`END_MARGIN`] of its end is marked watched (the playlist shows it); the
//! duration is kept with a position (the playlist shows how far it went).
//! The most recent first, at most [`CAPACITY`]; stored as lines of
//! `seconds<TAB>audio<TAB>subtitles<TAB>duration<TAB>path`, `end` for the
//! seconds of a file watched, `-` for what is not kept and `no` for a track
//! turned off, `@` and its path for subtitles of a file of their own
//! (lines of `seconds<TAB>audio<TAB>subtitles<TAB>path` and
//! `seconds<TAB>path` are read too).

use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Shorter files start from the beginning.
pub const MIN_DURATION: f64 = 300.0;
/// Positions this close to the start are not kept.
const MIN_START: f64 = 10.0;
/// Positions this close to the end are not kept: the file was watched.
const END_MARGIN: f64 = 30.0;
/// How many files are remembered.
const CAPACITY: usize = 500;

/// The tracks a file played: mpv's track ids, `None` for none (subtitles
/// off); subtitles of a file of their own by that file (`sub_file`, `sub`
/// then `None`): their id is their place among the files loaded, which
/// another subtitle file in the folder changes, and one loaded by hand is
/// not there the next time.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tracks {
    pub audio: Option<i64>,
    pub sub: Option<i64>,
    pub sub_file: Option<PathBuf>,
}

/// What is kept of a file.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Left {
    pub time: Option<f64>,
    /// The file's duration, kept with the time.
    pub duration: Option<f64>,
    /// Left at its end.
    pub watched: bool,
    pub tracks: Option<Tracks>,
}

impl Left {
    /// How far the file went, 0 to 1, when its position is kept.
    pub fn progress(&self) -> Option<f32> {
        let (time, duration) = (self.time?, self.duration?);
        (duration > 0.0).then(|| (time / duration).clamp(0.0, 1.0) as f32)
    }
}

#[derive(Clone, Debug, Default)]
pub struct Positions {
    /// The most recent first.
    list: Vec<(PathBuf, Left)>,
    /// Counts the changes, for views made of the list (the playlist's).
    revision: u64,
}

impl PartialEq for Positions {
    fn eq(&self, other: &Self) -> bool {
        self.list == other.list
    }
}

/// A path as Windows compares it: without regard to case.
pub fn key(path: &Path) -> String {
    path.to_string_lossy().to_lowercase()
}

fn same(a: &Path, b: &Path) -> bool {
    key(a) == key(b)
}

/// Whether `time` of `duration` is the end of something watched.
fn watched_to_end(time: f64, duration: f64) -> bool {
    duration >= MIN_DURATION && time >= duration - END_MARGIN && time.is_finite()
}

/// Whether `time` of `duration` is worth going on from.
fn worth_resuming(time: f64, duration: f64) -> bool {
    duration >= MIN_DURATION
        && time >= MIN_START
        && time < duration - END_MARGIN
        && time.is_finite()
}

impl Positions {
    /// What is kept of `path`, if anything.
    pub fn get(&self, path: &Path) -> Option<&Left> {
        self.list
            .iter()
            .find(|(p, _)| same(p, path))
            .map(|(_, left)| left)
    }

    /// `path` was left at `time` of `duration` (`None`: positions are not
    /// kept) playing `tracks` (`None`: tracks are not kept). What is worth
    /// keeping is kept, the rest of the file forgotten.
    pub fn remember(&mut self, path: &Path, time: Option<(f64, f64)>, tracks: Option<Tracks>) {
        self.forget(path);
        let watched = time.is_some_and(|(t, d)| watched_to_end(t, d));
        let time = time.filter(|(t, d)| worth_resuming(*t, *d));
        if time.is_some() || tracks.is_some() || watched {
            let left = Left {
                time: time.map(|(t, _)| t),
                duration: time.map(|(_, d)| d),
                watched,
                tracks,
            };
            self.list.insert(0, (path.to_path_buf(), left));
            self.list.truncate(CAPACITY);
        }
    }

    pub fn forget(&mut self, path: &Path) {
        self.list.retain(|(p, _)| !same(p, path));
        self.revision = self.revision.wrapping_add(1);
    }

    pub fn clear(&mut self) {
        self.list.clear();
        self.revision = self.revision.wrapping_add(1);
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// What is kept of each file by its [`key`], to look many up.
    pub fn by_key(&self) -> HashMap<String, Left> {
        // The most recent last, so that it wins over an older line.
        self.list
            .iter()
            .rev()
            .map(|(p, left)| (key(p), left.clone()))
            .collect()
    }

    /// The stored lines; lines that are not understood are skipped.
    pub fn parse(text: &str) -> Self {
        let id = |s: &str| match s {
            "no" => Some(None),
            s => s.parse::<i64>().ok().filter(|n| *n > 0).map(Some),
        };
        let list = text
            .lines()
            .filter_map(|line| {
                let fields: Vec<&str> = line.splitn(5, '\t').collect();
                let (time, tracks, duration, path) = match fields[..] {
                    [time, path] => (time, ("-", "-"), "-", path),
                    [time, audio, sub, path] => (time, (audio, sub), "-", path),
                    [time, audio, sub, duration, path] => (time, (audio, sub), duration, path),
                    _ => return None,
                };
                let tracks = match tracks {
                    ("-", "-") => None,
                    (audio, sub) => Some(match sub.strip_prefix('@') {
                        Some(file) if !file.is_empty() => Tracks {
                            audio: id(audio)?,
                            sub: None,
                            sub_file: Some(PathBuf::from(file)),
                        },
                        _ => Tracks {
                            audio: id(audio)?,
                            sub: id(sub)?,
                            sub_file: None,
                        },
                    }),
                };
                let seconds = |s: &str| match s {
                    "-" => Some(None),
                    s => s
                        .parse::<f64>()
                        .ok()
                        .filter(|t| t.is_finite() && *t > 0.0)
                        .map(Some),
                };
                let watched = time == "end";
                let time = if watched { None } else { seconds(time)? };
                let duration = time.and(seconds(duration)?);
                (!path.is_empty() && (time.is_some() || tracks.is_some() || watched)).then(|| {
                    let left = Left {
                        time,
                        duration,
                        watched,
                        tracks,
                    };
                    (PathBuf::from(path), left)
                })
            })
            .take(CAPACITY)
            .collect();
        Self { list, revision: 0 }
    }

    pub fn to_text(&self) -> String {
        let id = |n: Option<i64>| n.map_or("no".into(), |n| n.to_string());
        self.list
            .iter()
            .map(|(p, left)| {
                let time = match left.time {
                    Some(t) => format!("{t:.1}"),
                    None if left.watched => "end".into(),
                    None => "-".into(),
                };
                let duration = left.duration.map_or("-".into(), |d| format!("{d:.1}"));
                let (audio, sub) = match &left.tracks {
                    Some(t) => match &t.sub_file {
                        Some(file) => (id(t.audio), format!("@{}", file.display())),
                        None => (id(t.audio), id(t.sub)),
                    },
                    None => ("-".into(), "-".into()),
                };
                format!("{time}\t{audio}\t{sub}\t{duration}\t{}", p.display())
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn time_of(p: &Positions, path: &str) -> Option<f64> {
        p.get(Path::new(path)).and_then(|l| l.time)
    }

    #[test]
    fn remembers_what_is_worth_it() {
        let mut p = Positions::default();
        let film = Path::new(r"D:\Films\Brazil.mkv");
        p.remember(film, Some((1234.5, 8600.0)), None);
        assert_eq!(time_of(&p, r"D:\Films\Brazil.mkv"), Some(1234.5));
        // Any case of the same path.
        assert_eq!(time_of(&p, r"d:\films\BRAZIL.MKV"), Some(1234.5));
        assert_eq!(
            p.get(film).and_then(Left::progress),
            Some((1234.5 / 8600.0) as f32)
        );
        // Watched to the end: marked, starting from the beginning.
        p.remember(film, Some((8590.0, 8600.0)), None);
        let left = p.get(film).unwrap();
        assert!(left.watched && left.time.is_none());
        // Barely begun: forgotten.
        p.remember(film, Some((5.0, 8600.0)), None);
        assert_eq!(p.get(film), None);
        // A song: never kept.
        let song = Path::new(r"D:\Music\song.mp3");
        p.remember(song, Some((120.0, 240.0)), None);
        assert_eq!(p.get(song), None);
    }

    #[test]
    fn tracks_are_kept_whatever_the_position() {
        let mut p = Positions::default();
        let film = Path::new(r"D:\Films\Brazil.mkv");
        let tracks = Tracks {
            audio: Some(3),
            sub: None,
            sub_file: None,
        };
        // Watched to the end: the tracks stay, the position does not.
        p.remember(film, Some((8590.0, 8600.0)), Some(tracks.clone()));
        assert_eq!(
            p.get(film),
            Some(&Left {
                watched: true,
                tracks: Some(tracks),
                ..Left::default()
            })
        );
        let back = Positions::parse(&p.to_text());
        assert_eq!(back, p);
        assert_eq!(p.to_text(), "end\t3\tno\t-\tD:\\Films\\Brazil.mkv");
        // Subtitles of a file of their own, by that file.
        let srt = Tracks {
            audio: Some(1),
            sub: None,
            sub_file: Some(PathBuf::from(r"D:\Films\Subs\Brazil.en.srt")),
        };
        p.remember(film, Some((600.0, 8600.0)), Some(srt.clone()));
        assert_eq!(
            p.to_text(),
            "600.0\t1\t@D:\\Films\\Subs\\Brazil.en.srt\t8600.0\tD:\\Films\\Brazil.mkv"
        );
        let back = Positions::parse(&p.to_text());
        assert_eq!(back.get(film).unwrap().tracks, Some(srt));
    }

    #[test]
    fn the_latest_first_and_round_trip() {
        let mut p = Positions::default();
        for i in 0..(CAPACITY + 5) {
            let tracks = (i % 2 == 0).then_some(Tracks {
                audio: Some(2),
                sub: Some(1),
                sub_file: None,
            });
            p.remember(
                Path::new(&format!(r"D:\{i}.mkv")),
                Some((100.0 + i as f64, 3600.0)),
                tracks,
            );
        }
        assert_eq!(p.list.len(), CAPACITY);
        assert_eq!(
            p.list[0].0,
            PathBuf::from(format!(r"D:\{}.mkv", CAPACITY + 4))
        );
        // The oldest are dropped.
        assert_eq!(p.get(Path::new(r"D:\0.mkv")), None);
        let back = Positions::parse(&p.to_text());
        assert_eq!(back, p);
        assert_eq!(time_of(&back, r"D:\100.mkv"), Some(200.0));
        assert_eq!(
            back.get(Path::new(r"D:\100.mkv")).unwrap().duration,
            Some(3600.0)
        );
        // The older formats, and lines that are not understood.
        let old = "x\tD:\\a.mkv\n12.5\t\n7.0\tD:\\b.mkv\n-\t-\t-\tD:\\c.mkv\n9.0\t2\tno\tD:\\d.mkv";
        assert_eq!(
            Positions::parse(old).list,
            vec![
                (
                    PathBuf::from(r"D:\b.mkv"),
                    Left {
                        time: Some(7.0),
                        ..Left::default()
                    }
                ),
                (
                    PathBuf::from(r"D:\d.mkv"),
                    Left {
                        time: Some(9.0),
                        tracks: Some(Tracks {
                            audio: Some(2),
                            sub: None,
                            sub_file: None,
                        }),
                        ..Left::default()
                    }
                )
            ]
        );
    }
}
