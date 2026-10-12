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
//! the position. The most recent first, at most [`CAPACITY`]; stored as
//! lines of `seconds<TAB>audio<TAB>subtitles<TAB>path`, `-` for what is not
//! kept and `no` for a track turned off (lines of `seconds<TAB>path` are
//! read too).

use std::path::{Path, PathBuf};

/// Shorter files start from the beginning.
pub const MIN_DURATION: f64 = 300.0;
/// Positions this close to the start are not kept.
const MIN_START: f64 = 10.0;
/// Positions this close to the end are not kept: the file was watched.
const END_MARGIN: f64 = 30.0;
/// How many files are remembered.
const CAPACITY: usize = 200;

/// The tracks a file played: mpv's track ids, `None` for none (subtitles
/// off).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Tracks {
    pub audio: Option<i64>,
    pub sub: Option<i64>,
}

/// What is kept of a file.
#[derive(Clone, Debug, PartialEq)]
pub struct Left {
    pub time: Option<f64>,
    pub tracks: Option<Tracks>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Positions {
    /// The most recent first.
    list: Vec<(PathBuf, Left)>,
}

/// Paths compare as Windows does: without regard to case.
fn same(a: &Path, b: &Path) -> bool {
    a.to_string_lossy().to_lowercase() == b.to_string_lossy().to_lowercase()
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
        let time = time.filter(|(t, d)| worth_resuming(*t, *d)).map(|(t, _)| t);
        if time.is_some() || tracks.is_some() {
            self.list
                .insert(0, (path.to_path_buf(), Left { time, tracks }));
            self.list.truncate(CAPACITY);
        }
    }

    pub fn forget(&mut self, path: &Path) {
        self.list.retain(|(p, _)| !same(p, path));
    }

    pub fn clear(&mut self) {
        self.list.clear();
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
                let fields: Vec<&str> = line.splitn(4, '\t').collect();
                let (time, tracks, path) = match fields[..] {
                    [time, path] => (time, None, path),
                    [time, "-", "-", path] => (time, None, path),
                    [time, audio, sub, path] => (
                        time,
                        Some(Tracks {
                            audio: id(audio)?,
                            sub: id(sub)?,
                        }),
                        path,
                    ),
                    _ => return None,
                };
                let time = match time {
                    "-" => None,
                    t => Some(
                        t.parse::<f64>()
                            .ok()
                            .filter(|t| t.is_finite() && *t > 0.0)?,
                    ),
                };
                (!path.is_empty() && (time.is_some() || tracks.is_some()))
                    .then(|| (PathBuf::from(path), Left { time, tracks }))
            })
            .take(CAPACITY)
            .collect();
        Self { list }
    }

    pub fn to_text(&self) -> String {
        let id = |n: Option<i64>| n.map_or("no".into(), |n| n.to_string());
        self.list
            .iter()
            .map(|(p, left)| {
                let time = left.time.map_or("-".into(), |t| format!("{t:.1}"));
                let (audio, sub) = match left.tracks {
                    Some(t) => (id(t.audio), id(t.sub)),
                    None => ("-".into(), "-".into()),
                };
                format!("{time}\t{audio}\t{sub}\t{}", p.display())
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
        // Watched to the end, or barely begun: forgotten.
        p.remember(film, Some((8590.0, 8600.0)), None);
        assert_eq!(p.get(film), None);
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
        };
        // Watched to the end: the tracks stay, the position does not.
        p.remember(film, Some((8590.0, 8600.0)), Some(tracks));
        assert_eq!(
            p.get(film),
            Some(&Left {
                time: None,
                tracks: Some(tracks)
            })
        );
        let back = Positions::parse(&p.to_text());
        assert_eq!(back, p);
        assert_eq!(p.to_text(), "-\t3\tno\tD:\\Films\\Brazil.mkv");
    }

    #[test]
    fn the_latest_first_and_round_trip() {
        let mut p = Positions::default();
        for i in 0..(CAPACITY + 5) {
            let tracks = (i % 2 == 0).then_some(Tracks {
                audio: Some(2),
                sub: Some(1),
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
        // The first format, and lines that are not understood.
        assert_eq!(
            Positions::parse("x\tD:\\a.mkv\n12.5\t\n7.0\tD:\\b.mkv\n-\t-\t-\tD:\\c.mkv").list,
            vec![(
                PathBuf::from(r"D:\b.mkv"),
                Left {
                    time: Some(7.0),
                    tracks: None
                }
            )]
        );
    }
}
