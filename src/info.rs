//! What the file's information window says (`ui/info.rs`): the tags in a
//! familiar order with their names in the interface's language, the file's
//! size, container and bitrate, each track's rows (what the file states
//! and, for the picture on screen, what the decoder says), and all of it as
//! plain text for the clipboard. Each function has an `_in(lang, …)` form,
//! as in `format.rs`.

use std::path::Path;

use crate::i18n::Lang;
use crate::player::{AudioOut, Scan, State, VideoParams};
use crate::probe::{Facts, TrackFacts};
use crate::tracks::{self, Kind, Track};

/// What the window describes: the file, mpv's word on it and what its
/// container says (`probe.rs`; `None` until read, and for addresses).
#[derive(Clone, Copy)]
pub struct Subject<'a> {
    pub path: Option<&'a Path>,
    pub state: &'a State,
    pub facts: Option<&'a Facts>,
}

impl<'a> Subject<'a> {
    /// What the container says of `track`.
    fn of(&self, track: &Track) -> Option<&'a TrackFacts> {
        self.facts?.track(track)
    }

    /// A track's average bitrate: mpv's (FFmpeg's demuxers), else the
    /// container's statistics (Matroska written by mkvmerge).
    fn bitrate(&self, track: &Track) -> Option<i64> {
        track
            .bitrate
            .filter(|b| *b > 0)
            .or_else(|| self.of(track)?.bitrate)
    }
}

/// The tags shown first, in this order: mpv's names (any case) and the
/// label in English and Russian.
const KNOWN_TAGS: &[(&[&str], &str, &str)] = &[
    (&["title"], "Title", "Название"),
    (&["artist"], "Artist", "Исполнитель"),
    (
        &["album_artist", "albumartist", "album artist"],
        "Album artist",
        "Исполнитель альбома",
    ),
    (&["album"], "Album", "Альбом"),
    (&["track", "tracknumber"], "Track", "Дорожка"),
    (&["disc", "discnumber"], "Disc", "Диск"),
    (&["date", "year", "originaldate"], "Year", "Год"),
    (&["genre"], "Genre", "Жанр"),
    (&["composer"], "Composer", "Композитор"),
    (&["performer"], "Performer", "Исполнитель (партия)"),
    (&["conductor"], "Conductor", "Дирижёр"),
    (
        &["publisher", "label", "organization"],
        "Publisher",
        "Издатель",
    ),
    (&["copyright"], "Copyright", "Авторские права"),
    (&["comment", "description"], "Comment", "Комментарий"),
    (&["language"], "Language", "Язык"),
    (
        &["encoder", "encoded_by", "encoded-by", "encodedby"],
        "Encoded with",
        "Кодировщик",
    ),
    (
        &["creation_time", "date_released", "date_recorded"],
        "Created",
        "Создан",
    ),
];

/// The value of the first of `keys` among the tags (names in any case).
pub fn tag<'a>(metadata: &'a [(String, String)], keys: &[&str]) -> Option<&'a str> {
    keys.iter().find_map(|key| {
        metadata
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(key))
            .map(|(_, v)| v.as_str())
    })
}

/// The tags as label and value: the known ones first, in their order and
/// named in `lang`, then the rest under the file's own names.
pub fn tags_in(lang: Lang, metadata: &[(String, String)]) -> Vec<(String, String)> {
    let mut used = vec![false; metadata.len()];
    let mut out = Vec::new();
    for (keys, en, ru) in KNOWN_TAGS {
        for key in *keys {
            if let Some(i) = metadata
                .iter()
                .position(|(k, _)| k.eq_ignore_ascii_case(key))
                .filter(|i| !used[*i])
            {
                used[i] = true;
                let label = if lang == Lang::Ru { ru } else { en };
                out.push(((*label).to_owned(), metadata[i].1.clone()));
                break;
            }
        }
    }
    out.extend(
        metadata
            .iter()
            .zip(used)
            .filter(|(_, used)| !used)
            .map(|((k, v), _)| (k.clone(), v.clone())),
    );
    out
}

/// The artist for the header: the artist, else the album's.
pub fn artist(metadata: &[(String, String)]) -> Option<&str> {
    tag(
        metadata,
        &["artist", "album_artist", "albumartist", "performer"],
    )
}

/// The album with its year: "Paranoid (1970)".
pub fn album(metadata: &[(String, String)]) -> Option<String> {
    let album = tag(metadata, &["album"])?;
    // The year of a full date ("1970-09-18").
    let year = tag(metadata, &["date", "year", "originaldate"])
        .map(|d| d.chars().take(4).collect::<String>())
        .filter(|y| y.len() == 4 && y.chars().all(|c| c.is_ascii_digit()));
    Some(match year {
        Some(y) => format!("{album} ({y})"),
        None => album.to_owned(),
    })
}

/// A number with a decimal comma in Russian.
fn decimal_in(lang: Lang, value: f64, decimals: usize) -> String {
    let text = format!("{value:.decimals$}");
    match lang {
        Lang::En => text,
        Lang::Ru => text.replace('.', ","),
    }
}

/// A frame rate: "24 fps", "23,976 fps".
fn fps_in(lang: Lang, fps: f64) -> String {
    let decimals = if (fps - fps.round()).abs() < 0.001 {
        0
    } else {
        3
    };
    format!("{} fps", decimal_in(lang, fps, decimals))
}

/// A refresh rate: "144 Hz", "59,94 Гц".
fn hertz_in(lang: Lang, hz: f64) -> String {
    let decimals = if (hz - hz.round()).abs() < 0.05 { 0 } else { 2 };
    let unit = if lang == Lang::Ru { "Гц" } else { "Hz" };
    format!("{} {unit}", decimal_in(lang, hz, decimals))
}

/// A file size: "734 KB", "1.37 GB"; "1,37 ГБ".
pub fn size_in(lang: Lang, bytes: i64) -> String {
    let units: [&str; 5] = match lang {
        Lang::En => ["bytes", "KB", "MB", "GB", "TB"],
        Lang::Ru => ["байт", "КБ", "МБ", "ГБ", "ТБ"],
    };
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit + 1 < units.len() {
        value /= 1024.0;
        unit += 1;
    }
    let decimals = match unit {
        0 => 0,
        _ if value >= 100.0 => 0,
        _ if value >= 10.0 => 1,
        _ => 2,
    };
    format!("{} {}", decimal_in(lang, value, decimals), units[unit])
}

/// A bitrate: "320 kb/s", "8.5 Mb/s"; "8,5 Мбит/с"; a text track's few
/// bits "62 b/s". The unit follows the value as rounded (999.6 kb/s is
/// "1.0 Mb/s", not "1000 kb/s").
pub fn bitrate_in(lang: Lang, bits_per_second: f64) -> String {
    let (b, k, m) = match lang {
        Lang::En => ("b/s", "kb/s", "Mb/s"),
        Lang::Ru => ("бит/с", "кбит/с", "Мбит/с"),
    };
    let kb = bits_per_second / 1000.0;
    if kb.round() >= 1000.0 {
        let mb = bits_per_second / 1_000_000.0;
        let decimals = if (mb * 10.0).round() >= 1000.0 { 0 } else { 1 };
        format!("{} {m}", decimal_in(lang, mb, decimals))
    } else if kb >= 10.0 {
        format!("{kb:.0} {k}")
    } else if bits_per_second.round() >= 1000.0 {
        format!("{} {k}", decimal_in(lang, kb, 1))
    } else {
        format!("{bits_per_second:.0} {b}")
    }
}

