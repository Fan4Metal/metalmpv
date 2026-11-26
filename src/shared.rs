//! Several copies of the player at once (`settings.several_copies`, or an
//! installed copy beside a development build) keep the same settings file,
//! and each writes all of it every few seconds: the last to write would
//! undo what the others changed. So each copy writes what it changed
//! itself since it last saved, and takes the rest from the file when
//! another copy wrote it last (`WRITER`, the process id); the positions
//! where files were left are merged file by file.

use std::collections::HashMap;
use std::path::Path;

use crate::resume::Positions;

/// The process id of the copy that wrote the settings file last.
pub const WRITER: &str = "writer";
/// The setting read before the window is made (`Settings::several_copies`).
const SEVERAL_COPIES: &str = "several_copies";

/// The settings file's keys and values, `None` when it does not read (cut
/// short, or being written now).
fn read(file: &Path) -> Option<HashMap<String, String>> {
    let bytes = std::fs::read(file).ok()?;
    ron::from_str(&String::from_utf8_lossy(&bytes)).ok()
}

/// Whether each start is to open a window of its own: read at start-up,
/// before eframe reads the settings (`instance::forward` comes first). A
/// file that does not read is taken from its copy (`settings::keep_backup`).
pub fn several_copies() -> bool {
    let Some(file) = &crate::settings::location().file else {
        return false;
    };
    read(file)
        .or_else(|| read(&file.with_extension("ron.bak")))
        .and_then(|kv| kv.get(SEVERAL_COPIES).cloned())
        .is_some_and(|v| v == "true")
}

/// Passes the app's writes on to eframe's storage and notes them.
pub struct Recorder<'a> {
    inner: &'a mut dyn eframe::Storage,
    written: HashMap<String, Option<String>>,
}

impl<'a> Recorder<'a> {
    pub fn new(inner: &'a mut dyn eframe::Storage) -> Self {
        Self {
            inner,
            written: HashMap::new(),
        }
    }

    /// What was written: the value of each key, `None` for one removed.
    pub fn into_written(self) -> Written {
        self.written
    }
}

/// The keys written by a [`Recorder`].
pub type Written = HashMap<String, Option<String>>;

impl eframe::Storage for Recorder<'_> {
    fn get_string(&self, key: &str) -> Option<String> {
        self.inner.get_string(key)
    }
    fn set_string(&mut self, key: &str, value: String) {
        self.written.insert(key.into(), Some(value.clone()));
        self.inner.set_string(key, value);
    }
    fn remove_string(&mut self, key: &str) {
        self.written.insert(key.into(), None);
        self.inner.remove_string(key);
    }
    fn flush(&mut self) {}
}

/// A storage that keeps nothing, for the writes of the settings as loaded.
struct Nowhere;

impl eframe::Storage for Nowhere {
    fn get_string(&self, _: &str) -> Option<String> {
        None
    }
    fn set_string(&mut self, _: &str, _: String) {}
    fn remove_string(&mut self, _: &str) {}
    fn flush(&mut self) {}
}

/// What this copy wrote when it last saved.
pub struct Shared {
    /// The app's own values (before any taken from the file).
    produced: Written,
    /// The positions written.
    positions: Positions,
}

impl Shared {
    /// `write` writes the settings as loaded: nothing is changed yet.
    pub fn new(write: impl FnOnce(&mut dyn eframe::Storage), positions: &Positions) -> Self {
        let mut nowhere = Nowhere;
        let mut recorder = Recorder::new(&mut nowhere);
        write(&mut recorder);
        Self {
            produced: recorder.into_written(),
            positions: Positions::parse(&positions.to_text()),
        }
    }

    /// The settings file when another copy wrote it last.
    pub fn others_file() -> Option<HashMap<String, String>> {
        let kv = read(crate::settings::location().file.as_ref()?)?;
        let ours = std::process::id().to_string();
        kv.get(WRITER).is_some_and(|w| *w != ours).then_some(kv)
    }

    /// Takes into `positions` the files another copy changed and this one
    /// did not, from `file` (`others_file`).
    pub fn merge_positions(
        &self,
        positions: &mut Positions,
        file: Option<&HashMap<String, String>>,
        key: &str,
    ) {
        if let Some(file) = file {
            let theirs = Positions::parse(file.get(key).map_or("", String::as_str));
            if positions.merge(&self.positions, &theirs) {
                log::debug!("positions taken from another copy");
            }
        }
    }

    /// After the app wrote `written` (through a `Recorder`) and `positions` into
    /// `storage`: the values it did not change since it last saved are
    /// taken from `file` (`others_file`), and the file is marked as this
    /// copy's.
    pub fn finish(
        &mut self,
        storage: &mut dyn eframe::Storage,
        written: Written,
        positions: &Positions,
        file: Option<&HashMap<String, String>>,
    ) {
        if let Some(file) = file {
            for (key, value) in &written {
                let unchanged = self.produced.get(key) == Some(value);
                if let Some(theirs) = file.get(key).filter(|t| Some(*t) != value.as_ref())
                    && unchanged
                {
                    log::debug!("{key} taken from another copy");
                    storage.set_string(key, theirs.clone());
                }
            }
        }
        self.produced = written;
        self.positions = Positions::parse(&positions.to_text());
        storage.set_string(WRITER, std::process::id().to_string());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use eframe::Storage as _;

    #[derive(Default)]
    struct Mem(HashMap<String, String>);

    impl eframe::Storage for Mem {
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
    fn a_copy_keeps_what_it_changed_and_takes_the_rest() {
        let positions = Positions::default();
        let loaded = |s: &mut dyn eframe::Storage| {
            s.set_string("volume", "70".into());
            s.set_string("theme", "black".into());
        };
        let mut shared = Shared::new(loaded, &positions);
        // Here the volume changed; another copy changed both.
        let file: HashMap<String, String> = [
            ("volume", "20"),
            ("theme", "plum"),
            (WRITER, "1"),
            ("new", "x"),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
        let mut storage = Mem::default();
        let save = |storage: &mut Mem, shared: &mut Shared, theme: &str| {
            let mut recorder = Recorder::new(storage);
            recorder.set_string("volume", "90".into());
            recorder.set_string("theme", theme.into());
            let written = recorder.into_written();
            shared.finish(storage, written, &positions, Some(&file));
        };
        save(&mut storage, &mut shared, "black");
        assert_eq!(storage.get_string("volume").as_deref(), Some("90"));
        assert_eq!(storage.get_string("theme").as_deref(), Some("plum"));
        assert_eq!(storage.get_string("new"), None);
        assert_eq!(
            storage.get_string(WRITER),
            Some(std::process::id().to_string())
        );
        // Taken once, the theme stays the other copy's until changed here.
        save(&mut storage, &mut shared, "black");
        assert_eq!(storage.get_string("theme").as_deref(), Some("plum"));
        save(&mut storage, &mut shared, "forest");
        assert_eq!(storage.get_string("theme").as_deref(), Some("forest"));
    }
}
