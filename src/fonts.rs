//! The system's font families for the subtitle and OSD settings. They are
//! listed with fontdb on a thread of its own the first time the settings
//! ask for them, never at start-up. The subtitles take a family by its name
//! (mpv finds it); the OSD, drawn by egui, needs a file: the family's
//! regular face, kept in the settings, so that start-up reads just that one
//! file.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};

/// A system font family and its regular face.
#[derive(Clone, Debug)]
pub struct Family {
    pub name: String,
    pub file: PathBuf,
    /// The face's number in a collection (`.ttc`).
    pub index: u32,
}

/// The installed families, sorted by name; `None` while they are being
/// listed (the first call starts that; the UI is repainted when done).
pub fn families(ctx: &egui::Context) -> Option<&'static [Family]> {
    static LIST: OnceLock<Vec<Family>> = OnceLock::new();
    static STARTED: AtomicBool = AtomicBool::new(false);
    if let Some(list) = LIST.get() {
        return Some(list);
    }
    if !STARTED.swap(true, Ordering::Relaxed) {
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let started = std::time::Instant::now();
            let list = system_families();
            log::debug!(
                "{} font families listed in {:.0} ms",
                list.len(),
                started.elapsed().as_secs_f64() * 1000.0
            );
            let _ = LIST.set(list);
            ctx.request_repaint();
        });
    }
    None
}

/// Reads the header of every font file of the system and of the user (tens
/// of milliseconds, more with a cold disk): for a thread of its own.
fn system_families() -> Vec<Family> {
    use fontdb::{Language, Source, Style};
    let mut db = fontdb::Database::new();
    db.load_system_fonts();
    // The face nearest to an upright regular, per family.
    let mut best: BTreeMap<String, (u32, Family)> = BTreeMap::new();
    for face in db.faces() {
        let file = match &face.source {
            Source::File(path) | Source::SharedFile(path, _) => path.clone(),
            Source::Binary(_) => continue,
        };
        let name = face
            .families
            .iter()
            .find(|(_, language)| *language == Language::English_UnitedStates)
            .or_else(|| face.families.first())
            .map(|(name, _)| name.clone());
        // `@` names are the vertical forms of East Asian fonts.
        let Some(name) = name.filter(|n| !n.is_empty() && !n.starts_with('@')) else {
            continue;
        };
        let distance = u32::from(face.weight.0.abs_diff(400))
            + if face.style == Style::Normal { 0 } else { 1000 };
        let family = Family {
            name: name.clone(),
            file,
            index: face.index,
        };
        match best.get(&name) {
            Some((d, _)) if *d <= distance => {}
            _ => {
                best.insert(name, (distance, family));
            }
        }
    }
    let mut list: Vec<Family> = best.into_values().map(|(_, f)| f).collect();
    list.sort_by_key(|f| f.name.to_lowercase());
    list
}

/// Face `index` of `file` for egui, if it is a font egui can draw (it
/// panics on one it cannot parse) with Latin letters.
pub fn read(file: &Path, index: u32) -> Option<egui::FontData> {
    use skrifa::MetadataProvider;
    let bytes = std::fs::read(file)
        .map_err(|e| log::warn!("cannot read the font {}: {e}", file.display()))
        .ok()?;
    let usable = skrifa::FontRef::from_index(&bytes, index)
        .is_ok_and(|font| font.charmap().map('A').is_some());
    if !usable {
        log::warn!("not a usable font: {} #{index}", file.display());
        return None;
    }
    let mut data = egui::FontData::from_owned(bytes);
    data.index = index;
    Some(data)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_fonts_are_listed_and_read() {
        let families = system_families();
        let arial = families
            .iter()
            .find(|f| f.name == "Arial")
            .expect("Arial is part of Windows");
        assert!(read(&arial.file, arial.index).is_some());
        assert!(
            families
                .windows(2)
                .all(|w| w[0].name.to_lowercase() <= w[1].name.to_lowercase())
        );
    }

    #[test]
    fn a_file_that_is_not_a_font_is_refused() {
        assert!(read(Path::new("Cargo.toml"), 0).is_none());
        assert!(read(Path::new("no_such_font.ttf"), 0).is_none());
    }
}
