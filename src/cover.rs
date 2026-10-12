//! A file's cover art: the front cover of its tags (MP3, FLAC, OGG, Opus,
//! M4A, MP4, WAV, APE and the like, read with lofty), an image attachment
//! of a Matroska file (`cover.jpg`, read here), or for an audio file a
//! picture next to it (`cover.jpg`, `folder.jpg`). mpv shows a cover only
//! in place of the video, so the app finds it itself, on a thread of its
//! own once a file is loaded.

use std::fs::File;
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};

/// Covers are shown at most this big; a larger one is scaled down.
const MAX_SIDE: u32 = 2048;
/// A picture bigger than this is not a cover.
const MAX_BYTES: u64 = 64 << 20;

/// Extensions of the audio files whose folder may hold their cover.
const AUDIO_EXTENSIONS: &[&str] = &[
    "mp3", "flac", "m4a", "m4b", "aac", "ogg", "oga", "opus", "wav", "wma", "ape", "wv", "aiff",
    "aif", "mka", "dsf", "dff", "tta", "mpc", "alac",
];

/// Names of the pictures that stand for an album's cover in its folder,
/// in the order they are looked for (mpv's `cover-art-auto` list).
const FOLDER_NAMES: &[&str] = &[
    "cover",
    "folder",
    "front",
    "albumart",
    "album",
    "albumartsmall",
    ".folder",
    "thumb",
];

/// Where a cover was found.
#[derive(Clone, Debug, PartialEq)]
pub enum Source {
    /// In the file's tags or attachments.
    Embedded,
    /// A picture next to the file.
    Folder(PathBuf),
}

/// A cover: the picture as stored and decoded.
pub struct Cover {
    /// The file's bytes, for saving it as it is.
    pub bytes: Vec<u8>,
    /// `jpg` or `png`.
    pub extension: &'static str,
    /// The picture's own size in pixels.
    pub size: [usize; 2],
    /// Scaled down to [`MAX_SIDE`] at most.
    pub image: egui::ColorImage,
    pub source: Source,
}

/// Find and decode the cover of `path` on a thread of its own; the result
/// arrives on the receiver (`None`: there is none), and `ctx` is repainted.
pub fn load(path: PathBuf, ctx: &egui::Context) -> Receiver<Option<Cover>> {
    let (tx, rx) = mpsc::channel();
    let ctx = ctx.clone();
    std::thread::spawn(move || {
        let started = std::time::Instant::now();
        let cover = find(&path);
        log::debug!(
            "cover of {}: {} in {:.0} ms",
            path.display(),
            match &cover {
                Some(c) => format!("{}x{} {}", c.size[0], c.size[1], c.extension),
                None => "none".into(),
            },
            started.elapsed().as_secs_f64() * 1000.0
        );
        if tx.send(cover).is_ok() {
            ctx.request_repaint();
        }
    });
    rx
}

/// The cover of `path`, decoded.
pub fn find(path: &Path) -> Option<Cover> {
    if let Some(bytes) = embedded(path)
        && let Some(cover) = decode(bytes, Source::Embedded)
    {
        return Some(cover);
    }
    let extension = extension(path);
    if !AUDIO_EXTENSIONS.contains(&extension.as_str()) {
        return None;
    }
    let file = in_folder(path)?;
    let bytes = std::fs::read(&file).ok()?;
    decode(bytes, Source::Folder(file))
}

fn extension(path: &Path) -> String {
    path.extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default()
}

/// The bytes of the picture stored in the file.
fn embedded(path: &Path) -> Option<Vec<u8>> {
    if matches!(extension(path).as_str(), "mkv" | "mka" | "mk3d" | "webm") {
        return matroska_cover(path);
    }
    use lofty::file::TaggedFileExt;
    use lofty::picture::PictureType;
    let tagged = lofty::probe::Probe::open(path)
        .ok()?
        .options(lofty::config::ParseOptions::new().read_properties(false))
        .read()
        .ok()?;
    let pictures: Vec<&lofty::picture::Picture> =
        tagged.tags().iter().flat_map(|t| t.pictures()).collect();
    pictures
        .iter()
        .find(|p| p.pic_type() == PictureType::CoverFront)
        .or_else(|| pictures.first())
        .map(|p| p.data().to_vec())
}

