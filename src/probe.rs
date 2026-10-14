//! What a file's container says that mpv does not, read on a thread of its
//! own when the information window first needs it: Matroska's statistics
//! tags per track (mkvmerge's `BPS`, `NUMBER_OF_BYTES`, `NUMBER_OF_FRAMES`;
//! mpv's own Matroska demuxer passes no track tags, so no bitrate), the
//! program that wrote the file and when, the codec level of H.264, HEVC
//! and AV1 tracks from their decoder configuration (Matroska's
//! CodecPrivate, MP4's `avcC`/`hvcC`/`av1C`; mpv shows only the profile),
//! an MP4 video track's frame count and peak bitrate (`stsz`, `btrt`), and
//! the file's own dates.

use std::fs::File;
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::ebml;
use crate::mp4;
use crate::tracks::Track;

/// What the container says.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Facts {
    /// The file the facts are of.
    pub path: PathBuf,
    /// The file's dates in the file system.
    pub created: Option<SystemTime>,
    pub modified: Option<SystemTime>,
    /// The program that wrote the file ("mkvmerge v81.0 ('…') 64-bit") and
    /// the library it wrote it with ("libebml v1.4.5 + libmatroska v1.7.1"):
    /// Matroska's WritingApp and MuxingApp.
    pub writing_app: Option<String>,
    pub muxing_app: Option<String>,
    /// When the file was written (Matroska's DateUTC).
    pub written: Option<SystemTime>,
    pub tracks: Vec<TrackFacts>,
}

/// What the container says of a track.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TrackFacts {
    /// Matroska's track number: mpv's `src-id` with its own demuxer.
    pub number: Option<i64>,
    /// The track's place among the file's tracks: FFmpeg's stream index,
    /// mpv's `ff-index`.
    pub index: i64,
    /// The codec's level, with the tier when it is High: "4.1",
    /// "5.1 High".
    pub level: Option<String>,
    /// The average bitrate in bits per second.
    pub bitrate: Option<i64>,
    /// The highest bitrate (MP4's `btrt`).
    pub max_bitrate: Option<i64>,
    /// The stream's size in bytes.
    pub bytes: Option<u64>,
    /// Frames (video), blocks (sound) or subtitle entries.
    pub frames: Option<u64>,
    /// The track's other tags, as the file names them.
    pub tags: Vec<(String, String)>,
}

impl Facts {
    /// What the container says of `track`, if anything (an external track
    /// is another file's).
    pub fn track(&self, track: &Track) -> Option<&TrackFacts> {
        if track.external {
            return None;
        }
        self.tracks.iter().find(|t| match (t.number, track.src_id) {
            (Some(n), Some(id)) => n == id,
            _ => track.ff_index == Some(t.index),
        })
    }
}

/// Read the facts of `path` on a thread of its own; they arrive on the
/// receiver, and `ctx` is repainted.
pub fn load(path: PathBuf, ctx: &egui::Context) -> Receiver<Facts> {
    let (tx, rx) = mpsc::channel();
    let ctx = ctx.clone();
    let _ = crate::worker::spawn("facts", move || {
        let started = std::time::Instant::now();
        let facts = crate::worker::guarded(|| read(&path)).unwrap_or_default();
        log::debug!(
            "container facts of {}: {} tracks in {:.0} ms",
            path.display(),
            facts.tracks.len(),
            started.elapsed().as_secs_f64() * 1000.0
        );
        if tx.send(facts).is_ok() {
            ctx.request_repaint();
        }
    });
    rx
}

/// The facts of `path`; what cannot be read is left out.
pub fn read(path: &Path) -> Facts {
    let mut facts = Facts {
        path: path.to_owned(),
        ..Facts::default()
    };
    let Ok(file) = File::open(path) else {
        return facts;
    };
    if let Ok(meta) = file.metadata() {
        facts.created = meta.created().ok();
        facts.modified = meta.modified().ok();
    }
    let len = file.metadata().map_or(0, |m| m.len());
    let mut r = BufReader::new(file);
    let mut magic = [0u8; 8];
    if r.read_exact(&mut magic).is_err() {
        return facts;
    }
    if magic[..4] == ebml::EBML.to_be_bytes() {
        matroska(&mut r, &mut facts);
    } else if matches!(
        &magic[4..],
        b"ftyp" | b"moov" | b"mdat" | b"free" | b"wide" | b"skip"
    ) {
        let _ = r.seek(SeekFrom::Start(0));
        mp4_file(&mut r, len, &mut facts);
    }
    facts
}