/// The container as people know it, from FFmpeg's demuxer name.
pub fn container(file_format: &str) -> String {
    match file_format {
        // "mkv" is mpv's own demuxer's name.
        "matroska,webm" | "matroska" | "mkv" => "Matroska".into(),
        "webm" => "WebM".into(),
        f if f.starts_with("mov,mp4") => "MP4 / QuickTime".into(),
        "mpegts" => "MPEG-TS".into(),
        "mpeg" => "MPEG-PS".into(),
        "asf" => "ASF (WMV / WMA)".into(),
        "ogg" => "Ogg".into(),
        "flv" => "Flash Video".into(),
        f => f.split(',').next().unwrap_or(f).to_uppercase(),
    }
}

/// The kind of a track: "Video", "Аудио".
pub fn kind_in(lang: Lang, kind: Kind, albumart: bool) -> &'static str {
    match (kind, albumart, lang) {
        (Kind::Video, true, Lang::En) => "Cover",
        (Kind::Video, true, Lang::Ru) => "Обложка",
        (Kind::Video, _, Lang::En) => "Video",
        (Kind::Video, _, Lang::Ru) => "Видео",
        (Kind::Audio, _, Lang::En) => "Audio",
        (Kind::Audio, _, Lang::Ru) => "Аудио",
        (Kind::Sub, _, Lang::En) => "Subtitles",
        (Kind::Sub, _, Lang::Ru) => "Субтитры",
    }
}

/// A track's main details in one line, shown under its name (the rows of
/// [`track_rows_in`] unfold below it):
/// "HEVC · 3840×2160 · 23,976 fps · 10 бит · HDR (PQ) · 14,2 Мбит/с",
/// "AAC · 5.1 · 48 кГц · 384 кбит/с".
pub fn track_details_in(lang: Lang, track: &Track, s: &Subject) -> String {
    let state = s.state;
    let ru = lang == Lang::Ru;
    let mut parts = Vec::new();
    if let Some(format) = track.format_name() {
        parts.push(format);
    }
    match track.kind {
        Kind::Video => {
            if let (Some(w), Some(h)) = (track.width, track.height) {
                parts.push(format!("{w}×{h}"));
            }
            if let Some(fps) = track.fps.filter(|f| *f > 0.0 && !track.albumart) {
                parts.push(fps_in(lang, fps));
            }
            let on_screen = track.selected && !track.albumart;
            if let Some(params) = state.video_params.as_ref().filter(|_| on_screen) {
                // 8 bits are not worth saying here.
                if let Some((_, bits)) = params
                    .pixel_format
                    .as_deref()
                    .and_then(sampling)
                    .filter(|(_, b)| *b > 8)
                {
                    parts.push(if ru {
                        format!("{bits} бит")
                    } else {
                        format!("{bits}-bit")
                    });
                }
                match params.gamma.as_deref() {
                    Some("pq") => parts.push("HDR (PQ)".into()),
                    Some("hlg") => parts.push("HDR (HLG)".into()),
                    _ => {}
                }
            }
        }
        Kind::Audio => {
            if let Some(channels) = track.channels_label(lang) {
                parts.push(channels);
            }
            if let Some(rate) = track.samplerate.filter(|r| *r > 0) {
                let khz = rate as f64 / 1000.0;
                let decimals = tracks::khz_decimals(rate);
                let unit = if ru { "кГц" } else { "kHz" };
                parts.push(format!("{} {unit}", decimal_in(lang, khz, decimals)));
            }
        }
        Kind::Sub => {}
    }
    if let Some(bitrate) = s.bitrate(track) {
        parts.push(bitrate_in(lang, bitrate as f64));
    }
    let flags = [
        (track.external, "external", "внешняя"),
        (track.default, "default", "по умолчанию"),
        (track.forced, "forced", "принудительная"),
    ];
    for (on, en, ru_text) in flags {
        if on {
            parts.push(if ru { ru_text } else { en }.into());
        }
    }
    parts.join(" · ")
}

/// A track's rows: its format, what the file states of it and, for the
/// video track on screen, what the decoder says of the picture.
pub fn track_rows_in(lang: Lang, track: &Track, s: &Subject) -> Vec<(String, String)> {
    let state = s.state;
    let facts = s.of(track);
    let ru = lang == Lang::Ru;
    let label = |en: &str, ru_text: &str| if ru { ru_text } else { en }.to_owned();
    let mut rows = Vec::new();
    if let Some(codec) = &track.codec {
        rows.push((label("Format", "Формат"), tracks::codec_name(codec)));
    }
    if let Some(profile) = &track.profile {
        rows.push((label("Profile", "Профиль"), profile.clone()));
    }
    if let Some(level) = facts.and_then(|f| f.level.as_deref()) {
        rows.push((label("Level", "Уровень"), level_in(lang, level)));
    }
    // The decoder's word on the picture, for the track on screen only.
    let on_screen = track.kind == Kind::Video && track.selected && !track.albumart;
    let params = state.video_params.as_ref().filter(|_| on_screen);
    match track.kind {
        Kind::Video => {
            if let Some(dv) = track.dolby_vision {
                rows.push(("Dolby Vision".into(), dolby_vision_in(lang, dv)));
            }
            if let (Some(w), Some(h)) = (track.width, track.height) {
                rows.push((label("Size", "Размер"), format!("{w}×{h}")));
                let par = track.par.unwrap_or(1.0);
                if (par - 1.0).abs() > 0.001 {
                    rows.push((
                        label("Pixel aspect", "Пропорции пикселя"),
                        decimal_in(lang, par, 3),
                    ));
                }
                let aspect = params
                    .and_then(|p| p.aspect)
                    .unwrap_or(w as f64 * par / h.max(1) as f64);
                rows.push((
                    label("Aspect ratio", "Соотношение сторон"),
                    aspect_in(lang, aspect),
                ));
            }
            if let Some(fps) = track.fps.filter(|f| *f > 0.0 && !track.albumart) {
                rows.push((label("Frame rate", "Частота кадров"), fps_in(lang, fps)));
            }
            if let Some(rotation) = track.rotation {
                rows.push((label("Rotation", "Поворот"), format!("{rotation}°")));
            }
            if let Some(params) = params {
                if let Some((chroma, bits)) = params.pixel_format.as_deref().and_then(sampling) {
                    let bits = if ru {
                        format!("{bits} бит")
                    } else {
                        format!("{bits}-bit")
                    };
                    rows.push((label("Bit depth", "Разрядность"), bits));
                    rows.push((
                        label("Chroma subsampling", "Цветовая субдискретизация"),
                        chroma.to_owned(),
                    ));
                }
                if let Some(scan) = state.scan {
                    rows.push((label("Scan", "Развёртка"), scan_in(lang, scan)));
                }
                if let Some(levels) = &params.levels {
                    let text = match (levels.as_str(), ru) {
                        ("limited", false) => "limited (TV)".to_owned(),
                        ("limited", true) => "ограниченный (TV)".to_owned(),
                        ("full", false) => "full (PC)".to_owned(),
                        ("full", true) => "полный (PC)".to_owned(),
                        (other, _) => other.to_owned(),
                    };
                    rows.push((label("Color range", "Диапазон цветов"), text));
                }
                let colors = [
                    (&params.primaries, "Color primaries", "Основные цвета"),
                    (&params.gamma, "Transfer", "Передаточная функция"),
                    (&params.matrix, "Matrix", "Матрица"),
                ];
                for (value, en, ru_text) in colors {
                    if let Some(value) = value {
                        rows.push((label(en, ru_text), color_name(value)));
                    }
                }
                if let Some(hdr) = hdr_in(lang, params, state.hdr10_plus) {
                    rows.push(("HDR".into(), hdr));
                }
            }
        }
        Kind::Audio => {
            if let Some(mut text) = track.channels_label(lang) {
                // The layout when it says more: "5.1 (5.1(side))"; not
                // "stereo", nor "unknown2" of a track not being decoded.
                if let Some(layout) = track.layout.as_ref().filter(|l| {
                    **l != text
                        && !l.starts_with("unknown")
                        && !["stereo", "mono"].contains(&l.as_str())
                }) {
                    text = format!("{text} ({layout})");
                }
                rows.push((label("Channels", "Каналы"), text));
            }
            if let Some(rate) = track.samplerate.filter(|r| *r > 0) {
                let khz = rate as f64 / 1000.0;
                let decimals = tracks::khz_decimals(rate);
                let unit = if ru { "кГц" } else { "kHz" };
                rows.push((
                    label("Sample rate", "Частота дискретизации"),
                    format!("{} {unit}", decimal_in(lang, khz, decimals)),
                ));
            }
        }
        Kind::Sub => {}
    }
    if let Some(bitrate) = s.bitrate(track) {
        rows.push((
            label("Bitrate", "Битрейт"),
            bitrate_in(lang, bitrate as f64),
        ));
        if let Some(max) = facts.and_then(|f| f.max_bitrate).filter(|m| *m > bitrate) {
            rows.push((
                label("Peak bitrate", "Пиковый битрейт"),
                bitrate_in(lang, max as f64),
            ));
        }
        if let (Some(w), Some(h), Some(fps)) = (track.width, track.height, track.fps) {
            let pixels = (w * h) as f64 * fps;
            if pixels > 0.0 && !track.albumart {
                rows.push((
                    label("Bits/(pixel·frame)", "Бит/(пиксель·кадр)"),
                    decimal_in(lang, bitrate as f64 / pixels, 3),
                ));
            }
        }
        // The size the container counted, else one worked out.
        let counted = facts.and_then(|f| f.bytes).map(|b| b as f64);
        let worked_out = state
            .duration
            .filter(|_| !track.external)
            .map(|d| bitrate as f64 * d / 8.0);
        if let Some(bytes) = counted.or(worked_out) {
            let size = size_in(lang, bytes as i64);
            let mut text = if counted.is_some() {
                size
            } else {
                format!("≈ {size}")
            };
            if let Some(total) = state.file_size.filter(|s| *s > 0) {
                let share = (bytes / total as f64 * 100.0).round().min(100.0);
                text.push_str(&if ru {
                    format!(" ({share:.0} %)")
                } else {
                    format!(" ({share:.0}%)")
                });
            }
            rows.push((label("Stream size", "Размер потока"), text));
        }
    }
    if let Some(frames) = facts.and_then(|f| f.frames) {
        match track.kind {
            Kind::Video if !track.albumart => {
                rows.push((
                    label("Frames", "Кадров"),
                    crate::format::count_in(lang, frames),
                ));
            }
            Kind::Sub => rows.push((
                label("Entries", "Элементов"),
                crate::format::count_in(lang, frames),
            )),
            _ => {}
        }
    }
    if on_screen {
        let text = match (&state.hwdec, ru) {
            (Some(hw), false) => format!("hardware ({hw})"),
            (Some(hw), true) => format!("аппаратное ({hw})"),
            (None, false) => "software".to_owned(),
            (None, true) => "программное".to_owned(),
        };
        rows.push((label("Decoding", "Декодирование"), text));
    }
    let flags: Vec<&str> = [
        (track.default, "default", "по умолчанию"),
        (track.forced, "forced", "принудительная"),
        (track.external, "external", "внешняя"),
        (track.original, "original", "оригинальная"),
        (track.commentary, "commentary", "комментарии"),
        (
            track.hearing_impaired,
            "for the hard of hearing",
            "для слабослышащих",
        ),
        (
            track.visual_impaired,
            "for the visually impaired",
            "для слабовидящих",
        ),
    ]
    .into_iter()
    .filter(|(on, _, _)| *on)
    .map(|(_, en, ru_text)| if ru { ru_text } else { en })
    .collect();
    if !flags.is_empty() {
        rows.push((label("Flags", "Признаки"), flags.join(", ")));
    }
    rows.extend(tags_in(lang, &track.tags));
    if let Some(f) = facts {
        rows.extend(tags_in(lang, &f.tags));
    }
    rows
}