/// A cover picture in the folder of `path`.
fn in_folder(path: &Path) -> Option<PathBuf> {
    let dir = path.parent()?;
    let pictures: Vec<(String, PathBuf)> = std::fs::read_dir(dir)
        .ok()?
        .filter_map(Result::ok)
        .filter_map(|e| {
            let path = e.path();
            let ext = extension(&path);
            if !matches!(ext.as_str(), "jpg" | "jpeg" | "png") {
                return None;
            }
            let stem = path.file_stem()?.to_string_lossy().to_lowercase();
            Some((stem, path))
        })
        .collect();
    FOLDER_NAMES.iter().find_map(|name| {
        pictures
            .iter()
            .find(|(stem, _)| stem == name)
            .map(|(_, p)| p.clone())
    })
}

/// Decode a JPEG or PNG picture.
fn decode(bytes: Vec<u8>, source: Source) -> Option<Cover> {
    let format = image::guess_format(&bytes).ok()?;
    let extension = match format {
        image::ImageFormat::Jpeg => "jpg",
        image::ImageFormat::Png => "png",
        _ => return None,
    };
    let picture = image::load_from_memory_with_format(&bytes, format)
        .map_err(|e| log::warn!("cannot decode the cover: {e}"))
        .ok()?;
    let size = [picture.width() as usize, picture.height() as usize];
    let picture = if picture.width().max(picture.height()) > MAX_SIDE {
        picture.resize(MAX_SIDE, MAX_SIDE, image::imageops::FilterType::Triangle)
    } else {
        picture
    };
    let rgba = picture.to_rgba8();
    let image = egui::ColorImage::from_rgba_unmultiplied(
        [rgba.width() as usize, rgba.height() as usize],
        rgba.as_raw(),
    );
    Some(Cover {
        bytes,
        extension,
        size,
        image,
        source,
    })
}

// --- Matroska -----------------------------------------------------------
//
// Just enough of EBML to find the attachments: the Segment's SeekHead says
// where they are (they may come after the clusters), else the Segment's
// children are walked up to the first cluster.

const EBML: u32 = 0x1A45_DFA3;
const SEGMENT: u32 = 0x1853_8067;
const SEEK_HEAD: u32 = 0x114D_9B74;
const SEEK: u32 = 0x4DBB;
const SEEK_ID: u32 = 0x53AB;
const SEEK_POSITION: u32 = 0x53AC;
const ATTACHMENTS: u32 = 0x1941_A469;
const ATTACHED_FILE: u32 = 0x61A7;
const FILE_NAME: u32 = 0x466E;
const FILE_MIME_TYPE: u32 = 0x4660;
const FILE_DATA: u32 = 0x465C;
const CLUSTER: u32 = 0x1F43_B675;

/// An element's id and the size of its data (`None`: unknown).
fn element(r: &mut impl Read) -> Option<(u32, Option<u64>)> {
    let (id, _) = vint(r, true)?;
    let (size, all_ones) = vint(r, false)?;
    Some((id as u32, (!all_ones).then_some(size)))
}

/// A variable-size integer; `marker` keeps its length marker (ids do).
/// Also whether all its value bits are set (an unknown size).
fn vint(r: &mut impl Read, marker: bool) -> Option<(u64, bool)> {
    let mut first = [0u8];
    r.read_exact(&mut first).ok()?;
    let len = first[0].leading_zeros() as usize + 1;
    if len > 8 {
        return None;
    }
    let mask = if len == 8 { 0 } else { 0xFFu8 >> len };
    let mut value = u64::from(if marker { first[0] } else { first[0] & mask });
    let mut all_ones = first[0] & mask == mask;
    for _ in 1..len {
        let mut b = [0u8];
        r.read_exact(&mut b).ok()?;
        value = (value << 8) | u64::from(b[0]);
        all_ones &= b[0] == 0xFF;
    }
    Some((value, all_ones))
}

fn read_bytes(r: &mut impl Read, size: u64, limit: u64) -> Option<Vec<u8>> {
    if size > limit {
        return None;
    }
    let mut data = vec![0; size as usize];
    r.read_exact(&mut data).ok()?;
    Some(data)
}

fn read_uint(r: &mut impl Read, size: u64) -> Option<u64> {
    let data = read_bytes(r, size, 8)?;
    Some(data.iter().fold(0, |v, b| (v << 8) | u64::from(*b)))
}