// --- Matroska -----------------------------------------------------------

const INFO: u32 = 0x1549_A966;
const MUXING_APP: u32 = 0x4D80;
const WRITING_APP: u32 = 0x5741;
const DATE_UTC: u32 = 0x4461;
const TRACKS: u32 = 0x1654_AE6B;
const TRACK_ENTRY: u32 = 0xAE;
const TRACK_NUMBER: u32 = 0xD7;
const TRACK_UID: u32 = 0x73C5;
const CODEC_ID: u32 = 0x86;
const CODEC_PRIVATE: u32 = 0x63A2;
const TAGS: u32 = 0x1254_C367;
const TAG: u32 = 0x7373;
const TARGETS: u32 = 0x63C0;
const TAG_TRACK_UID: u32 = 0x63C5;
const SIMPLE_TAG: u32 = 0x67C8;
const TAG_NAME: u32 = 0x45A3;
const TAG_STRING: u32 = 0x4487;

/// Matroska's dates count from 2001-01-01 UTC, in nanoseconds.
const MATROSKA_EPOCH: u64 = 978_307_200;

fn matroska(r: &mut (impl Read + Seek), facts: &mut Facts) -> Option<()> {
    let found = ebml::top_level(r, &[INFO, TRACKS, TAGS])?;
    // The tracks' UIDs, which the tags name, with their place in `facts`.
    let mut uids: Vec<(u64, usize)> = Vec::new();
    for wanted in [INFO, TRACKS, TAGS] {
        let Some(&(_, at, size)) = found.iter().find(|(id, _, _)| *id == wanted) else {
            continue;
        };
        r.seek(SeekFrom::Start(at)).ok()?;
        match wanted {
            INFO => {
                ebml::children(r, size, |r, id, size| {
                    match id {
                        MUXING_APP => facts.muxing_app = text(r, size),
                        WRITING_APP => facts.writing_app = text(r, size),
                        DATE_UTC => facts.written = ebml::read_int(r, size).and_then(date_utc),
                        _ => {}
                    }
                    Some(())
                });
            }
            TRACKS => {
                let mut index = 0;
                ebml::children(r, size, |r, id, size| {
                    if id != TRACK_ENTRY {
                        return Some(());
                    }
                    let (mut number, mut uid, mut codec, mut private) = (None, None, None, None);
                    ebml::children(r, size, |r, id, size| {
                        match id {
                            TRACK_NUMBER => number = ebml::read_uint(r, size),
                            TRACK_UID => uid = ebml::read_uint(r, size),
                            CODEC_ID => codec = ebml::read_text(r, size, 256),
                            CODEC_PRIVATE => private = ebml::read_bytes(r, size, 1 << 16),
                            _ => {}
                        }
                        Some(())
                    });
                    let config = match codec.as_deref() {
                        Some("V_MPEG4/ISO/AVC") => Some(b"avcC"),
                        Some("V_MPEGH/ISO/HEVC") => Some(b"hvcC"),
                        Some("V_AV1") => Some(b"av1C"),
                        _ => None,
                    };
                    facts.tracks.push(TrackFacts {
                        number: number.and_then(|n| i64::try_from(n).ok()),
                        index,
                        level: config.zip(private).and_then(|(c, p)| level(c, &p)),
                        ..TrackFacts::default()
                    });
                    index += 1;
                    if let Some(uid) = uid {
                        uids.push((uid, facts.tracks.len() - 1));
                    }
                    Some(())
                });
            }
            _ => {
                ebml::children(r, size, |r, id, size| {
                    if id != TAG {
                        return Some(());
                    }
                    let (targets, tags) = tag(r, size);
                    for uid in targets {
                        if let Some(&(_, i)) = uids.iter().find(|(u, _)| *u == uid) {
                            take_tags(&mut facts.tracks[i], &tags);
                        }
                    }
                    Some(())
                });
            }
        }
    }
    Some(())
}

