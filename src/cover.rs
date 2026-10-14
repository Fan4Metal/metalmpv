//! A file's cover art: the front cover of its tags (MP3, FLAC, OGG, Opus,
//! WAV, APE and the like, read with lofty; MP4's `covr` read here), an
//! image attachment of a Matroska file (`cover.jpg`, read here), or for an
//! audio file a picture next to it (`cover.jpg`, `folder.jpg`). mpv shows a
//! cover only in place of the video, so the app finds it itself, on a
//! thread of its own once a file is loaded.

use std::fs::File;
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};

use crate::ebml;
use crate::mp4::child as mp4_child;

/// Covers are shown at most this big; a larger one is scaled down.
const MAX_SIDE: u32 = 2048;
/// A picture bigger than this is not a cover.
const MAX_BYTES: u64 = 64 << 20;
/// The largest picture decoded, a side and its pixels in memory.
const MAX_DECODED_SIDE: u32 = 16384;
const MAX_DECODED_BYTES: u64 = 256 << 20;

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
    let _ = crate::worker::spawn("cover", move || {
        let started = std::time::Instant::now();
        let cover = crate::worker::guarded(|| find(&path)).flatten();
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
    match extension(path).as_str() {
        "mkv" | "mka" | "mk3d" | "webm" => return matroska_cover(path),
        "mp4" | "m4v" | "m4a" | "m4b" | "m4p" | "mov" | "3gp" | "3g2" | "f4v" => {
            return mp4_cover(path);
        }
        _ => {}
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
    // Limits as a picture of a downloaded file may be a bomb: 64 MB of PNG
    // could ask for gigabytes.
    let mut reader = image::ImageReader::with_format(std::io::Cursor::new(&bytes), format);
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(MAX_DECODED_SIDE);
    limits.max_image_height = Some(MAX_DECODED_SIDE);
    limits.max_alloc = Some(MAX_DECODED_BYTES);
    reader.limits(limits);
    let picture = reader
        .decode()
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

// --- MP4 ----------------------------------------------------------------
//
// The cover is in moov/udta/meta/ilst/covr (iTunes' tags).
/// The cover of an MP4 (or QuickTime) file.
fn mp4_cover(path: &Path) -> Option<Vec<u8>> {
    let file = File::open(path).ok()?;
    let len = file.metadata().ok()?.len();
    let mut r = BufReader::new(file);
    let moov_end = mp4_child(&mut r, len, b"moov")?;
    let moov = r.stream_position().ok()?;
    // iTunes puts the tags in moov/udta/meta, some writers in moov/meta.
    let meta_end = match mp4_child(&mut r, moov_end, b"udta")
        .and_then(|udta_end| mp4_child(&mut r, udta_end, b"meta"))
    {
        Some(end) => end,
        None => {
            r.seek(SeekFrom::Start(moov)).ok()?;
            mp4_child(&mut r, moov_end, b"meta")?
        }
    };
    // ISO's meta starts with a version and flags, QuickTime's with a box.
    let meta = r.stream_position().ok()?;
    let mut peek = [0u8; 8];
    r.read_exact(&mut peek).ok()?;
    let children = if &peek[4..] == b"hdlr" {
        meta
    } else {
        meta + 4
    };
    r.seek(SeekFrom::Start(children)).ok()?;
    let ilst_end = mp4_child(&mut r, meta_end, b"ilst")?;
    let covr_end = mp4_child(&mut r, ilst_end, b"covr")?;
    // Its data boxes: a type (13 JPEG, 14 PNG), a locale, the picture; the
    // first that can be shown (a BMP or GIF, or one too large, is passed).
    loop {
        let data_end = mp4_child(&mut r, covr_end, b"data")?;
        let size = data_end - r.stream_position().ok()?;
        if size > 8 {
            r.seek_relative(8).ok()?;
            if let Some(bytes) = ebml::read_bytes(&mut r, size - 8, MAX_BYTES)
                && shown(&bytes)
            {
                return Some(bytes);
            }
        }
        r.seek(SeekFrom::Start(data_end)).ok()?;
    }
}

/// Whether `bytes` are a picture `decode` takes: JPEG or PNG.
fn shown(bytes: &[u8]) -> bool {
    matches!(
        image::guess_format(bytes),
        Ok(image::ImageFormat::Jpeg | image::ImageFormat::Png)
    )
}

// --- Matroska -----------------------------------------------------------

const ATTACHMENTS: u32 = 0x1941_A469;
const ATTACHED_FILE: u32 = 0x61A7;
const FILE_NAME: u32 = 0x466E;
const FILE_MIME_TYPE: u32 = 0x4660;
const FILE_DATA: u32 = 0x465C;

/// The cover attachment of a Matroska file.
fn matroska_cover(path: &Path) -> Option<Vec<u8>> {
    let mut r = BufReader::new(File::open(path).ok()?);
    let found = ebml::top_level(&mut r, &[ATTACHMENTS])?;
    let (_, at, size) = found.first()?;
    r.seek(SeekFrom::Start(*at)).ok()?;
    attachment(&mut r, *size)
}

/// The cover among the Attachments element's files of `size` bytes: by
/// the Matroska names (`cover`, `cover_land`, `small_cover`, …), else the
/// first picture.
fn attachment(r: &mut (impl Read + Seek), size: u64) -> Option<Vec<u8>> {
    let end = r.stream_position().ok()?.checked_add(size)?;
    const ORDER: [&str; 4] = ["cover.", "cover_land.", "small_cover.", "small_cover_land."];
    let rank = |name: &str| {
        ORDER
            .iter()
            .position(|o| name.starts_with(o))
            .unwrap_or(ORDER.len())
    };
    // Only the best picture so far is kept: a file may carry many large
    // ones. The name comes before the data in a Matroska file.
    let mut best: Option<(usize, Vec<u8>)> = None;
    while r.stream_position().ok()? < end {
        let (id, size) = ebml::element(r)?;
        let size = size?;
        if id != ATTACHED_FILE {
            r.seek_relative(size as i64).ok()?;
            continue;
        }
        let file_end = r.stream_position().ok()?.checked_add(size)?;
        let (mut name, mut mime, mut data) = (String::new(), String::new(), None);
        while r.stream_position().ok()? < file_end {
            let (id, size) = ebml::element(r)?;
            let size = size?;
            match id {
                FILE_NAME => {
                    name = String::from_utf8_lossy(&ebml::read_bytes(r, size, 4096)?).into()
                }
                FILE_MIME_TYPE => {
                    mime = String::from_utf8_lossy(&ebml::read_bytes(r, size, 256)?).into();
                }
                // The name usually comes first; when it does not, the data
                // is read to be ranked after.
                FILE_DATA
                    if (mime.starts_with("image/") || mime.is_empty())
                        && (name.is_empty()
                            || best
                                .as_ref()
                                .is_none_or(|(b, _)| rank(&name.to_lowercase()) < *b)) =>
                {
                    data = ebml::read_bytes(r, size, MAX_BYTES);
                    if data.is_none() {
                        r.seek_relative(size as i64).ok()?;
                    }
                }
                _ => r.seek_relative(size as i64).ok()?,
            }
        }
        let rank = rank(&name.to_lowercase());
        // Only a picture that can be shown: a WebP cover would have taken
        // the place of a small JPEG one and shown nothing.
        if let Some(data) = data
            && shown(&data)
            && best.as_ref().is_none_or(|(b, _)| rank < *b)
        {
            best = Some((rank, data));
        }
    }
    best.map(|(_, data)| data)
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::ebml::tests::el;
    use crate::mp4::tests::bx;

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

    #[test]
    fn a_cover_that_cannot_be_shown_is_passed() {
        let picture = png();
        let mut files = attached("cover.webp", "image/webp", b"RIFF\0\0\0\0WEBPVP8 ");
        files.extend(attached("small_cover.png", "image/png", &picture));
        let mut r = std::io::Cursor::new(&files);
        assert_eq!(attachment(&mut r, files.len() as u64), Some(picture));
    }

    #[test]
    fn mp4_cover_after_a_64_bit_mdat() {
        let picture = png();
        let mut covr_data = 14u32.to_be_bytes().to_vec();
        covr_data.extend([0; 4]);
        covr_data.extend(&picture);
        let mut ilst = bx(b"\xA9nam", &bx(b"data", b"\0\0\0\x01\0\0\0\0Name"));
        ilst.extend(bx(b"covr", &bx(b"data", &covr_data)));
        let hdlr = bx(b"hdlr", &[0; 25]);
        let meta = |full: bool| {
            let mut data = if full { vec![0; 4] } else { Vec::new() };
            data.extend(&hdlr);
            data.extend(bx(b"ilst", &ilst));
            bx(b"meta", &data)
        };
        let dir = std::env::temp_dir().join(format!("mp4_cover_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        for (name, moov) in [
            ("itunes.mp4", bx(b"moov", &bx(b"udta", &meta(true)))),
            ("quicktime.mov", bx(b"moov", &meta(false))),
        ] {
            let mut file = bx(b"ftyp", b"isom\0\0\x02\0isom");
            // A 64-bit size: 1, the type, the size with its 16-byte header.
            file.extend(1u32.to_be_bytes());
            file.extend(b"mdat");
            file.extend(36u64.to_be_bytes());
            file.extend([0xAB; 20]);
            file.extend(moov);
            let path = dir.join(name);
            std::fs::write(&path, &file).unwrap();
            assert_eq!(mp4_cover(&path), Some(picture.clone()), "{name}");
        }
        let path = dir.join("none.mp4");
        std::fs::write(&path, bx(b"moov", &bx(b"udta", &[]))).unwrap();
        assert!(find(&path).is_none());
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