/// Dolby Vision's profile and level: "profile 8, level 6".
fn dolby_vision_in(lang: Lang, (profile, level): (i64, i64)) -> String {
    match (lang, level) {
        (Lang::En, 0) => format!("profile {profile}"),
        (Lang::Ru, 0) => format!("профиль {profile}"),
        (Lang::En, l) => format!("profile {profile}, level {l}"),
        (Lang::Ru, l) => format!("профиль {profile}, уровень {l}"),
    }
}

/// A codec's level: "4.1", "5.1, High tier"; "5.1, ярус High".
fn level_in(lang: Lang, level: &str) -> String {
    match level.strip_suffix(" High") {
        Some(l) => match lang {
            Lang::En => format!("{l}, High tier"),
            Lang::Ru => format!("{l}, ярус High"),
        },
        None => level.to_owned(),
    }
}

/// The chroma subsampling and the bits per component of an FFmpeg pixel
/// format: `yuv420p10` → 4:2:0, 10; `nv12` → 4:2:0, 8.
fn sampling(pixel_format: &str) -> Option<(&'static str, u32)> {
    let format = pixel_format
        .strip_suffix("le")
        .or_else(|| pixel_format.strip_suffix("be"))
        .unwrap_or(pixel_format);
    // Formats of their own name: semi-planar and packed ones.
    let known = match format {
        "nv12" | "nv21" => Some(("4:2:0", 8)),
        "p010" => Some(("4:2:0", 10)),
        "p012" => Some(("4:2:0", 12)),
        "p016" => Some(("4:2:0", 16)),
        "nv16" | "yuyv422" | "uyvy422" => Some(("4:2:2", 8)),
        "p210" | "y210" => Some(("4:2:2", 10)),
        "nv24" => Some(("4:4:4", 8)),
        "p410" | "y410" | "xv30" => Some(("4:4:4", 10)),
        _ => None,
    };
    if known.is_some() {
        return known;
    }
    let chroma = [
        ("yuv420", "4:2:0"),
        ("yuvj420", "4:2:0"),
        ("yuva420", "4:2:0"),
        ("yuv422", "4:2:2"),
        ("yuvj422", "4:2:2"),
        ("yuva422", "4:2:2"),
        ("yuv444", "4:4:4"),
        ("yuvj444", "4:4:4"),
        ("yuva444", "4:4:4"),
        ("yuv440", "4:4:0"),
        ("yuv411", "4:1:1"),
        ("yuv410", "4:1:0"),
        ("gbr", "4:4:4 (RGB)"),
        ("gray", "4:0:0"),
    ]
    .iter()
    .find(|(prefix, _)| format.starts_with(prefix))?
    .1;
    // The bits after the last letter: yuv420p10 → 10; none → 8.
    let digits: String = format
        .chars()
        .rev()
        .take_while(char::is_ascii_digit)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    let bits = match digits.parse::<u32>() {
        Ok(b) if format.len() > digits.len() + 3 && (9..=16).contains(&b) => b,
        _ => 8,
    };
    Some((chroma, bits))
}

/// A display aspect ratio: "16:9", "2:1", "2,39:1".
fn aspect_in(lang: Lang, aspect: f64) -> String {
    const NAMED: [(f64, f64); 8] = [
        (16.0, 9.0),
        (4.0, 3.0),
        (16.0, 10.0),
        (5.0, 4.0),
        (3.0, 2.0),
        (1.0, 1.0),
        (2.0, 1.0),
        (21.0, 9.0),
    ];
    if let Some((w, h)) = NAMED.iter().find(|(w, h)| (aspect - w / h).abs() < 0.01) {
        return format!("{w}:{h}");
    }
    let text = decimal_in(lang, aspect, 2);
    format!("{text}:1")
}

