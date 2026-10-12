//! What the file's information window says (`ui/info.rs`): the tags in a
//! familiar order with their names in the interface's language, the file's
//! size, container and bitrate, each track's rows (what the file states
//! and, for the picture on screen, what the decoder says), and all of it as
//! plain text for the clipboard. Each function has an `_in(lang, …)` form,
//! as in `format.rs`.

use std::path::Path;

use crate::i18n::Lang;
use crate::player::{Scan, State, VideoParams};
use crate::tracks::{self, Kind, Track};

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

/// A bitrate: "320 kb/s", "8.5 Mb/s"; "8,5 Мбит/с".
pub fn bitrate_in(lang: Lang, bits_per_second: f64) -> String {
    let (k, m) = match lang {
        Lang::En => ("kb/s", "Mb/s"),
        Lang::Ru => ("кбит/с", "Мбит/с"),
    };
    if bits_per_second >= 1_000_000.0 {
        let mb = bits_per_second / 1_000_000.0;
        let decimals = if mb >= 100.0 { 0 } else { 1 };
        format!("{} {m}", decimal_in(lang, mb, decimals))
    } else {
        format!("{:.0} {k}", bits_per_second / 1000.0)
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
pub fn track_details_in(lang: Lang, track: &Track, state: &State) -> String {
    let ru = lang == Lang::Ru;
    let mut parts = Vec::new();
    if let Some(codec) = &track.codec {
        parts.push(tracks::codec_name(codec));
    }
    match track.kind {
        Kind::Video => {
            if let (Some(w), Some(h)) = (track.width, track.height) {
                parts.push(format!("{w}×{h}"));
            }
            if let Some(fps) = track.fps.filter(|f| *f > 0.0 && !track.albumart) {
                let decimals = if (fps - fps.round()).abs() < 0.001 {
                    0
                } else {
                    3
                };
                parts.push(format!("{} fps", decimal_in(lang, fps, decimals)));
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
            if let Some(n) = track.channels {
                parts.push(tracks::channels_in(lang, n));
            }
            if let Some(rate) = track.samplerate.filter(|r| *r > 0) {
                let khz = rate as f64 / 1000.0;
                let decimals = if rate % 1000 == 0 { 0 } else { 1 };
                let unit = if ru { "кГц" } else { "kHz" };
                parts.push(format!("{} {unit}", decimal_in(lang, khz, decimals)));
            }
        }
        Kind::Sub => {}
    }
    if let Some(bitrate) = track.bitrate.filter(|b| *b > 0) {
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
pub fn track_rows_in(lang: Lang, track: &Track, state: &State) -> Vec<(String, String)> {
    let ru = lang == Lang::Ru;
    let label = |en: &str, ru_text: &str| if ru { ru_text } else { en }.to_owned();
    let mut rows = Vec::new();
    if let Some(codec) = &track.codec {
        rows.push((label("Format", "Формат"), tracks::codec_name(codec)));
    }
    if let Some(profile) = &track.profile {
        rows.push((label("Profile", "Профиль"), profile.clone()));
    }
    // The decoder's word on the picture, for the track on screen only.
    let on_screen = track.kind == Kind::Video && track.selected && !track.albumart;
    let params = state.video_params.as_ref().filter(|_| on_screen);
    match track.kind {
        Kind::Video => {
            if let Some((profile, level)) = track.dolby_vision {
                let text = match (ru, level) {
                    (false, 0) => format!("profile {profile}"),
                    (true, 0) => format!("профиль {profile}"),
                    (false, l) => format!("profile {profile}, level {l}"),
                    (true, l) => format!("профиль {profile}, уровень {l}"),
                };
                rows.push(("Dolby Vision".into(), text));
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
                let decimals = if (fps - fps.round()).abs() < 0.001 {
                    0
                } else {
                    3
                };
                let text = format!("{} fps", decimal_in(lang, fps, decimals));
                rows.push((label("Frame rate", "Частота кадров"), text));
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
                if let Some(hdr) = hdr_in(lang, params) {
                    rows.push(("HDR".into(), hdr));
                }
            }
        }
        Kind::Audio => {
            if let Some(n) = track.channels {
                let mut text = tracks::channels_in(lang, n);
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
                let decimals = if rate % 1000 == 0 { 0 } else { 1 };
                let unit = if ru { "кГц" } else { "kHz" };
                rows.push((
                    label("Sample rate", "Частота дискретизации"),
                    format!("{} {unit}", decimal_in(lang, khz, decimals)),
                ));
            }
        }
        Kind::Sub => {}
    }
    if let Some(bitrate) = track.bitrate.filter(|b| *b > 0) {
        rows.push((
            label("Bitrate", "Битрейт"),
            bitrate_in(lang, bitrate as f64),
        ));
        if let (Some(w), Some(h), Some(fps)) = (track.width, track.height, track.fps) {
            let pixels = (w * h) as f64 * fps;
            if pixels > 0.0 && !track.albumart {
                rows.push((
                    label("Bits/(pixel·frame)", "Бит/(пиксель·кадр)"),
                    decimal_in(lang, bitrate as f64 / pixels, 3),
                ));
            }
        }
        if let Some(duration) = state.duration.filter(|_| !track.external) {
            let bytes = bitrate as f64 * duration / 8.0;
            let mut text = format!("≈ {}", size_in(lang, bytes as i64));
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
    rows
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
fn hdr_in(lang: Lang, params: &VideoParams) -> Option<String> {
    let kind = match params.gamma.as_deref() {
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
        parts.push(match lang {
            Lang::En => format!("mastering {v:.0} {nits}"),
            Lang::Ru => format!("мастеринг {v:.0} {nits}"),
        });
    }
    Some(parts.join(" · "))
}

/// The file's rows: name, folder, size, container, duration, bitrate.
pub fn file_rows_in(lang: Lang, path: Option<&Path>, state: &State) -> Vec<(String, String)> {
    let ru = lang == Lang::Ru;
    let label = |en: &str, ru_text: &str| if ru { ru_text } else { en }.to_owned();
    let mut rows = Vec::new();
    if let Some(path) = path {
        if let Some(name) = path.file_name() {
            rows.push((label("Name", "Имя"), name.to_string_lossy().into_owned()));
        }
        if let Some(dir) = path.parent() {
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

/// Everything the window shows, as text for the clipboard.
pub fn report_in(lang: Lang, path: Option<&Path>, state: &State) -> String {
    let ru = lang == Lang::Ru;
    let mut text = String::new();
    let mut section = |title: &str, rows: Vec<(String, String)>| {
        if rows.is_empty() {
            return;
        }
        if !text.is_empty() {
            text.push('\n');
        }
        text.push_str(title);
        text.push('\n');
        for (label, value) in rows {
            text.push_str(&format!("{label}: {value}\n"));
        }
    };
    section(
        if ru { "Файл" } else { "File" },
        file_rows_in(lang, path, state),
    );
    section(
        if ru { "Теги" } else { "Tags" },
        tags_in(lang, &state.metadata),
    );
    for t in &state.tracks {
        section(
            &track_heading_in(lang, t, &state.tracks),
            track_rows_in(lang, t, state),
        );
    }
    text
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
        track_rows_in(Lang::En, track, state)
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
            track_details_in(Lang::En, &video, &state),
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
            track_details_in(Lang::Ru, &audio, &state),
            "AAC · 5.1 · 48 кГц · 384 кбит/с"
        );
        let rows = track_rows_in(Lang::Ru, &audio, &state);
        assert_eq!(value(&rows, "Каналы"), Some("5.1 (5.1(side))"));
        assert_eq!(value(&rows, "Частота дискретизации"), Some("48 кГц"));
        assert_eq!(value(&rows, "Битрейт"), Some("384 кбит/с"));
        assert_eq!(value(&rows, "Признаки"), Some("комментарии"));
        assert_eq!(value(&rows, "Бит/(пиксель·кадр)"), None);
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
            hdr_in(Lang::En, &params).as_deref(),
            Some("HDR10 (PQ) · MaxCLL 1000 cd/m² · MaxFALL 400 cd/m²")
        );
        assert_eq!(color_name("bt.2020-ncl"), "BT.2020 NCL");
    }
}
