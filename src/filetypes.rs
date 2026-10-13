//! The file types MetalMPV registers in Windows (`assoc.rs`): one ProgID
//! per format, its extensions, its name in Explorer's Type column and the
//! colours of its icon, after GOM Player's: a dark band and a light ground
//! of one hue per family of formats. Dependency-free: `build.rs` includes
//! this file (with `type_icon.rs`) to draw the icons embedded in the
//! executable.

// build.rs uses the table only for the icons.
#![allow(dead_code)]

pub struct FileType {
    /// ProgID suffix: the type is registered as `metalmpv.<id>`.
    pub id: &'static str,
    /// Text on the icon.
    pub label: &'static str,
    /// Extensions, lowercase, without the dot; the first is the one whose
    /// default program the associations dialog shows.
    pub extensions: &'static [&'static str],
    /// Name in Explorer's Type column.
    pub name_en: &'static str,
    pub name_ru: &'static str,
    pub colors: Colors,
    pub kind: Kind,
}

/// What a type's files hold: the icon shows Play, a note or a list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Video,
    Audio,
    Playlist,
}

/// The icon's band (white letters on it) and the ground of its picture.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Colors {
    pub band: [u8; 3],
    pub light: [u8; 3],
}

// GOM Player's hues (its Icon.dll), the band and the light ground.
/// AVI, DivX.
const ORANGE: Colors = Colors {
    band: [0xd1, 0x35, 0x2a],
    light: [0xf5, 0x90, 0x81],
};
/// Windows Media.
const CRIMSON: Colors = Colors {
    band: [0x91, 0x27, 0x55],
    light: [0xcb, 0x7b, 0x99],
};
/// MPEG, DVD, transport streams.
const TEAL: Colors = Colors {
    band: [0x17, 0x86, 0x7e],
    light: [0x69, 0xba, 0xb6],
};
/// MP4, QuickTime, 3GP.
const LIME: Colors = Colors {
    band: [0x73, 0xa0, 0x49],
    light: [0xb7, 0xd6, 0x8a],
};
/// RealMedia.
const BROWN: Colors = Colors {
    band: [0x80, 0x3a, 0x38],
    light: [0xb2, 0x90, 0x8a],
};
/// Matroska.
const BLUE: Colors = Colors {
    band: [0x04, 0x56, 0x9b],
    light: [0x66, 0x9d, 0xc9],
};
/// Ogg.
const GREEN: Colors = Colors {
    band: [0x00, 0x86, 0x57],
    light: [0x66, 0xc8, 0x93],
};
/// Playlists: amber, a hue none of the media types has.
const AMBER: Colors = Colors {
    band: [0xc2, 0x7c, 0x0e],
    light: [0xf2, 0xbe, 0x5a],
};
/// Flash.
const PURPLE: Colors = Colors {
    band: [0x4e, 0x1f, 0x6b],
    light: [0xa8, 0x7c, 0xae],
};

const fn video(
    id: &'static str,
    label: &'static str,
    extensions: &'static [&'static str],
    name_en: &'static str,
    name_ru: &'static str,
    colors: Colors,
) -> FileType {
    FileType {
        id,
        label,
        extensions,
        name_en,
        name_ru,
        colors,
        kind: Kind::Video,
    }
}

const fn audio(
    id: &'static str,
    label: &'static str,
    extensions: &'static [&'static str],
    name_en: &'static str,
    name_ru: &'static str,
    colors: Colors,
) -> FileType {
    FileType {
        kind: Kind::Audio,
        ..video(id, label, extensions, name_en, name_ru, colors)
    }
}