/// A string element, if not empty.
fn text(r: &mut impl Read, size: u64) -> Option<String> {
    ebml::read_text(r, size, 4096).filter(|t| !t.is_empty())
}

/// A Matroska date: nanoseconds from 2001-01-01 UTC.
fn date_utc(ns: i64) -> Option<SystemTime> {
    let epoch = UNIX_EPOCH + Duration::from_secs(MATROSKA_EPOCH);
    let offset = Duration::from_nanos(ns.unsigned_abs());
    if ns >= 0 {
        epoch.checked_add(offset)
    } else {
        epoch.checked_sub(offset)
    }
}

/// A Tag element: the tracks it is of (their UIDs) and its simple tags.
fn tag(r: &mut (impl Read + Seek), size: u64) -> (Vec<u64>, Vec<(String, String)>) {
    let mut targets = Vec::new();
    let mut tags = Vec::new();
    ebml::children(r, size, |r, id, size| {
        match id {
            TARGETS => {
                ebml::children(r, size, |r, id, size| {
                    if id == TAG_TRACK_UID
                        && let Some(uid) = ebml::read_uint(r, size).filter(|u| *u != 0)
                    {
                        targets.push(uid);
                    }
                    Some(())
                });
            }
            SIMPLE_TAG => {
                let (mut name, mut value) = (None, None);
                ebml::children(r, size, |r, id, size| {
                    match id {
                        TAG_NAME => name = text(r, size),
                        TAG_STRING => value = text(r, size),
                        _ => {}
                    }
                    Some(())
                });
                if let (Some(name), Some(value)) = (name, value) {
                    tags.push((name, value));
                }
            }
            _ => {}
        }
        Some(())
    });
    (targets, tags)
}

/// A track's tags into its facts: the statistics as numbers, the rest as
/// they are, less what says nothing (the duration, which the file has,
/// FFmpeg's handler names and empty vendor).
fn take_tags(track: &mut TrackFacts, tags: &[(String, String)]) {
    for (name, value) in tags {
        // FFmpeg, remuxing, appends the language: "BPS-eng".
        let base = name.split('-').next().unwrap_or(name).to_ascii_uppercase();
        match base.as_str() {
            "BPS" => track.bitrate = value.trim().parse().ok().filter(|b| *b > 0),
            "NUMBER_OF_BYTES" => track.bytes = value.trim().parse().ok().filter(|b| *b > 0),
            "NUMBER_OF_FRAMES" => track.frames = value.trim().parse().ok().filter(|f| *f > 0),
            "DURATION" | "LANGUAGE" => {}
            "HANDLER_NAME" if value.ends_with("Handler") => {}
            "VENDOR_ID" if value.trim_matches(|c| "[0]".contains(c)).is_empty() => {}
            b if b.starts_with("_STATISTICS_") => {}
            _ => track.tags.push((name.clone(), value.clone())),
        }
    }
}

// --- MP4 ----------------------------------------------------------------

/// The size of a visual sample entry before its boxes (ISO 14496-12).
const VISUAL_ENTRY: u64 = 78;

fn mp4_file(r: &mut (impl Read + Seek), len: u64, facts: &mut Facts) -> Option<()> {
    let moov_end = mp4::child(r, len, b"moov")?;
    let mut index = 0;
    mp4::each_box(r, moov_end, |r, kind, end| {
        if &kind != b"trak" {
            return Some(());
        }
        let mut track = TrackFacts {
            index,
            ..TrackFacts::default()
        };
        index += 1;
        let _ = trak(r, end, &mut track);
        if track.level.is_some() || track.frames.is_some() || track.max_bitrate.is_some() {
            facts.tracks.push(track);
        }
        Some(())
    });
    Some(())
}