/// The scan in words.
fn scan_in(lang: Lang, scan: Scan) -> String {
    match (scan, lang) {
        (Scan::Progressive, Lang::En) => "progressive".into(),
        (Scan::Progressive, Lang::Ru) => "прогрессивная".into(),
        (Scan::Interlaced { top_first: true }, Lang::En) => "interlaced, top field first".into(),
        (Scan::Interlaced { top_first: true }, Lang::Ru) => {
            "чересстрочная, верхнее поле первым".into()
        }
        (Scan::Interlaced { top_first: false }, Lang::En) => {
            "interlaced, bottom field first".into()
        }
        (Scan::Interlaced { top_first: false }, Lang::Ru) => {
            "чересстрочная, нижнее поле первым".into()
        }
    }
}

/// mpv's name of a color space, primaries or transfer function as the
/// standards write it.
fn color_name(name: &str) -> String {
    match name {
        "bt.601" => "BT.601",
        "bt.601-525" => "BT.601 NTSC",
        "bt.601-625" => "BT.601 PAL",
        "bt.709" => "BT.709",
        "bt.2020" => "BT.2020",
        "bt.2020-ncl" => "BT.2020 NCL",
        "bt.2020-cl" => "BT.2020 CL",
        "bt.2100-pq" => "BT.2100 PQ",
        "bt.2100-hlg" => "BT.2100 HLG",
        "bt.470m" => "BT.470 M",
        "bt.1886" => "BT.1886",
        "smpte-240m" => "SMPTE 240M",
        "srgb" => "sRGB",
        "pq" => "PQ (SMPTE ST 2084)",
        "hlg" => "HLG",
        "linear" => "Linear",
        "rgb" => "RGB",
        "ycgco" => "YCgCo",
        "dci-p3" => "DCI-P3",
        "display-p3" => "Display P3",
        "apple" => "Apple RGB",
        "adobe" => "Adobe RGB",
        "film-c" => "Film C",
        "gamma2.2" => "Gamma 2.2",
        "gamma2.4" => "Gamma 2.4",
        other => return other.to_owned(),
    }
    .to_owned()
}

/// What makes the picture HDR: the transfer function and the light levels
/// the file states. `None` for SDR.
fn hdr_in(lang: Lang, params: &VideoParams, hdr10_plus: bool) -> Option<String> {
    let kind = match params.gamma.as_deref() {
        Some("pq") if hdr10_plus => "HDR10+ (PQ)",
        Some("pq") => "HDR10 (PQ)",
        Some("hlg") => "HLG",
        _ => return None,
    };
    let mut parts = vec![kind.to_owned()];
    let nits = if lang == Lang::Ru {
        "кд/м²"
    } else {
        "cd/m²"
    };
    if let Some(v) = params.max_cll {
        parts.push(format!("MaxCLL {v:.0} {nits}"));
    }
    if let Some(v) = params.max_fall {
        parts.push(format!("MaxFALL {v:.0} {nits}"));
    }
    if let Some(v) = params.max_luma {
        let range = match params.min_luma {
            Some(min) => format!("{}–{v:.0}", decimal_in(lang, min, 4)),
            None => format!("{v:.0}"),
        };
        let mut text = match lang {
            Lang::En => format!("mastering {range} {nits}"),
            Lang::Ru => format!("мастеринг {range} {nits}"),
        };
        if let Some(gamut) = params.mastering {
            text.push_str(&format!(", {gamut}"));
        }
        parts.push(text);
    }
    Some(parts.join(" · "))
}

/// The file's rows: name, folder, size, container, duration, bitrate.
pub fn file_rows_in(lang: Lang, s: &Subject) -> Vec<(String, String)> {
    let (path, state) = (s.path, s.state);
    let ru = lang == Lang::Ru;
    let label = |en: &str, ru_text: &str| if ru { ru_text } else { en }.to_owned();
    let mut rows = Vec::new();
    if let Some(path) = path {
        if let Some(name) = path.file_name() {
            rows.push((label("Name", "Имя"), name.to_string_lossy().into_owned()));
        }
        if crate::playlist::is_url(path) {
            rows.push((label("Address", "Адрес"), path.display().to_string()));
        } else if let Some(dir) = path.parent() {
            rows.push((label("Folder", "Папка"), dir.display().to_string()));
        }
    }
    if let Some(size) = state.file_size {
        rows.push((label("Size", "Размер"), size_in(lang, size)));
    }
    if let Some(format) = &state.file_format {
        rows.push((label("Format", "Формат"), container(format)));
    }
    if let Some(d) = state.duration {
        rows.push((
            label("Duration", "Длительность"),
            crate::format::time(Some(d), false),
        ));
        if let Some(size) = state.file_size.filter(|_| d > 0.0) {
            rows.push((
                label("Bitrate", "Битрейт"),
                bitrate_in(lang, size as f64 * 8.0 / d),
            ));
        }
    }
    if !state.chapters.is_empty() {
        rows.push((label("Chapters", "Главы"), state.chapters.len().to_string()));
    }
    if let Some(f) = s.facts {
        // What mpv's tags already say is not said again.
        if let Some(app) = f
            .writing_app
            .as_ref()
            .filter(|a| tag(&state.metadata, &["encoder"]) != Some(a.as_str()))
        {
            rows.push((label("Written by", "Программа записи"), app.clone()));
        }
        if let Some(library) = f
            .muxing_app
            .as_ref()
            .filter(|m| f.writing_app.as_ref() != Some(*m))
        {
            rows.push((label("Library", "Библиотека"), library.clone()));
        }
        let date = |t| crate::win::local_time(t).map(|d| crate::format::date_in(lang, d));
        if tag(&state.metadata, &["creation_time"]).is_none()
            && let Some(d) = f.written.and_then(date)
        {
            rows.push((label("Written", "Дата записи"), d));
        }
        if let Some(d) = f.created.and_then(date) {
            rows.push((label("File created", "Файл создан"), d));
        }
        if let Some(d) = f.modified.and_then(date) {
            rows.push((label("File modified", "Файл изменён"), d));
        }
    }
    rows
}

/// How the playback goes now (mpv's figures, asked for every second
/// while the window is open): the bitrates of the moment, the frame rate
/// against the screen's, dropped frames, what the sound device gets and,
/// over the network, the cache.
pub fn live_rows_in(lang: Lang, s: &Subject) -> Vec<(String, String)> {
    let live = &s.state.live;
    let ru = lang == Lang::Ru;
    let label = |en: &str, ru_text: &str| if ru { ru_text } else { en }.to_owned();
    let mut rows = Vec::new();
    let picture = s.state.video().is_some();
    if let Some(b) = live.video_bitrate.filter(|_| picture) {
        rows.push((label("Video now", "Видео сейчас"), bitrate_in(lang, b)));
    }
    if let Some(b) = live.audio_bitrate {
        rows.push((label("Sound now", "Звук сейчас"), bitrate_in(lang, b)));
    }
    if picture {
        if let Some(fps) = live.fps {
            let mut text = fps_in(lang, fps);
            if let Some(hz) = live.display_fps {
                let screen = if ru { "экран" } else { "screen" };
                text = format!("{text}, {screen} {}", hertz_in(lang, hz));
            }
            rows.push((label("Frames shown", "Показ кадров"), text));
        }
        if let Some(dropped) = live.dropped {
            let decoder = live.decoder_dropped.unwrap_or(0);
            let text = match (decoder, ru) {
                (0, _) => dropped.to_string(),
                (d, false) => format!("output {dropped}, decoder {d}"),
                (d, true) => format!("вывод {dropped}, декодер {d}"),
            };
            rows.push((label("Dropped frames", "Пропущено кадров"), text));
        }
    }
    if let Some(text) = live.audio_out.as_ref().and_then(|o| audio_out_in(lang, o)) {
        rows.push((label("Sound output", "Вывод звука"), text));
    }
    if live.network
        && let Some(cache) = live.cache
    {
        let mut text = if ru {
            format!("{} с", decimal_in(lang, cache, 0))
        } else {
            format!("{} s", decimal_in(lang, cache, 0))
        };
        if let Some(speed) = live.cache_speed.filter(|b| *b > 0) {
            let per = if ru { "/с" } else { "/s" };
            text.push_str(&format!(" · {}{per}", size_in(lang, speed)));
        }
        rows.push((label("Read ahead", "Буфер"), text));
    }
    rows
}