/// Every extension of `playlist::MEDIA_EXTENSIONS` and
/// `PLAYLIST_EXTENSIONS`, once (tested). New types go at the end: a
/// type's icon is its place (`icon_id`), which Explorer remembers.
pub const FILE_TYPES: &[FileType] = &[
    video(
        "mp4",
        "MP4",
        &["mp4", "m4v"],
        "MPEG-4 Video",
        "Видео MPEG-4",
        LIME,
    ),
    video(
        "mkv",
        "MKV",
        &["mkv"],
        "Matroska Video",
        "Видео Matroska",
        BLUE,
    ),
    video("webm", "WEBM", &["webm"], "WebM Video", "Видео WebM", GREEN),
    video(
        "avi",
        "AVI",
        &["avi", "divx"],
        "AVI Video",
        "Видео AVI",
        ORANGE,
    ),
    video(
        "mov",
        "MOV",
        &["mov", "qt"],
        "QuickTime Video",
        "Видео QuickTime",
        LIME,
    ),
    video(
        "wmv",
        "WMV",
        &["wmv", "asf"],
        "Windows Media Video",
        "Видео Windows Media",
        CRIMSON,
    ),
    video(
        "flv",
        "FLV",
        &["flv", "f4v"],
        "Flash Video",
        "Видео Flash",
        PURPLE,
    ),
    video(
        "mpeg",
        "MPG",
        &["mpg", "mpeg", "m2v", "vob"],
        "MPEG Video",
        "Видео MPEG",
        TEAL,
    ),
    video(
        "ts",
        "TS",
        &["ts", "m2ts", "mts"],
        "MPEG-TS Video",
        "Видео MPEG-TS",
        TEAL,
    ),
    video(
        "3gp",
        "3GP",
        &["3gp", "3g2"],
        "3GP Video",
        "Видео 3GP",
        LIME,
    ),
    video("ogv", "OGV", &["ogv"], "Ogg Video", "Видео Ogg", GREEN),
    video(
        "rm",
        "RM",
        &["rm", "rmvb"],
        "RealMedia Video",
        "Видео RealMedia",
        BROWN,
    ),
    audio("mp3", "MP3", &["mp3"], "MP3 Audio", "Аудио MP3", ORANGE),
    audio("flac", "FLAC", &["flac"], "FLAC Audio", "Аудио FLAC", BLUE),
    audio(
        "m4a",
        "M4A",
        &["m4a", "aac"],
        "AAC Audio",
        "Аудио AAC",
        LIME,
    ),
    audio(
        "ogg",
        "OGG",
        &["ogg", "oga"],
        "Ogg Audio",
        "Аудио Ogg",
        GREEN,
    ),
    audio("opus", "OPUS", &["opus"], "Opus Audio", "Аудио Opus", GREEN),
    audio("wav", "WAV", &["wav"], "WAVE Audio", "Аудио WAVE", TEAL),
    audio(
        "wma",
        "WMA",
        &["wma"],
        "Windows Media Audio",
        "Аудио Windows Media",
        CRIMSON,
    ),
    audio(
        "ape",
        "APE",
        &["ape"],
        "Monkey's Audio",
        "Аудио Monkey's Audio",
        BROWN,
    ),
    audio("wv", "WV", &["wv"], "WavPack Audio", "Аудио WavPack", BROWN),
    audio(
        "mka",
        "MKA",
        &["mka"],
        "Matroska Audio",
        "Аудио Matroska",
        BLUE,
    ),
    audio(
        "ac3",
        "AC3",
        &["ac3"],
        "Dolby Digital Audio",
        "Аудио Dolby Digital",
        PURPLE,
    ),
    audio("dts", "DTS", &["dts"], "DTS Audio", "Аудио DTS", PURPLE),
    FileType {
        kind: Kind::Playlist,
        ..video(
            "m3u",
            "M3U",
            &["m3u", "m3u8"],
            "M3U Playlist",
            "Плейлист M3U",
            AMBER,
        )
    },
];

/// Resource ID of the icon of `FILE_TYPES[i]` in the executable (the app
/// icon is 1).
/// The type of files with `extension` (any case, without the dot).
pub fn of_extension(extension: &str) -> Option<&'static FileType> {
    let extension = extension.to_ascii_lowercase();
    FILE_TYPES
        .iter()
        .find(|t| t.extensions.contains(&extension.as_str()))
}

pub const fn icon_id(i: usize) -> u16 {
    101 + i as u16
}