/// A video `trak`'s level, peak bitrate and frame count.
fn trak(r: &mut (impl Read + Seek), end: u64, track: &mut TrackFacts) -> Option<()> {
    let mdia_end = mp4::child(r, end, b"mdia")?;
    let mdia = r.stream_position().ok()?;
    // hdlr: version and flags, pre_defined, the handler's type.
    mp4::child(r, mdia_end, b"hdlr")?;
    let mut hdlr = [0u8; 12];
    r.read_exact(&mut hdlr).ok()?;
    if &hdlr[8..] != b"vide" {
        return None;
    }
    r.seek(SeekFrom::Start(mdia)).ok()?;
    let stbl_end = mp4::path(r, mdia_end, &[b"minf", b"stbl"])?;
    let stbl = r.stream_position().ok()?;
    // The first sample description: a visual entry, its boxes after it.
    if let Some(stsd_end) = mp4::child(r, stbl_end, b"stsd") {
        r.seek_relative(8).ok()?;
        if let Some((_, size)) = mp4::next_box(r, stsd_end) {
            let entry = r.stream_position().ok()?;
            let entry_end = entry + size;
            if size > VISUAL_ENTRY {
                r.seek(SeekFrom::Start(entry + VISUAL_ENTRY)).ok()?;
                mp4::each_box(r, entry_end, |r, kind, end| {
                    let size = end - r.stream_position().ok()?;
                    match &kind {
                        b"avcC" | b"hvcC" | b"av1C" => {
                            if let Some(data) = ebml::read_bytes(r, size.min(64), 64) {
                                track.level = level(&kind, &data);
                            }
                        }
                        b"btrt" if size >= 12 => {
                            let mut b = [0u8; 12];
                            r.read_exact(&mut b).ok()?;
                            let max = u32::from_be_bytes([b[4], b[5], b[6], b[7]]);
                            track.max_bitrate = (max > 0).then_some(i64::from(max));
                        }
                        _ => {}
                    }
                    Some(())
                });
            }
        }
    }
    // The samples: the frames of a video track.
    r.seek(SeekFrom::Start(stbl)).ok()?;
    if mp4::child(r, stbl_end, b"stsz").is_some() {
        let mut b = [0u8; 12];
        r.read_exact(&mut b).ok()?;
        let count = u32::from_be_bytes([b[8], b[9], b[10], b[11]]);
        track.frames = (count > 0).then_some(u64::from(count));
    }
    Some(())
}

// --- Levels -------------------------------------------------------------

/// The level (and the tier when High) of a decoder configuration: H.264's
/// `avcC`, HEVC's `hvcC`, AV1's `av1C`.
fn level(kind: &[u8; 4], config: &[u8]) -> Option<String> {
    match kind {
        b"avcC" => {
            // version, profile, constraint flags, level.
            if config.len() < 4 || config[0] != 1 {
                return None;
            }
            let (profile, flags, level) = (config[1], config[2], config[3]);
            // Level 1b: 11 with constraint_set3 in Baseline, Main,
            // Extended; or 9.
            if level == 9 || (level == 11 && flags & 0x10 != 0 && matches!(profile, 66 | 77 | 88)) {
                return Some("1b".into());
            }
            (level > 0).then(|| tenths(u32::from(level)))
        }
        b"hvcC" => {
            // version, then space(2) tier(1) profile(5), 4 bytes of
            // compatibility, 6 of constraints, the level times 30.
            if config.len() < 13 || config[0] != 1 {
                return None;
            }
            let high = config[1] & 0x20 != 0;
            let level = u32::from(config[12]);
            (level > 0).then(|| tiered(tenths(level / 3), high))
        }
        b"av1C" => {
            // marker(1) version(7), profile(3) level index(5), tier(1)…
            if config.len() < 3 || config[0] & 0x7F != 1 {
                return None;
            }
            let index = u32::from(config[1] & 0x1F);
            if index == 31 {
                return None;
            }
            let high = config[2] & 0x80 != 0;
            let text = format!("{}.{}", 2 + (index >> 2), index & 3);
            Some(tiered(text, high))
        }
        _ => None,
    }
}

/// A level in tenths as written: 41 → "4.1", 50 → "5".
fn tenths(level: u32) -> String {
    match level % 10 {
        0 => (level / 10).to_string(),
        n => format!("{}.{n}", level / 10),
    }
}