/// The cover attachment of a Matroska file.
fn matroska_cover(path: &Path) -> Option<Vec<u8>> {
    let mut r = BufReader::new(File::open(path).ok()?);
    let (id, size) = element(&mut r)?;
    if id != EBML {
        return None;
    }
    r.seek_relative(size? as i64).ok()?;
    let (id, segment_size) = element(&mut r)?;
    if id != SEGMENT {
        return None;
    }
    let start = r.stream_position().ok()?;
    let end = segment_size.map(|s| start + s);
    let mut attachments_at = None;
    // The Segment's children up to the first cluster (or the attachments).
    for _ in 0..64 {
        let at = r.stream_position().ok()?;
        if end.is_some_and(|e| at >= e) {
            break;
        }
        let Some((id, Some(size))) = element(&mut r) else {
            break;
        };
        match id {
            ATTACHMENTS => return attachment(&mut r, size),
            SEEK_HEAD => {
                let data = read_bytes(&mut r, size, 1 << 20)?;
                attachments_at = attachments_at.or(seek_position(&data, ATTACHMENTS));
            }
            CLUSTER => break,
            _ => r.seek_relative(size as i64).ok()?,
        }
    }
    let position = attachments_at?;
    r.seek(SeekFrom::Start(start + position)).ok()?;
    let (id, size) = element(&mut r)?;
    (id == ATTACHMENTS).then_some(())?;
    attachment(&mut r, size?)
}

/// Where the SeekHead in `data` puts element `wanted`, from the Segment's
/// data.
fn seek_position(data: &[u8], wanted: u32) -> Option<u64> {
    let mut r = std::io::Cursor::new(data);
    while (r.position() as usize) < data.len() {
        let (id, size) = element(&mut r)?;
        let size = size?;
        if id != SEEK {
            r.seek_relative(size as i64).ok()?;
            continue;
        }
        let end = r.position() + size;
        let (mut seek_id, mut position) = (None, None);
        while r.position() < end {
            let (id, size) = element(&mut r)?;
            let size = size?;
            match id {
                SEEK_ID => seek_id = read_uint(&mut r, size),
                SEEK_POSITION => position = read_uint(&mut r, size),
                _ => r.seek_relative(size as i64).ok()?,
            }
        }
        if seek_id == Some(u64::from(wanted)) {
            return position;
        }
    }
    None
}