/// What the sound device gets: "48 kHz · 5.1 · float"; compressed sound
/// passed through: "bitstream (E-AC3)".
pub fn audio_out_in(lang: Lang, out: &AudioOut) -> Option<String> {
    let ru = lang == Lang::Ru;
    if let Some(codec) = out.format.as_deref().and_then(|f| f.strip_prefix("spdif-")) {
        let codec = tracks::codec_name(codec);
        return Some(if ru {
            format!("битовый поток ({codec})")
        } else {
            format!("bitstream ({codec})")
        });
    }
    let mut parts = Vec::new();
    if let Some(rate) = out.samplerate.filter(|r| *r > 0) {
        let khz = rate as f64 / 1000.0;
        let decimals = tracks::khz_decimals(rate);
        let unit = if ru { "кГц" } else { "kHz" };
        parts.push(format!("{} {unit}", decimal_in(lang, khz, decimals)));
    }
    if let Some(channels) = &out.channels {
        parts.push(match channels.as_str() {
            "stereo" => "2.0".to_owned(),
            "mono" => tracks::channels_in(lang, 1),
            c => c.to_owned(),
        });
    }
    if let Some(format) = &out.format {
        let format = format.trim_end_matches('p');
        let bits = match format {
            "u8" => Some(8),
            "s16" => Some(16),
            "s24" => Some(24),
            "s32" => Some(32),
            "s64" => Some(64),
            _ => None,
        };
        parts.push(match (bits, format) {
            (Some(b), _) if ru => format!("{b} бит"),
            (Some(b), _) => format!("{b}-bit"),
            (None, "double") => "float 64".to_owned(),
            (None, f) => f.to_owned(),
        });
    }
    (!parts.is_empty()).then(|| parts.join(" · "))
}

/// What a chip of the header does when clicked.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChipAction {
    None,
    /// Plays this audio track.
    Audio(i64),
}

/// A chip's colour, when the window shows colours.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tone {
    /// Grey: the file's size, duration and bitrate.
    Plain,
    /// The format, in the colours of its file icon.
    Format(crate::filetypes::Colors),
    Picture,
    Sound,
    Subtitles,
}

/// A short fact of the file in the header, on a light ground: "MKV",
/// "HEVC", "1920×1080", "AAC 5.1 Русский".
#[derive(Clone, Debug, PartialEq)]
pub struct Chip {
    pub text: String,
    /// The whole of it, for the tooltip.
    pub tip: Option<String>,
    /// The track playing: outlined.
    pub selected: bool,
    pub action: ChipAction,
    pub tone: Tone,
}

impl Chip {
    fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            tip: None,
            selected: false,
            action: ChipAction::None,
            tone: Tone::Plain,
        }
    }

    fn tone(mut self, tone: Tone) -> Self {
        self.tone = tone;
        self
    }

    fn tip(mut self, tip: impl Into<String>) -> Self {
        self.tip = Some(tip.into());
        self
    }
}

/// Audio chips beyond this many are folded into one ("+3").
const AUDIO_CHIPS: usize = 4;

/// The header's chips in rows: the file (format, size, duration, overall
/// bitrate), the picture (codec, size, fps, depth, HDR, bitrate), the
/// sound (a chip per audio track, the playing one marked; the subtitles'
/// count and language). What the list below says, at a glance.
pub fn chips_in(lang: Lang, s: &Subject) -> Vec<Vec<Chip>> {
    let (path, state) = (s.path, s.state);
    let ru = lang == Lang::Ru;
    let mut rows = Vec::new();

    let mut file = Vec::new();
    if let Some(format) = &state.file_format {
        let container = container(format);
        // The extension is what people call the format (MKV, not Matroska).
        let extension = path
            .filter(|p| !crate::playlist::is_url(p))
            .and_then(|p| p.extension())
            .map(|e| e.to_string_lossy().into_owned())
            .filter(|e| e.len() <= 5);
        let tone = extension
            .as_deref()
            .and_then(crate::filetypes::of_extension)
            .map_or(Tone::Plain, |t| Tone::Format(t.colors));
        let chip = match extension {
            Some(e) => Chip::new(e.to_uppercase()).tip(container),
            None => Chip::new(container),
        };
        file.push(chip.tone(tone));
    }
    if let Some(size) = state.file_size {
        file.push(Chip::new(size_in(lang, size)).tip(if ru {
            "Размер файла"
        } else {
            "File size"
        }));
    }
    if let Some(d) = state.duration {
        file.push(Chip::new(crate::format::time(Some(d), false)).tip(if ru {
            "Длительность"
        } else {
            "Duration"
        }));
        if let Some(size) = state.file_size.filter(|_| d > 0.0) {
            file.push(
                Chip::new(bitrate_in(lang, size as f64 * 8.0 / d)).tip(if ru {
                    "Общий битрейт: размер файла на длительность"
                } else {
                    "Overall bitrate: the file's size over its duration"
                }),
            );
        }
    }
    if !file.is_empty() {
        rows.push(file);
    }

    let video = state
        .tracks
        .iter()
        .find(|t| t.kind == Kind::Video && t.selected && !t.albumart)
        .or_else(|| {
            state
                .tracks
                .iter()
                .find(|t| t.kind == Kind::Video && !t.albumart)
        });
    if let Some(track) = video {
        let mut picture = Vec::new();
        if let Some(codec) = &track.codec {
            let mut chip = Chip::new(tracks::codec_name(codec));
            let mut tip = track.codec_desc.clone().unwrap_or_default();
            let level = s
                .of(track)
                .and_then(|f| f.level.as_deref())
                .map(|l| level_in(lang, l));
            let level = level.map(|l| {
                if ru {
                    format!("уровень {l}")
                } else {
                    format!("level {l}")
                }
            });
            for part in [track.profile.clone(), level].into_iter().flatten() {
                if !tip.is_empty() {
                    tip.push_str(" · ");
                }
                tip.push_str(&part);
            }
            if !tip.is_empty() {
                chip = chip.tip(tip);
            }
            picture.push(chip);
        }
        if let (Some(w), Some(h)) = (track.width, track.height) {
            picture.push(Chip::new(format!("{w}×{h}")));
        }
        if let Some(fps) = track.fps.filter(|f| *f > 0.0) {
            picture.push(Chip::new(fps_in(lang, fps)));
        }
        if let Some(params) = state.video_params.as_ref().filter(|_| track.selected) {
            if let Some((_, bits)) = params
                .pixel_format
                .as_deref()
                .and_then(sampling)
                .filter(|(_, b)| *b > 8)
            {
                picture.push(Chip::new(if ru {
                    format!("{bits} бит")
                } else {
                    format!("{bits}-bit")
                }));
            }
            let hdr = match params.gamma.as_deref() {
                Some("pq") if state.hdr10_plus => Some("HDR10+"),
                Some("pq") => Some("HDR (PQ)"),
                Some("hlg") => Some("HDR (HLG)"),
                _ => None,
            };
            if let Some(hdr) = hdr {
                let mut chip = Chip::new(hdr);
                if let Some(tip) = hdr_in(lang, params, state.hdr10_plus) {
                    chip = chip.tip(tip);
                }
                picture.push(chip);
            }
        }
        if let Some(dv) = track.dolby_vision {
            picture.push(Chip::new("Dolby Vision").tip(dolby_vision_in(lang, dv)));
        }
        if let Some(bitrate) = s.bitrate(track) {
            picture.push(Chip::new(bitrate_in(lang, bitrate as f64)).tip(if ru {
                "Битрейт видео"
            } else {
                "Video bitrate"
            }));
        }
        if !picture.is_empty() {
            for chip in &mut picture {
                chip.tone = Tone::Picture;
            }
            rows.push(picture);
        }
    }

    let mut sound = Vec::new();
    let audio = tracks::of_kind(&state.tracks, Kind::Audio);
    let shown = if audio.len() > AUDIO_CHIPS {
        AUDIO_CHIPS - 1
    } else {
        audio.len()
    };
    for (i, track) in audio.iter().enumerate().take(shown) {
        let mut parts = Vec::new();
        if let Some(format) = track.format_name() {
            parts.push(format);
        }
        if let Some(channels) = track.channels_label(lang) {
            parts.push(channels);
        }
        if let Some(code) = &track.lang {
            parts.push(tracks::language_in(lang, code));
        }
        let name = track.name_in(lang, i + 1);
        if parts.is_empty() {
            parts.push(name.clone());
        }
        let tip = format!(
            "{} {}: {name}\n{}",
            kind_in(lang, Kind::Audio, false),
            i + 1,
            track_details_in(lang, track, s)
        );
        sound.push(Chip {
            text: parts.join(" "),
            tip: Some(tip.trim_end().to_owned()),
            selected: track.selected,
            action: ChipAction::Audio(track.id),
            tone: Tone::Sound,
        });
    }
    if shown < audio.len() {
        let rest: Vec<String> = audio
            .iter()
            .enumerate()
            .skip(shown)
            .map(|(i, t)| format!("{}. {}", i + 1, t.name_in(lang, i + 1)))
            .collect();
        sound.push(Chip {
            text: format!("+{}", audio.len() - shown),
            tip: Some(rest.join("\n")),
            selected: audio.iter().skip(shown).any(|t| t.selected),
            action: ChipAction::None,
            tone: Tone::Sound,
        });
    }
    let subs = tracks::of_kind(&state.tracks, Kind::Sub);
    if !subs.is_empty() {
        let label = if ru { "Субтитры" } else { "Subtitles" };
        let n = subs.len();
        let text = match subs.iter().position(|t| t.selected) {
            Some(i) => {
                let name = subs[i].name_in(lang, i + 1);
                if n > 1 {
                    format!("{label}: {name} ({}/{n})", i + 1)
                } else {
                    format!("{label}: {name}")
                }
            }
            None => format!("{label}: {n}"),
        };
        let all: Vec<String> = subs
            .iter()
            .enumerate()
            .map(|(i, t)| {
                let mark = if t.selected { "● " } else { "   " };
                format!("{mark}{}. {}", i + 1, t.name_in(lang, i + 1))
            })
            .collect();
        sound.push(Chip::new(text).tip(all.join("\n")).tone(Tone::Subtitles));
    }
    if !sound.is_empty() {
        rows.push(sound);
    }
    rows
}