fn tiered(level: String, high: bool) -> String {
    if high { format!("{level} High") } else { level }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ebml::tests::el;
    use crate::mp4::tests::bx;

    #[test]
    fn levels() {
        assert_eq!(level(b"avcC", &[1, 100, 0, 41]).as_deref(), Some("4.1"));
        assert_eq!(level(b"avcC", &[1, 100, 0, 50]).as_deref(), Some("5"));
        assert_eq!(level(b"avcC", &[1, 66, 0x10, 11]).as_deref(), Some("1b"));
        assert_eq!(level(b"avcC", &[1, 100, 0x10, 11]).as_deref(), Some("1.1"));
        let mut hvcc = [0u8; 23];
        hvcc[0] = 1;
        hvcc[1] = 0x02;
        hvcc[12] = 153;
        assert_eq!(level(b"hvcC", &hvcc).as_deref(), Some("5.1"));
        hvcc[1] = 0x22;
        hvcc[12] = 120;
        assert_eq!(level(b"hvcC", &hvcc).as_deref(), Some("4 High"));
        assert_eq!(level(b"av1C", &[0x81, 0x09, 0x0C]).as_deref(), Some("4.1"));
        assert_eq!(
            level(b"av1C", &[0x81, 0x0C, 0x8C]).as_deref(),
            Some("5.0 High")
        );
        assert_eq!(level(b"avcC", &[0, 100, 0, 41]), None);
    }

    fn uint(id: &[u8], value: u64) -> Vec<u8> {
        el(id, &value.to_be_bytes())
    }

    #[test]
    fn matroska_statistics_and_levels() {
        let mut avcc = vec![1, 100, 0, 40];
        avcc.extend([0xFF; 4]);
        let mut video = uint(&[0xD7], 1);
        video.extend(uint(&[0x73, 0xC5], 0xABCD));
        video.extend(el(&[0x86], b"V_MPEG4/ISO/AVC"));
        video.extend(el(&[0x63, 0xA2], &avcc));
        let mut audio = uint(&[0xD7], 2);
        audio.extend(uint(&[0x73, 0xC5], 0x1234));
        audio.extend(el(&[0x86], b"A_AC3"));
        let mut tracks = el(&[0xAE], &video);
        tracks.extend(el(&[0xAE], &audio));

        let simple = |name: &str, value: &str| {
            let mut s = el(&[0x45, 0xA3], name.as_bytes());
            s.extend(el(&[0x44, 0x87], value.as_bytes()));
            el(&[0x67, 0xC8], &s)
        };
        let mut tag = el(&[0x63, 0xC0], &uint(&[0x63, 0xC5], 0x1234));
        tag.extend(simple("BPS", "640000"));
        tag.extend(simple("NUMBER_OF_FRAMES", "224891"));
        tag.extend(simple("NUMBER_OF_BYTES", "575720960"));
        tag.extend(simple("DURATION", "01:59:55.360000000"));
        tag.extend(simple("_STATISTICS_WRITING_APP", "mkvmerge"));
        tag.extend(simple("ENCODER", "Lavc aac"));
        let tags = el(&[0x73, 0x73], &tag);

        let mut info = el(&[0x4D, 0x80], b"libebml v1.4.5 + libmatroska v1.7.1");
        info.extend(el(&[0x57, 0x41], b"mkvmerge v81.0 ('Close Enough') 64-bit"));
        info.extend(el(&[0x44, 0x61], &(86_400_000_000_000i64).to_be_bytes()));

        let mut segment = el(&[0x15, 0x49, 0xA9, 0x66], &info);
        segment.extend(el(&[0x16, 0x54, 0xAE, 0x6B], &tracks));
        segment.extend(el(&[0x12, 0x54, 0xC3, 0x67], &tags));
        let mut file = el(&[0x1A, 0x45, 0xDF, 0xA3], &[0x42, 0x86, 0x81, 0x01]);
        file.extend(el(&[0x18, 0x53, 0x80, 0x67], &segment));

        let dir = std::env::temp_dir().join(format!("probe_mkv_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("film.mkv");
        std::fs::write(&path, &file).unwrap();
        let facts = read(&path);
        std::fs::remove_dir_all(&dir).unwrap();

        assert_eq!(
            facts.writing_app.as_deref(),
            Some("mkvmerge v81.0 ('Close Enough') 64-bit")
        );
        assert!(facts.muxing_app.is_some() && facts.modified.is_some());
        assert_eq!(
            facts.written,
            Some(UNIX_EPOCH + Duration::from_secs(MATROSKA_EPOCH + 86_400))
        );
        assert_eq!(facts.tracks.len(), 2);
        assert_eq!(facts.tracks[0].level.as_deref(), Some("4"));
        let audio = &facts.tracks[1];
        assert_eq!((audio.number, audio.index), (Some(2), 1));
        assert_eq!(audio.bitrate, Some(640_000));
        assert_eq!(audio.frames, Some(224_891));
        assert_eq!(audio.bytes, Some(575_720_960));
        assert_eq!(audio.tags, [("ENCODER".to_owned(), "Lavc aac".to_owned())]);

        // mpv's own demuxer gives the number, FFmpeg's the index.
        let track = Track {
            src_id: Some(2),
            ff_index: Some(0),
            ..Track::default()
        };
        assert_eq!(facts.track(&track), Some(audio));
        let track = Track {
            ff_index: Some(1),
            ..Track::default()
        };
        assert_eq!(facts.track(&track), Some(audio));
    }

    #[test]
    fn mp4_level_frames_and_peak() {
        let mut hvcc = vec![0u8; 23];
        hvcc[0] = 1;
        hvcc[12] = 150;
        let mut btrt = vec![0u8; 4];
        btrt.extend(20_000_000u32.to_be_bytes());
        btrt.extend(8_000_000u32.to_be_bytes());
        let mut entry = vec![0u8; VISUAL_ENTRY as usize];
        entry.extend(bx(b"hvcC", &hvcc));
        entry.extend(bx(b"btrt", &btrt));
        let mut stsd = vec![0, 0, 0, 0, 0, 0, 0, 1];
        stsd.extend(bx(b"hvc1", &entry));
        let mut stsz = vec![0u8; 8];
        stsz.extend(1440u32.to_be_bytes());
        let mut stbl = bx(b"stsd", &stsd);
        stbl.extend(bx(b"stsz", &stsz));
        let mut hdlr = vec![0u8; 8];
        hdlr.extend(b"vide");
        hdlr.extend([0u8; 13]);
        let mut mdia = bx(b"hdlr", &hdlr);
        mdia.extend(bx(b"minf", &bx(b"stbl", &stbl)));
        let video = bx(b"trak", &bx(b"mdia", &mdia));
        let mut sound_hdlr = vec![0u8; 8];
        sound_hdlr.extend(b"soun");
        sound_hdlr.extend([0u8; 13]);
        let sound = bx(b"trak", &bx(b"mdia", &bx(b"hdlr", &sound_hdlr)));
        let mut moov = sound;
        moov.extend(video);
        let mut file = bx(b"ftyp", b"isom\0\0\x02\0isom");
        file.extend(bx(b"moov", &moov));

        let dir = std::env::temp_dir().join(format!("probe_mp4_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("film.mp4");
        std::fs::write(&path, &file).unwrap();
        let facts = read(&path);
        std::fs::remove_dir_all(&dir).unwrap();

        assert_eq!(facts.tracks.len(), 1);
        let video = &facts.tracks[0];
        assert_eq!(video.index, 1);
        assert_eq!(video.level.as_deref(), Some("5"));
        assert_eq!(video.max_bitrate, Some(20_000_000));
        assert_eq!(video.frames, Some(1440));
    }

    /// `cargo test facts_of_sample -- --ignored --nocapture` with
    /// METALMPV_SAMPLE set: what a real file's container says.
    #[test]
    #[ignore]
    fn facts_of_sample() {
        let sample = std::env::var("METALMPV_SAMPLE").expect("METALMPV_SAMPLE is not set");
        println!("{:#?}", read(Path::new(&sample)));
    }
}
