//! Settings kept between runs in eframe's storage
//! (`%APPDATA%\MetalMPV\data\app.ron`, or [`PORTABLE_FILE`] next to the exe,
//! see [`location`]). Window size and position and egui's own state (panel
//! widths) are saved by eframe itself (`persist_window`).
//!
//! Plain strings through `eframe::Storage`: no serde derives needed, and an
//! unknown or malformed value falls back to its default, so settings of an
//! older or newer version never make the file unreadable.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use crate::format::TimeMode;
use crate::i18n::LangChoice;

const LANGUAGE: &str = "language";
const CHECK_UPDATES: &str = "check_updates";
const LAST_UPDATE_CHECK: &str = "last_update_check";
const VOLUME: &str = "volume";
const MUTE: &str = "mute";
const SEEK_SHORT: &str = "seek_short";
const SEEK_LONG: &str = "seek_long";
const HWDEC: &str = "hwdec";
const FILL_PLAYLIST: &str = "fill_playlist_from_folder";
const PLAYLIST_VISIBLE: &str = "playlist_visible";
const CHAPTER_MARKS: &str = "chapter_marks";
const TIME_DISPLAY: &str = "time_display";
const WINDOW_NORMAL: &str = "window_normal";

/// The longest seek step the settings accept, in seconds.
pub const MAX_STEP: f64 = 3600.0;

/// Settings file that makes a copy portable: when it lies next to the exe
/// (even empty) and can be written, the settings are kept in it instead of
/// the user profile. The portable zip ships an empty one
/// (`PORTABLE_SETTINGS` in tools/make_release.py).
pub const PORTABLE_FILE: &str = "metalmpv.ron";

/// Where the settings of this run are kept.
#[derive(Debug)]
pub struct Location {
    /// The settings file, `None` if there is nowhere to keep it.
    pub file: Option<PathBuf>,
    /// `file` is [`PORTABLE_FILE`] next to the exe.
    pub portable: bool,
}

impl Location {
    /// Whether settings of an earlier run are there (a portable file starts
    /// empty). `NativeOptions::centered` is set only when there are none:
    /// eframe would otherwise re-centre a restored window.
    pub fn has_saved(&self) -> bool {
        self.file
            .as_ref()
            .is_some_and(|f| f.metadata().is_ok_and(|m| m.len() > 0))
    }
}

/// Where the settings are kept, decided once per run.
pub fn location() -> &'static Location {
    static LOCATION: OnceLock<Location> = OnceLock::new();
    LOCATION.get_or_init(|| {
        let exe_dir = std::env::current_exe()
            .ok()
            .and_then(|e| e.parent().map(Path::to_path_buf));
        match exe_dir.and_then(|d| portable_file_in(&d)) {
            Some(file) => Location {
                file: Some(file),
                portable: true,
            },
            None => Location {
                file: eframe::storage_dir(crate::APP_ID).map(|d| d.join("app.ron")),
                portable: false,
            },
        }
    })
}

/// [`PORTABLE_FILE`] in `dir` if it is there and can be written; a copy on
/// a read-only medium keeps its settings in the profile instead of losing
/// them.
fn portable_file_in(dir: &Path) -> Option<PathBuf> {
    let file = dir.join(PORTABLE_FILE);
    // Opening for appending checks the permission without changing the file.
    std::fs::OpenOptions::new()
        .append(true)
        .open(&file)
        .ok()
        .map(|_| file)
}

#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    /// Interface language, or the one of Windows.
    pub language: LangChoice,
    /// Ask GitHub for a newer release at start-up (off unless enabled).
    pub check_updates: bool,
    /// Unix seconds of the last update check that got an answer.
    pub last_update_check: u64,
    /// 0 to 100.
    pub volume: f64,
    pub mute: bool,
    /// The step of Left and Right, in seconds (keyframe seek).
    pub seek_short: f64,
    /// The step of Ctrl+Left and Ctrl+Right, in seconds (keyframe seek).
    pub seek_long: f64,
    /// Hardware decoding (`hwdec=auto-safe`), else software.
    pub hwdec: bool,
    /// Opening one file lists the media files of its folder.
    pub fill_playlist: bool,
    pub playlist_visible: bool,
    /// Marks at the chapters' starts on the seek bar.
    pub chapter_marks: bool,
    /// What the time in the control bar shows (a click changes it).
    pub time_mode: TimeMode,
    /// The window's restored rectangle in pixels (left, top, right,
    /// bottom) while it is maximized: eframe saves the maximized size.
    pub window_normal: Option<[i32; 4]>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            language: LangChoice::System,
            check_updates: false,
            last_update_check: 0,
            volume: 100.0,
            mute: false,
            seek_short: 10.0,
            seek_long: 60.0,
            hwdec: true,
            fill_playlist: false,
            playlist_visible: false,
            chapter_marks: true,
            time_mode: TimeMode::Both,
            window_normal: None,
        }
    }
}