/// A track's heading: "Audio 2: English — Commentary".
pub fn track_heading_in(lang: Lang, track: &Track, tracks: &[Track]) -> String {
    let n = track.number(tracks);
    let kind = kind_in(lang, track.kind, track.albumart);
    if track.albumart {
        return kind.to_owned();
    }
    format!("{kind} {n}: {}", track.name_in(lang, n))
}

/// A section as text: its title, then a line per row with the labels in
/// a column, as MediaInfo writes them ("Format       : Matroska"); the
/// lines of a long value go on under its first.
pub fn section_text(title: &str, rows: &[(String, String)]) -> String {
    let width = rows
        .iter()
        .map(|(l, _)| l.chars().count())
        .max()
        .unwrap_or(0);
    let mut text = format!("{title}\n");
    for (label, value) in rows {
        let pad = " ".repeat(width - label.chars().count());
        let mut lines = value.lines();
        let first = lines.next().unwrap_or_default();
        text.push_str(&format!("{label}{pad} : {first}\n"));
        for line in lines {
            text.push_str(&format!("{}   {line}\n", " ".repeat(width)));
        }
    }
    text
}

/// The tracks as text: a section each.
pub fn tracks_text_in(lang: Lang, s: &Subject) -> String {
    let tracks = &s.state.tracks;
    tracks
        .iter()
        .map(|t| {
            section_text(
                &track_heading_in(lang, t, tracks),
                &track_rows_in(lang, t, s),
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Everything the window shows, as text for the clipboard.
pub fn report_in(lang: Lang, s: &Subject) -> String {
    let ru = lang == Lang::Ru;
    let title = |en, ru_text| if ru { ru_text } else { en };
    let mut parts: Vec<String> = [
        (title("File", "Файл"), file_rows_in(lang, s)),
        (title("Playback", "Воспроизведение"), live_rows_in(lang, s)),
        (title("Tags", "Теги"), tags_in(lang, &s.state.metadata)),
    ]
    .into_iter()
    .filter(|(_, rows)| !rows.is_empty())
    .map(|(title, rows)| section_text(title, &rows))
    .collect();
    if !s.state.tracks.is_empty() {
        parts.push(tracks_text_in(lang, s));
    }
    parts.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tags(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect()
    }

    #[test]
    fn known_tags_first_and_named() {
        let m = tags(&[
            ("ENCODER", "Lavf61"),
            ("ALBUM", "Paranoid"),
            ("TITLE", "War Pigs"),
            ("DATE", "1970-09-18"),
            ("ARTIST", "Black Sabbath"),
            ("REPLAYGAIN_TRACK_GAIN", "-6.1 dB"),
        ]);
        let t = tags_in(Lang::Ru, &m);
        let labels: Vec<&str> = t.iter().map(|(l, _)| l.as_str()).collect();
        assert_eq!(
            labels,
            [
                "Название",
                "Исполнитель",
                "Альбом",
                "Год",
                "Кодировщик",
                "REPLAYGAIN_TRACK_GAIN"
            ]
        );
        assert_eq!(artist(&m), Some("Black Sabbath"));
        assert_eq!(album(&m).as_deref(), Some("Paranoid (1970)"));
        assert_eq!(tag(&m, &["title"]), Some("War Pigs"));
    }

    #[test]
    fn sizes_and_bitrates() {
        assert_eq!(size_in(Lang::En, 512), "512 bytes");
        assert_eq!(size_in(Lang::Ru, 1_468_006), "1,40 МБ");
        assert_eq!(size_in(Lang::En, 1_471_026_299), "1.37 GB");
        assert_eq!(size_in(Lang::Ru, 160 << 20), "160 МБ");
        assert_eq!(bitrate_in(Lang::En, 320_000.0), "320 kb/s");
        assert_eq!(bitrate_in(Lang::Ru, 8_512_000.0), "8,5 Мбит/с");
        assert_eq!(bitrate_in(Lang::En, 62.0), "62 b/s");
        assert_eq!(bitrate_in(Lang::Ru, 62.0), "62 бит/с");
        assert_eq!(bitrate_in(Lang::Ru, 4_260.0), "4,3 кбит/с");
        assert_eq!(bitrate_in(Lang::En, 999_600.0), "1.0 Mb/s");
        assert_eq!(bitrate_in(Lang::En, 99_960_000.0), "100 Mb/s");
        assert_eq!(container("matroska,webm"), "Matroska");
        assert_eq!(container("mov,mp4,m4a,3gp,3g2,mj2"), "MP4 / QuickTime");
        assert_eq!(container("mkv"), "Matroska");
        assert_eq!(container("mp3"), "MP3");
    }

    #[test]
    fn samplings() {
        assert_eq!(sampling("yuv420p10"), Some(("4:2:0", 10)));
        assert_eq!(sampling("yuv420p10le"), Some(("4:2:0", 10)));
        assert_eq!(sampling("yuv422p"), Some(("4:2:2", 8)));
        assert_eq!(sampling("yuvj444p"), Some(("4:4:4", 8)));
        assert_eq!(sampling("p010"), Some(("4:2:0", 10)));
        assert_eq!(sampling("nv12"), Some(("4:2:0", 8)));
        assert_eq!(sampling("gray10le"), Some(("4:0:0", 10)));
        assert_eq!(sampling("d3d11"), None);
        assert_eq!(aspect_in(Lang::Ru, 1920.0 / 1080.0), "16:9");
        assert_eq!(aspect_in(Lang::En, 2.0), "2:1");
        assert_eq!(aspect_in(Lang::Ru, 2.39), "2,39:1");
    }

    /// The rows' values by their English labels.
    fn rows_of(track: &Track, state: &State) -> Vec<(String, String)> {
        track_rows_in(Lang::En, track, &subject(state))
    }

    fn subject(state: &State) -> Subject<'_> {
        Subject {
            path: None,
            state,
            facts: None,
        }
    }

    fn value<'a>(rows: &'a [(String, String)], label: &str) -> Option<&'a str> {
        rows.iter()
            .find(|(l, _)| l == label)
            .map(|(_, v)| v.as_str())
    }

    #[test]
    fn track_rows() {
        let video = Track {
            id: 1,
            kind: Kind::Video,
            codec: Some("h264".into()),
            profile: Some("High".into()),
            width: Some(1920),
            height: Some(960),
            fps: Some(24.0),
            bitrate: Some(7_425_000),
            default: true,
            selected: true,
            tags: vec![("encoder".into(), "Lavc62.28.100 h264_nvenc".into())],
            ..Track::default()
        };
        let state = State {
            idle: false,
            duration: Some(6480.0),
            file_size: Some(6_365_000_000),
            hwdec: Some("nvdec".into()),
            scan: Some(Scan::Progressive),
            video_params: Some(VideoParams {
                pixel_format: Some("nv12".into()),
                primaries: Some("bt.709".into()),
                gamma: Some("bt.1886".into()),
                matrix: Some("bt.709".into()),
                levels: Some("limited".into()),
                aspect: Some(2.0),
                ..VideoParams::default()
            }),
            tracks: vec![video.clone()],
            ..State::default()
        };
        assert_eq!(
            track_details_in(Lang::En, &video, &subject(&state)),
            "H.264 · 1920×960 · 24 fps · 7.4 Mb/s · default"
        );
        let rows = rows_of(&video, &state);
        assert_eq!(value(&rows, "Format"), Some("H.264"));
        assert_eq!(value(&rows, "Profile"), Some("High"));
        assert_eq!(value(&rows, "Size"), Some("1920×960"));
        assert_eq!(value(&rows, "Aspect ratio"), Some("2:1"));
        assert_eq!(value(&rows, "Frame rate"), Some("24 fps"));
        assert_eq!(value(&rows, "Bit depth"), Some("8-bit"));
        assert_eq!(value(&rows, "Chroma subsampling"), Some("4:2:0"));
        assert_eq!(value(&rows, "Scan"), Some("progressive"));
        assert_eq!(value(&rows, "Color range"), Some("limited (TV)"));
        assert_eq!(value(&rows, "Transfer"), Some("BT.1886"));
        assert_eq!(value(&rows, "Bits/(pixel·frame)"), Some("0.168"));
        assert_eq!(value(&rows, "Stream size"), Some("≈ 5.60 GB (94%)"));
        assert_eq!(value(&rows, "Decoding"), Some("hardware (nvdec)"));
        assert_eq!(value(&rows, "Flags"), Some("default"));
        assert_eq!(
            value(&rows, "Encoded with"),
            Some("Lavc62.28.100 h264_nvenc")
        );
        assert_eq!(value(&rows, "HDR"), None);

        // A video track not on screen: only what the file states.
        let other = Track {
            selected: false,
            ..video.clone()
        };
        let rows = rows_of(&other, &state);
        assert_eq!(value(&rows, "Bit depth"), None);
        assert_eq!(value(&rows, "Decoding"), None);

        let audio = Track {
            kind: Kind::Audio,
            codec: Some("aac".into()),
            profile: Some("LC".into()),
            channels: Some(6),
            layout: Some("5.1(side)".into()),
            samplerate: Some(48000),
            bitrate: Some(384_000),
            commentary: true,
            ..Track::default()
        };
        assert_eq!(
            track_details_in(Lang::Ru, &audio, &subject(&state)),
            "AAC · 5.1 · 48 кГц · 384 кбит/с"
        );
        let rows = track_rows_in(Lang::Ru, &audio, &subject(&state));
        assert_eq!(value(&rows, "Каналы"), Some("5.1 (5.1(side))"));
        assert_eq!(value(&rows, "Частота дискретизации"), Some("48 кГц"));
        assert_eq!(value(&rows, "Битрейт"), Some("384 кбит/с"));
        assert_eq!(value(&rows, "Признаки"), Some("комментарии"));
        assert_eq!(value(&rows, "Бит/(пиксель·кадр)"), None);
    }

    #[test]
    fn container_facts() {
        // mpv's own Matroska demuxer: no bitrate; mkvmerge's statistics.
        let video = Track {
            id: 1,
            src_id: Some(1),
            kind: Kind::Video,
            codec: Some("hevc".into()),
            profile: Some("Main 10".into()),
            width: Some(3840),
            height: Some(2160),
            fps: Some(24.0),
            selected: true,
            ..Track::default()
        };
        let subs = Track {
            id: 1,
            src_id: Some(3),
            kind: Kind::Sub,
            ..Track::default()
        };
        let state = State {
            idle: false,
            duration: Some(6000.0),
            file_size: Some(10_000_000_000),
            tracks: vec![video.clone(), subs.clone()],
            ..State::default()
        };
        let facts = Facts {
            tracks: vec![
                TrackFacts {
                    number: Some(1),
                    level: Some("5.1 High".into()),
                    bitrate: Some(12_000_000),
                    bytes: Some(9_000_000_000),
                    frames: Some(144_000),
                    tags: vec![("ENCODER".into(), "x265".into())],
                    ..TrackFacts::default()
                },
                TrackFacts {
                    number: Some(3),
                    index: 2,
                    frames: Some(1234),
                    ..TrackFacts::default()
                },
            ],
            ..Facts::default()
        };
        let s = Subject {
            path: None,
            state: &state,
            facts: Some(&facts),
        };
        let rows = track_rows_in(Lang::En, &video, &s);
        assert_eq!(value(&rows, "Level"), Some("5.1, High tier"));
        assert_eq!(value(&rows, "Bitrate"), Some("12.0 Mb/s"));
        assert_eq!(value(&rows, "Stream size"), Some("8.38 GB (90%)"));
        assert_eq!(value(&rows, "Frames"), Some("144,000"));
        assert_eq!(value(&rows, "Encoded with"), Some("x265"));
        assert!(track_details_in(Lang::En, &video, &s).ends_with("12.0 Mb/s"));
        let rows = track_rows_in(Lang::Ru, &subs, &s);
        assert_eq!(value(&rows, "Элементов"), Some("1\u{a0}234"));
        let chips = chips_in(Lang::En, &s);
        assert_eq!(
            chips[1][0].tip.as_deref(),
            Some("Main 10 · level 5.1, High tier")
        );
    }

    #[test]
    fn live_rows() {
        let state = State {
            idle: false,
            width: Some(1920),
            height: Some(1080),
            live: crate::player::Live {
                video_bitrate: Some(8_400_000.0),
                audio_bitrate: Some(640_000.0),
                fps: Some(23.976),
                display_fps: Some(143.998),
                dropped: Some(12),
                decoder_dropped: Some(3),
                audio_out: Some(AudioOut {
                    format: Some("floatp".into()),
                    samplerate: Some(48000),
                    channels: Some("5.1".into()),
                    channel_count: Some(6),
                }),
                cache: Some(12.4),
                cache_speed: Some(2_202_009),
                network: true,
                ..Default::default()
            },
            ..State::default()
        };
        let rows = live_rows_in(Lang::Ru, &subject(&state));
        assert_eq!(value(&rows, "Видео сейчас"), Some("8,4 Мбит/с"));
        assert_eq!(value(&rows, "Звук сейчас"), Some("640 кбит/с"));
        assert_eq!(
            value(&rows, "Показ кадров"),
            Some("23,976 fps, экран 144 Гц")
        );
        assert_eq!(
            value(&rows, "Пропущено кадров"),
            Some("вывод 12, декодер 3")
        );
        assert_eq!(value(&rows, "Вывод звука"), Some("48 кГц · 5.1 · float"));
        assert_eq!(value(&rows, "Буфер"), Some("12 с · 2,10 МБ/с"));
        let out = AudioOut {
            format: Some("spdif-eac3".into()),
            ..Default::default()
        };
        assert_eq!(
            audio_out_in(Lang::En, &out).as_deref(),
            Some("bitstream (E-AC3)")
        );
        let out = AudioOut {
            format: Some("s16".into()),
            samplerate: Some(44100),
            channels: Some("stereo".into()),
            channel_count: Some(2),
        };
        assert_eq!(
            audio_out_in(Lang::En, &out).as_deref(),
            Some("44.1 kHz · 2.0 · 16-bit")
        );
    }

    #[test]
    fn section_texts() {
        let rows = vec![
            ("Format".to_owned(), "Matroska".to_owned()),
            ("Size".to_owned(), "2.13 GB".to_owned()),
            ("Comment".to_owned(), "One\nTwo".to_owned()),
        ];
        assert_eq!(
            section_text("File", &rows),
            "File\nFormat  : Matroska\nSize    : 2.13 GB\nComment : One\n          Two\n"
        );
    }

    #[test]
    fn hdr() {
        let params = VideoParams {
            gamma: Some("pq".into()),
            max_cll: Some(1000.0),
            max_fall: Some(400.0),
            ..VideoParams::default()
        };
        assert_eq!(
            hdr_in(Lang::En, &params, false).as_deref(),
            Some("HDR10 (PQ) · MaxCLL 1000 cd/m² · MaxFALL 400 cd/m²")
        );
        assert_eq!(color_name("bt.2020-ncl"), "BT.2020 NCL");
        let params = VideoParams {
            gamma: Some("pq".into()),
            max_luma: Some(1000.0),
            min_luma: Some(0.005),
            mastering: Some("Display P3"),
            ..VideoParams::default()
        };
        assert_eq!(
            hdr_in(Lang::Ru, &params, true).as_deref(),
            Some("HDR10+ (PQ) · мастеринг 0,0050–1000 кд/м², Display P3")
        );
    }

    #[test]
    fn chips() {
        let audio = |id, codec: &str, channels, lang: &str, selected| Track {
            id,
            kind: Kind::Audio,
            codec: Some(codec.into()),
            channels: Some(channels),
            lang: Some(lang.into()),
            selected,
            ..Track::default()
        };
        let mut state = State {
            idle: false,
            duration: Some(7683.0),
            file_size: Some(2_287_000_000),
            file_format: Some("mkv".into()),
            video_params: Some(VideoParams {
                pixel_format: Some("yuv420p10".into()),
                gamma: Some("pq".into()),
                ..VideoParams::default()
            }),
            tracks: vec![
                Track {
                    id: 1,
                    kind: Kind::Video,
                    codec: Some("hevc".into()),
                    codec_desc: Some("H.265 / HEVC".into()),
                    profile: Some("Main 10".into()),
                    width: Some(1920),
                    height: Some(1080),
                    fps: Some(23.976),
                    selected: true,
                    ..Track::default()
                },
                audio(1, "aac", 6, "rus", true),
                audio(2, "ac3", 2, "eng", false),
                Track {
                    id: 1,
                    kind: Kind::Sub,
                    lang: Some("rus".into()),
                    ..Track::default()
                },
                Track {
                    id: 2,
                    kind: Kind::Sub,
                    lang: Some("eng".into()),
                    selected: true,
                    ..Track::default()
                },
            ],
            ..State::default()
        };
        let path = Path::new(r"D:\Films\La La Land.mkv");
        let rows = chips_in(
            Lang::Ru,
            &Subject {
                path: Some(path),
                state: &state,
                facts: None,
            },
        );
        let texts: Vec<Vec<&str>> = rows
            .iter()
            .map(|r| r.iter().map(|c| c.text.as_str()).collect())
            .collect();
        assert_eq!(
            texts,
            [
                vec!["MKV", "2,13 ГБ", "2:08:03", "2,4 Мбит/с"],
                vec!["HEVC", "1920×1080", "23,976 fps", "10 бит", "HDR (PQ)"],
                vec![
                    "AAC 5.1 Русский",
                    "AC3 2.0 Английский",
                    "Субтитры: Английский (2/2)"
                ],
            ]
        );
        assert_eq!(rows[0][0].tip.as_deref(), Some("Matroska"));
        let mkv = crate::filetypes::of_extension("MKV").unwrap().colors;
        assert_eq!(rows[0][0].tone, Tone::Format(mkv));
        assert_eq!(rows[0][1].tone, Tone::Plain);
        assert!(rows[1].iter().all(|c| c.tone == Tone::Picture));
        assert_eq!(rows[2][0].tone, Tone::Sound);
        assert_eq!(rows[2][2].tone, Tone::Subtitles);
        assert_eq!(rows[1][0].tip.as_deref(), Some("H.265 / HEVC · Main 10"));
        assert!(rows[2][0].selected && !rows[2][1].selected);
        assert_eq!(rows[2][1].action, ChipAction::Audio(2));
        assert_eq!(rows[2][2].action, ChipAction::None);
        assert_eq!(
            rows[2][2].tip.as_deref(),
            Some("   1. Русский\n● 2. Английский")
        );

        // Many audio tracks fold into "+N"; no subtitles chosen: the count.
        state.tracks.truncate(1);
        for i in 1..=6 {
            state.tracks.push(audio(i, "aac", 2, "eng", i == 6));
        }
        state.tracks.push(Track {
            id: 1,
            kind: Kind::Sub,
            ..Track::default()
        });
        let rows = chips_in(
            Lang::En,
            &Subject {
                path: Some(path),
                state: &state,
                facts: None,
            },
        );
        let sound: Vec<&str> = rows[2].iter().map(|c| c.text.as_str()).collect();
        assert_eq!(
            sound,
            [
                "AAC 2.0 English",
                "AAC 2.0 English",
                "AAC 2.0 English",
                "+3",
                "Subtitles: 1"
            ]
        );
        assert!(rows[2][3].selected, "the chosen track is among the folded");
        assert_eq!(
            rows[2][3].tip.as_deref(),
            Some("4. English\n5. English\n6. English")
        );

        // An address has no extension to show: the container's name.
        state.file_format = Some("mpegts".into());
        let rows = chips_in(
            Lang::En,
            &Subject {
                path: Some(Path::new("https://x.invalid/live.ts")),
                state: &state,
                facts: None,
            },
        );
        assert_eq!(rows[0][0].text, "MPEG-TS");
        assert_eq!(rows[0][0].tone, Tone::Plain);
    }
}