/// The cover among the Attachments element's files of `size` bytes: by
/// the Matroska names (`cover`, `cover_land`, `small_cover`, …), else the
/// first picture.
fn attachment(r: &mut (impl Read + Seek), size: u64) -> Option<Vec<u8>> {
    let end = r.stream_position().ok()? + size;
    let mut pictures: Vec<(String, Vec<u8>)> = Vec::new();
    while r.stream_position().ok()? < end {
        let (id, size) = element(r)?;
        let size = size?;
        if id != ATTACHED_FILE {
            r.seek_relative(size as i64).ok()?;
            continue;
        }
        let file_end = r.stream_position().ok()? + size;
        let (mut name, mut mime, mut data) = (String::new(), String::new(), None);
        while r.stream_position().ok()? < file_end {
            let (id, size) = element(r)?;
            let size = size?;
            match id {
                FILE_NAME => name = String::from_utf8_lossy(&read_bytes(r, size, 4096)?).into(),
                FILE_MIME_TYPE => {
                    mime = String::from_utf8_lossy(&read_bytes(r, size, 256)?).into();
                }
                FILE_DATA if mime.starts_with("image/") || mime.is_empty() => {
                    data = read_bytes(r, size, MAX_BYTES);
                    if data.is_none() {
                        r.seek_relative(size as i64).ok()?;
                    }
                }
                _ => r.seek_relative(size as i64).ok()?,
            }
        }
        if let Some(data) = data
            && (mime.starts_with("image/") || image::guess_format(&data).is_ok())
        {
            pictures.push((name.to_lowercase(), data));
        }
    }
    const ORDER: [&str; 4] = ["cover.", "cover_land.", "small_cover.", "small_cover_land."];
    let rank = |name: &str| {
        ORDER
            .iter()
            .position(|o| name.starts_with(o))
            .unwrap_or(ORDER.len())
    };
    pictures
        .into_iter()
        .enumerate()
        .min_by_key(|(i, (name, _))| (rank(name), *i))
        .map(|(_, (_, data))| data)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An EBML element with a size of one or two bytes.
    fn el(id: &[u8], data: &[u8]) -> Vec<u8> {
        let mut v = id.to_vec();
        if data.len() < 127 {
            v.push(0x80 | data.len() as u8);
        } else {
            assert!(data.len() < 0x3FFF);
            v.extend([0x40 | (data.len() >> 8) as u8, data.len() as u8]);
        }
        v.extend_from_slice(data);
        v
    }

    fn png() -> Vec<u8> {
        let mut bytes = Vec::new();
        image::RgbaImage::from_pixel(3, 2, image::Rgba([10, 20, 30, 255]))
            .write_to(
                &mut std::io::Cursor::new(&mut bytes),
                image::ImageFormat::Png,
            )
            .unwrap();
        bytes
    }

    fn attached(name: &str, mime: &str, data: &[u8]) -> Vec<u8> {
        let mut file = el(&[0x46, 0x6E], name.as_bytes());
        file.extend(el(&[0x46, 0x60], mime.as_bytes()));
        file.extend(el(&[0x46, 0x5C], data));
        el(&[0x61, 0xA7], &file)
    }

    #[test]
    fn vints() {
        let mut r = std::io::Cursor::new([0x81u8, 0x40, 0x02, 0xFF, 0x1A, 0x45, 0xDF, 0xA3]);
        assert_eq!(vint(&mut r, false), Some((1, false)));
        assert_eq!(vint(&mut r, false), Some((2, false)));
        assert_eq!(vint(&mut r, false), Some((0x7F, true)));
        assert_eq!(vint(&mut r, true), Some((u64::from(EBML), false)));
    }

    #[test]
    fn matroska_cover_found_by_seek_head_or_walk() {
        let picture = png();
        let mut files = attached("small_cover.png", "image/png", b"small");
        files.extend(attached("notes.txt", "text/plain", b"text"));
        files.extend(attached("cover.png", "image/png", &picture));
        let attachments = el(&[0x19, 0x41, 0xA4, 0x69], &files);
        let dir = std::env::temp_dir().join(format!("cover_test_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();

        // Attachments right in the Segment.
        let mut file = el(&[0x1A, 0x45, 0xDF, 0xA3], &[0x42, 0x86, 0x81, 0x01]);
        file.extend(el(&[0x18, 0x53, 0x80, 0x67], &attachments));
        let walked = dir.join("walked.mkv");
        std::fs::write(&walked, &file).unwrap();
        assert_eq!(matroska_cover(&walked), Some(picture.clone()));

        // Attachments after a cluster, found through the SeekHead.
        let cluster = el(&[0x1F, 0x43, 0xB6, 0x75], &[0; 20]);
        let seek_head = |position: u8| {
            let mut seek = el(&[0x53, 0xAB], &[0x19, 0x41, 0xA4, 0x69]);
            seek.extend(el(&[0x53, 0xAC], &[position]));
            el(&[0x11, 0x4D, 0x9B, 0x74], &el(&[0x4D, 0xBB], &seek))
        };
        let seek_head = seek_head((seek_head(0).len() + cluster.len()) as u8);
        let mut segment = seek_head;
        segment.extend(&cluster);
        segment.extend(&attachments);
        let mut file = el(&[0x1A, 0x45, 0xDF, 0xA3], &[0x42, 0x86, 0x81, 0x01]);
        let mut seg = vec![0x18, 0x53, 0x80, 0x67, 0x40 | (segment.len() >> 8) as u8];
        seg.push(segment.len() as u8);
        seg.extend(&segment);
        file.extend(seg);
        let sought = dir.join("sought.mkv");
        std::fs::write(&sought, &file).unwrap();
        let cover = find(&sought).expect("a cover");
        assert_eq!(cover.extension, "png");
        assert_eq!(cover.size, [3, 2]);
        assert_eq!(cover.source, Source::Embedded);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// `cargo test cover_of_sample -- --ignored --nocapture` with
    /// METALMPV_SAMPLE set: the cover a real file has.
    #[test]
    #[ignore]
    fn cover_of_sample() {
        let sample = std::env::var("METALMPV_SAMPLE").expect("METALMPV_SAMPLE is not set");
        match find(Path::new(&sample)) {
            Some(c) => println!(
                "{}x{} {} ({} bytes) {:?}",
                c.size[0],
                c.size[1],
                c.extension,
                c.bytes.len(),
                c.source
            ),
            None => println!("no cover"),
        }
    }

    #[test]
    fn folder_cover_only_for_audio() {
        let dir = std::env::temp_dir().join(format!("folder_cover_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("Folder.PNG"), png()).unwrap();
        std::fs::write(dir.join("song.mp3"), b"not really").unwrap();
        std::fs::write(dir.join("film.mp4"), b"not really").unwrap();
        let cover = find(&dir.join("song.mp3")).expect("the folder's picture");
        assert_eq!(cover.source, Source::Folder(dir.join("Folder.PNG")));
        assert!(find(&dir.join("film.mp4")).is_none());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