impl Settings {
    /// Read settings; missing or unknown values keep their defaults.
    pub fn load(storage: &dyn eframe::Storage) -> Self {
        let d = Self::default();
        let get = |key| storage.get_string(key);
        let flag = |key, default| match get(key).as_deref() {
            Some("true") => true,
            Some("false") => false,
            _ => default,
        };
        let number = |key, range: std::ops::RangeInclusive<f64>, default| {
            get(key)
                .and_then(|v| v.parse::<f64>().ok())
                .filter(|v| range.contains(v))
                .unwrap_or(default)
        };
        Self {
            language: get(LANGUAGE)
                .as_deref()
                .map_or(d.language, LangChoice::from_name),
            check_updates: flag(CHECK_UPDATES, d.check_updates),
            last_update_check: get(LAST_UPDATE_CHECK)
                .and_then(|v| v.parse().ok())
                .unwrap_or(d.last_update_check),
            volume: number(VOLUME, 0.0..=100.0, d.volume),
            mute: flag(MUTE, d.mute),
            seek_short: number(SEEK_SHORT, 0.1..=MAX_STEP, d.seek_short),
            seek_long: number(SEEK_LONG, 0.1..=MAX_STEP, d.seek_long),
            hwdec: flag(HWDEC, d.hwdec),
            fill_playlist: flag(FILL_PLAYLIST, d.fill_playlist),
            playlist_visible: flag(PLAYLIST_VISIBLE, d.playlist_visible),
            chapter_marks: flag(CHAPTER_MARKS, d.chapter_marks),
            time_mode: get(TIME_DISPLAY)
                .as_deref()
                .and_then(TimeMode::from_name)
                .unwrap_or(d.time_mode),
            window_normal: get(WINDOW_NORMAL).and_then(|v| {
                let n: Vec<i32> = v.split(',').filter_map(|x| x.trim().parse().ok()).collect();
                <[i32; 4]>::try_from(n)
                    .ok()
                    .filter(|r| r[2] > r[0] && r[3] > r[1])
            }),
        }
    }

    pub fn save(&self, storage: &mut dyn eframe::Storage) {
        storage.set_string(LANGUAGE, self.language.name().into());
        storage.set_string(CHECK_UPDATES, self.check_updates.to_string());
        storage.set_string(LAST_UPDATE_CHECK, self.last_update_check.to_string());
        storage.set_string(VOLUME, format!("{:.0}", self.volume));
        storage.set_string(MUTE, self.mute.to_string());
        storage.set_string(SEEK_SHORT, self.seek_short.to_string());
        storage.set_string(SEEK_LONG, self.seek_long.to_string());
        storage.set_string(HWDEC, self.hwdec.to_string());
        storage.set_string(FILL_PLAYLIST, self.fill_playlist.to_string());
        storage.set_string(PLAYLIST_VISIBLE, self.playlist_visible.to_string());
        storage.set_string(CHAPTER_MARKS, self.chapter_marks.to_string());
        storage.set_string(TIME_DISPLAY, self.time_mode.name().into());
        match self.window_normal {
            Some(r) => storage.set_string(
                WINDOW_NORMAL,
                format!("{},{},{},{}", r[0], r[1], r[2], r[3]),
            ),
            None => storage.remove_string(WINDOW_NORMAL),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use eframe::Storage;
    use std::collections::HashMap;

    #[derive(Default)]
    struct MemStorage(HashMap<String, String>);

    impl Storage for MemStorage {
        fn get_string(&self, key: &str) -> Option<String> {
            self.0.get(key).cloned()
        }
        fn set_string(&mut self, key: &str, value: String) {
            self.0.insert(key.into(), value);
        }
        fn remove_string(&mut self, key: &str) {
            self.0.remove(key);
        }
        fn flush(&mut self) {}
    }

    #[test]
    fn round_trip() {
        let s = Settings {
            language: LangChoice::Ru,
            check_updates: true,
            last_update_check: 1_790_000_000,
            volume: 35.0,
            mute: true,
            seek_short: 5.0,
            seek_long: 30.5,
            hwdec: false,
            fill_playlist: true,
            playlist_visible: true,
            chapter_marks: false,
            time_mode: TimeMode::Remaining,
            window_normal: Some([10, 20, 1010, 720]),
        };
        let mut storage = MemStorage::default();
        s.save(&mut storage);
        assert_eq!(Settings::load(&storage), s);
    }

    #[test]
    fn missing_or_unknown_values_use_defaults() {
        assert_eq!(Settings::load(&MemStorage::default()), Settings::default());
        let mut storage = MemStorage::default();
        storage.set_string(LANGUAGE, "de".into());
        storage.set_string(CHECK_UPDATES, "yes".into());
        storage.set_string(LAST_UPDATE_CHECK, "soon".into());
        storage.set_string(VOLUME, "150".into());
        storage.set_string(SEEK_SHORT, "0".into());
        storage.set_string(SEEK_LONG, "NaN".into());
        storage.set_string(HWDEC, "maybe".into());
        storage.set_string(TIME_DISPLAY, "total".into());
        storage.set_string(WINDOW_NORMAL, "1,2,3".into());
        assert_eq!(Settings::load(&storage), Settings::default());
    }

    #[test]
    fn portable_file_only_when_present_and_writable() {
        let dir = std::env::temp_dir().join(format!("portable_test_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join(PORTABLE_FILE);
        assert_eq!(portable_file_in(&dir), None);
        std::fs::write(&file, "").unwrap();
        assert_eq!(portable_file_in(&dir), Some(file.clone()));
        let mut perm = std::fs::metadata(&file).unwrap().permissions();
        perm.set_readonly(true);
        std::fs::set_permissions(&file, perm.clone()).unwrap();
        assert_eq!(portable_file_in(&dir), None);
        #[allow(clippy::permissions_set_readonly_false)]
        perm.set_readonly(false);
        std::fs::set_permissions(&file, perm).unwrap();
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
