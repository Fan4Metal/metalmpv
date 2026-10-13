//! Registering MetalMPV with Windows for its video and audio types, per
//! user (under HKEY_CURRENT_USER, no administrator rights), as qview does:
//!
//! - a ProgID per format, `metalmpv.<id>` (`filetypes::FILE_TYPES`), with
//!   its name, its icon from the executable and the open command;
//! - the ProgID in each extension's `OpenWithProgids`, so that MetalMPV is
//!   offered in "Open with" and Windows asks, on the next open, whether it
//!   should be used from now on;
//! - `Applications\metalmpv.exe` and `Capabilities` listed in
//!   `RegisteredApplications`, so that MetalMPV appears in Settings →
//!   Default apps.
//!
//! Windows does not let a program make itself the default: the choice
//! (`UserChoice`) is protected and belongs to the user. So `register` only
//! offers MetalMPV, and `open_default_apps` opens the page where the user
//! picks it. An extension no program handles at all (no default ProgID, no
//! user choice, no other program in `OpenWithProgids`) gets MetalMPV's
//! ProgID as its default, the documented way of claiming an unclaimed
//! extension.
//!
//! When MetalMPV was picked in "Open with" before it was registered for a
//! type, Explorer made a ProgID of its own for it (`MKV_auto_file`), with
//! the command and nothing else, and keeps choosing it, even when MetalMPV
//! is picked again: the files show the app's icon and "MKV File".
//! Registering gives such ProgIDs the icon and the name of MetalMPV's type.

use std::path::Path;

use crate::filetypes::{FILE_TYPES, icon_id};

/// Where the keys go, under HKEY_CURRENT_USER. Tests use a scratch key.
struct Roots {
    classes: String,
    software: String,
    registered_apps: String,
    /// Whether other programs' registrations are read from the real
    /// HKEY_CLASSES_ROOT and the user's choices (tests: only `classes`).
    system: bool,
}

impl Roots {
    fn real() -> Self {
        Self {
            classes: r"Software\Classes".into(),
            software: r"Software\MetalMPV".into(),
            registered_apps: r"Software\RegisteredApplications".into(),
            system: true,
        }
    }
}

/// The name in "Open with", Default apps and `RegisteredApplications`.
const APP_NAME: &str = crate::APP_NAME;
const APP_KEY: &str = r"Applications\metalmpv.exe";
/// What the icon resources are referenced by, in lower case.
const EXE_NAME: &str = "metalmpv.exe";

fn prog_id(id: &str) -> String {
    format!("metalmpv.{id}")
}

/// The command Explorer runs to open a file with `exe`.
fn open_command(exe: &Path) -> String {
    format!("\"{}\" \"%1\"", exe.display())
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Status {
    NotRegistered,
    /// Registered for this executable.
    Registered,
    /// Registered for another copy, at this path.
    OtherCopy(String),
}

/// Register `exe` (the running MetalMPV) for all its types.
pub fn register(exe: &Path) -> Result<(), String> {
    register_in(&Roots::real(), exe)?;
    notify_shell();
    Ok(())
}

/// Remove everything `register` wrote. A type whose default was MetalMPV
/// goes back to asking which program to use.
pub fn unregister() -> Result<(), String> {
    unregister_in(&Roots::real())?;
    notify_shell();
    Ok(())
}

/// Whether MetalMPV is registered, and for which copy.
pub fn status(exe: &Path) -> Status {
    status_in(&Roots::real(), exe)
}

/// Which program opens a type by default.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Opener {
    /// Another program, or none chosen.
    Other,
    /// MetalMPV, through its ProgID for the type.
    Us,
    /// This MetalMPV, through a ProgID of another name: one Explorer made
    /// when MetalMPV was picked in "Open with" before it was registered for
    /// the type (`MKV_auto_file`). The files lack the type's icon and name.
    UsElsewhere,
}

/// The user's choice for `.ext`, else its default ProgID.
fn effective_prog_id(ext: &str) -> Option<String> {
    let choice =
        format!(r"Software\Microsoft\Windows\CurrentVersion\Explorer\FileExts\.{ext}\UserChoice");
    reg::get(&choice, Some("ProgId"))
        .or_else(|| reg::get_in(reg::Root::Classes, &format!(".{ext}"), None))
        .filter(|p| !p.is_empty())
}

/// For each of `FILE_TYPES`: which program opens its main extension, `exe`
/// being this MetalMPV.
pub fn defaults(exe: Option<&Path>) -> Vec<Opener> {
    let exe = exe.map(|e| e.display().to_string().to_lowercase());
    FILE_TYPES
        .iter()
        .map(|t| match effective_prog_id(t.extensions[0]) {
            Some(p) if p.eq_ignore_ascii_case(&prog_id(t.id)) => Opener::Us,
            Some(p) => {
                let command = reg::get_in(
                    reg::Root::Classes,
                    &format!(r"{p}\shell\open\command"),
                    None,
                );
                match (command, &exe) {
                    (Some(c), Some(exe)) if c.to_lowercase().contains(exe.as_str()) => {
                        Opener::UsElsewhere
                    }
                    _ => Opener::Other,
                }
            }
            None => Opener::Other,
        })
        .collect()
}

/// ProgIDs other than MetalMPV's that `.ext` may open with: its default,
/// and with `r.system` the user's choice and those Explorer offers.
fn other_prog_ids(r: &Roots, ext: &str) -> Vec<String> {
    let mut found: Vec<String> = reg::get(&format!(r"{}\.{ext}", r.classes), None)
        .into_iter()
        .collect();
    if r.system {
        let explorer =
            format!(r"Software\Microsoft\Windows\CurrentVersion\Explorer\FileExts\.{ext}");
        found.extend(reg::get(&format!(r"{explorer}\UserChoice"), Some("ProgId")));
        found.extend(reg::value_names(
            reg::Root::User,
            &format!(r"{explorer}\OpenWithProgids"),
        ));
    }
    // Not MetalMPV's own, nor `Applications\metalmpv.exe`.
    found.retain(|p| {
        !p.is_empty() && !p.to_lowercase().starts_with("metalmpv.") && !p.contains('\\')
    });
    found.sort_unstable();
    found.dedup();
    found
}

/// Whether no program handles `.ext` but MetalMPV (`id`): no default ProgID
/// (or MetalMPV's), and with `r.system` no user choice and no other program
/// offered for it.
fn unclaimed(r: &Roots, ext: &str, id: &str) -> bool {
    let ours = |p: &String| p.is_empty() || p.eq_ignore_ascii_case(id);
    if !r.system {
        return reg::get(&format!(r"{}\.{ext}", r.classes), None).is_none_or(|p| ours(&p));
    }
    let explorer = format!(r"Software\Microsoft\Windows\CurrentVersion\Explorer\FileExts\.{ext}");
    let others = |names: Vec<String>| {
        names
            .iter()
            .all(|n| n.is_empty() || n.eq_ignore_ascii_case(id))
    };
    reg::get_in(reg::Root::Classes, &format!(".{ext}"), None).is_none_or(|p| ours(&p))
        && reg::get(&format!(r"{explorer}\UserChoice"), Some("ProgId")).is_none()
        && others(reg::value_names(
            reg::Root::Classes,
            &format!(r".{ext}\OpenWithProgids"),
        ))
        && others(reg::value_names(
            reg::Root::User,
            &format!(r"{explorer}\OpenWithProgids"),
        ))
}

/// Open Settings → Default apps (on Windows 11 at MetalMPV's own page).
pub fn open_default_apps() -> bool {
    crate::win::shell_open(format!(
        "ms-settings:defaultapps?registeredAppUser={APP_NAME}"
    ))
}

fn register_in(r: &Roots, exe: &Path) -> Result<(), String> {
    let command = open_command(exe);
    let exe_text = exe.display();
    let ru = crate::i18n::lang() == crate::i18n::Lang::Ru;
    let capabilities = format!(r"{}\Capabilities", r.software);
    let exe_lower = exe_text.to_string().to_lowercase();
    for (i, t) in FILE_TYPES.iter().enumerate() {
        let id = prog_id(t.id);
        let key = format!(r"{}\{id}", r.classes);
        let name = if ru { t.name_ru } else { t.name_en };
        let icon = format!("\"{exe_text}\",-{}", icon_id(i));
        reg::set(&key, None, name)?;
        reg::set(&format!(r"{key}\DefaultIcon"), None, &icon)?;
        reg::set(&format!(r"{key}\shell\open\command"), None, &command)?;
        for ext in t.extensions {
            if unclaimed(r, ext, &id) {
                reg::set(&format!(r"{}\.{ext}", r.classes), None, &id)?;
            }
            reg::set(
                &format!(r"{}\.{ext}\OpenWithProgids", r.classes),
                Some(&id),
                "",
            )?;
            // Explorer's own ProgIDs that run this MetalMPV, in the user's
            // classes: the type's icon and name.
            for p in other_prog_ids(r, ext) {
                let other = format!(r"{}\{p}", r.classes);
                let command = reg::get(&format!(r"{other}\shell\open\command"), None);
                if command.is_some_and(|c| c.to_lowercase().contains(&exe_lower)) {
                    reg::set(&other, None, name)?;
                    reg::set(&format!(r"{other}\DefaultIcon"), None, &icon)?;
                }
            }
            reg::set(
                &format!(r"{}\{APP_KEY}\SupportedTypes", r.classes),
                Some(&format!(".{ext}")),
                "",
            )?;
            reg::set(
                &format!(r"{capabilities}\FileAssociations"),
                Some(&format!(".{ext}")),
                &id,
            )?;
        }
    }
    let app = format!(r"{}\{APP_KEY}", r.classes);
    reg::set(&app, Some("FriendlyAppName"), APP_NAME)?;
    reg::set(&format!(r"{app}\shell\open\command"), None, &command)?;
    reg::set(&capabilities, Some("ApplicationName"), APP_NAME)?;
    // The sentence of About and the README.
    reg::set(
        &capabilities,
        Some("ApplicationDescription"),
        if ru {
            "Быстрый видеоплеер для Windows на основе libmpv."
        } else {
            "A fast video player for Windows built on libmpv."
        },
    )?;
    reg::set(
        &capabilities,
        Some("ApplicationIcon"),
        &format!("\"{exe_text}\",0"),
    )?;
    reg::set(&r.registered_apps, Some(APP_NAME), &capabilities)?;
    Ok(())
}

fn unregister_in(r: &Roots) -> Result<(), String> {
    for (i, t) in FILE_TYPES.iter().enumerate() {
        let id = prog_id(t.id);
        reg::delete_tree(&format!(r"{}\{id}", r.classes))?;
        for ext in t.extensions {
            // The icon and name given to Explorer's ProgIDs; their command
            // stays.
            for p in other_prog_ids(r, ext) {
                let other = format!(r"{}\{p}", r.classes);
                let ours = |icon: String| {
                    icon.to_lowercase().contains(EXE_NAME)
                        && icon.ends_with(&format!(",-{}", icon_id(i)))
                };
                if reg::get(&format!(r"{other}\DefaultIcon"), None).is_some_and(ours) {
                    reg::delete_tree(&format!(r"{other}\DefaultIcon"))?;
                    if reg::get(&other, None).is_some_and(|n| n == t.name_en || n == t.name_ru) {
                        reg::delete_value(&other, "")?;
                    }
                }
            }
            reg::delete_value(&format!(r"{}\.{ext}\OpenWithProgids", r.classes), &id)?;
            // The default it was given as an unclaimed extension.
            let key = format!(r"{}\.{ext}", r.classes);
            if reg::get(&key, None).is_some_and(|p| p.eq_ignore_ascii_case(&id)) {
                reg::delete_value(&key, "")?;
            }
        }
    }
    reg::delete_tree(&format!(r"{}\{APP_KEY}", r.classes))?;
    reg::delete_tree(&r.software)?;
    reg::delete_value(&r.registered_apps, APP_NAME)?;
    Ok(())
}

fn status_in(r: &Roots, exe: &Path) -> Status {
    match reg::get(
        &format!(r"{}\{APP_KEY}\shell\open\command", r.classes),
        None,
    ) {
        None => Status::NotRegistered,
        Some(c) if c.eq_ignore_ascii_case(&open_command(exe)) => Status::Registered,
        // `"path" "%1"`: the path between the first pair of quotes.
        Some(c) => Status::OtherCopy(c.split('"').nth(1).unwrap_or(&c).to_string()),
    }
}

/// Tell Explorer that associations changed, so that it shows the new
/// icons and names without a restart.
fn notify_shell() {
    use windows_sys::Win32::UI::Shell::{SHCNE_ASSOCCHANGED, SHCNF_IDLIST, SHChangeNotify};
    unsafe {
        SHChangeNotify(
            SHCNE_ASSOCCHANGED as i32,
            SHCNF_IDLIST,
            std::ptr::null(),
            std::ptr::null(),
        )
    };
}

/// String values under HKEY_CURRENT_USER (written), and read from
/// HKEY_CLASSES_ROOT too.
mod reg {
    use windows_sys::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS};
    use windows_sys::Win32::System::Registry::{
        HKEY, HKEY_CLASSES_ROOT, HKEY_CURRENT_USER, KEY_READ, REG_SZ, RRF_RT_REG_SZ, RegCloseKey,
        RegDeleteKeyValueW, RegDeleteTreeW, RegEnumValueW, RegGetValueW, RegOpenKeyExW,
        RegSetKeyValueW,
    };

    use crate::win::wide;

    /// Where a value is read from.
    #[derive(Clone, Copy)]
    pub enum Root {
        User,
        /// HKEY_CLASSES_ROOT: the user's classes over the machine's.
        Classes,
    }

    fn hkey(root: Root) -> HKEY {
        match root {
            Root::User => HKEY_CURRENT_USER,
            Root::Classes => HKEY_CLASSES_ROOT,
        }
    }

    fn name_ptr(name: &Option<Vec<u16>>) -> *const u16 {
        name.as_ref().map_or(std::ptr::null(), |n| n.as_ptr())
    }

    fn check(code: u32, what: &str, key: &str) -> Result<(), String> {
        if code == ERROR_SUCCESS {
            Ok(())
        } else {
            Err(format!(
                "{what} {key}: {}",
                std::io::Error::from_raw_os_error(code as i32)
            ))
        }
    }

    /// Set value `name` (None: the key's default value) of `key`, creating
    /// the key if needed.
    pub fn set(key: &str, name: Option<&str>, value: &str) -> Result<(), String> {
        let (k, n, v) = (wide(key), name.map(wide), wide(value));
        let code = unsafe {
            RegSetKeyValueW(
                HKEY_CURRENT_USER,
                k.as_ptr(),
                name_ptr(&n),
                REG_SZ,
                v.as_ptr().cast(),
                (v.len() * 2) as u32,
            )
        };
        check(code, "cannot write", key)
    }

    /// Value `name` of `key`, if it is there.
    pub fn get(key: &str, name: Option<&str>) -> Option<String> {
        get_in(Root::User, key, name)
    }

    /// The names of the values of `key` (the default one as "").
    pub fn value_names(root: Root, key: &str) -> Vec<String> {
        let k = wide(key);
        let mut handle: HKEY = std::ptr::null_mut();
        if unsafe { RegOpenKeyExW(hkey(root), k.as_ptr(), 0, KEY_READ, &mut handle) }
            != ERROR_SUCCESS
        {
            return Vec::new();
        }
        let mut names = Vec::new();
        let mut buf = vec![0u16; 16384];
        for index in 0.. {
            let mut len = buf.len() as u32;
            let code = unsafe {
                RegEnumValueW(
                    handle,
                    index,
                    buf.as_mut_ptr(),
                    &mut len,
                    std::ptr::null(),
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                )
            };
            if code != ERROR_SUCCESS {
                break;
            }
            names.push(String::from_utf16_lossy(&buf[..len as usize]));
        }
        unsafe { RegCloseKey(handle) };
        names
    }

    /// Value `name` of `key` under `root`, if it is there.
    pub fn get_in(root: Root, key: &str, name: Option<&str>) -> Option<String> {
        let (k, n) = (wide(key), name.map(wide));
        let mut buf = vec![0u16; 1024];
        loop {
            let mut bytes = (buf.len() * 2) as u32;
            let code = unsafe {
                RegGetValueW(
                    hkey(root),
                    k.as_ptr(),
                    name_ptr(&n),
                    RRF_RT_REG_SZ,
                    std::ptr::null_mut(),
                    buf.as_mut_ptr().cast(),
                    &mut bytes,
                )
            };
            match code {
                ERROR_SUCCESS => {
                    let len = (bytes as usize / 2).saturating_sub(1);
                    return Some(String::from_utf16_lossy(&buf[..len]));
                }
                // ERROR_MORE_DATA: `bytes` holds the size needed.
                234 => buf.resize(bytes as usize / 2 + 1, 0),
                _ => return None,
            }
        }
    }

    /// Delete `key` with everything under it; a missing key is fine.
    pub fn delete_tree(key: &str) -> Result<(), String> {
        let k = wide(key);
        let code = unsafe { RegDeleteTreeW(HKEY_CURRENT_USER, k.as_ptr()) };
        if code == ERROR_FILE_NOT_FOUND {
            Ok(())
        } else {
            check(code, "cannot delete", key)
        }
    }

    /// Delete value `name` of `key`; a missing one is fine.
    pub fn delete_value(key: &str, name: &str) -> Result<(), String> {
        let (k, n) = (wide(key), wide(name));
        let code = unsafe { RegDeleteKeyValueW(HKEY_CURRENT_USER, k.as_ptr(), n.as_ptr()) };
        if code == ERROR_FILE_NOT_FOUND {
            Ok(())
        } else {
            check(code, "cannot delete", key)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Registration into a scratch key, never the real associations.
    #[test]
    fn register_and_unregister() {
        let root = format!(r"Software\metalmpv_test_{}", std::process::id());
        let r = Roots {
            classes: format!(r"{root}\Classes"),
            software: format!(r"{root}\Software\MetalMPV"),
            registered_apps: format!(r"{root}\RegisteredApplications"),
            system: false,
        };
        let exe = Path::new(r"C:\Program Files\MetalMPV\metalmpv.exe");
        assert_eq!(status_in(&r, exe), Status::NotRegistered);
        register_in(&r, exe).unwrap();
        assert_eq!(status_in(&r, exe), Status::Registered);
        assert_eq!(
            status_in(&r, Path::new(r"D:\other\metalmpv.exe")),
            Status::OtherCopy(r"C:\Program Files\MetalMPV\metalmpv.exe".into())
        );
        let get = |key: &str, name: Option<&str>| reg::get(&format!(r"{}\{key}", r.classes), name);
        assert_eq!(
            get(r"metalmpv.mp4\shell\open\command", None).unwrap(),
            r#""C:\Program Files\MetalMPV\metalmpv.exe" "%1""#
        );
        assert_eq!(
            get(r"metalmpv.mp4\DefaultIcon", None).unwrap(),
            r#""C:\Program Files\MetalMPV\metalmpv.exe",-101"#
        );
        assert_eq!(
            get(r".m4v\OpenWithProgids", Some("metalmpv.mp4")).unwrap(),
            ""
        );
        assert_eq!(
            get(r".m2ts\OpenWithProgids", Some("metalmpv.ts")).unwrap(),
            ""
        );
        assert_eq!(
            get(r".flac\OpenWithProgids", Some("metalmpv.flac")).unwrap(),
            ""
        );
        assert_eq!(
            reg::get(
                &format!(r"{}\Capabilities\FileAssociations", r.software),
                Some(".mkv")
            )
            .unwrap(),
            "metalmpv.mkv"
        );
        assert_eq!(
            reg::get(&r.registered_apps, Some(APP_NAME)).unwrap(),
            format!(r"{}\Capabilities", r.software)
        );

        // An unclaimed extension gets MetalMPV as its default; one another
        // program is the default of keeps it, before and after.
        assert_eq!(get(".wv", None).unwrap(), "metalmpv.wv");
        reg::delete_tree(&format!(r"{}\.mp4", r.classes)).unwrap();
        reg::set(&format!(r"{}\.mp4", r.classes), None, "Other.mp4").unwrap();
        register_in(&r, exe).unwrap();
        assert_eq!(get(".mp4", None).unwrap(), "Other.mp4");

        // A ProgID Explorer made for MetalMPV gets the type's icon and
        // name, removed again with the registration; one of another
        // program does not.
        let auto = format!(r"{}\MKV_auto_file", r.classes);
        reg::set(
            &format!(r"{auto}\shell\open\command"),
            None,
            &open_command(exe),
        )
        .unwrap();
        reg::set(&format!(r"{}\.mkv", r.classes), None, "MKV_auto_file").unwrap();
        let other = format!(r"{}\Other.avi", r.classes);
        reg::set(
            &format!(r"{other}\shell\open\command"),
            None,
            r#""C:\Other\other.exe" "%1""#,
        )
        .unwrap();
        reg::set(&format!(r"{}\.avi", r.classes), None, "Other.avi").unwrap();
        register_in(&r, exe).unwrap();
        assert_eq!(
            get(r"MKV_auto_file\DefaultIcon", None).unwrap(),
            r#""C:\Program Files\MetalMPV\metalmpv.exe",-102"#
        );
        assert!(get("MKV_auto_file", None).is_some());
        assert_eq!(get(r"Other.avi\DefaultIcon", None), None);
        assert_eq!(get(".mkv", None).unwrap(), "MKV_auto_file");

        // Another program's entry in OpenWithProgids survives.
        reg::set(
            &format!(r"{}\.mp3\OpenWithProgids", r.classes),
            Some("Other.mp3"),
            "",
        )
        .unwrap();
        unregister_in(&r).unwrap();
        assert_eq!(status_in(&r, exe), Status::NotRegistered);
        assert_eq!(get("metalmpv.mp4", None), None);
        assert_eq!(get(r".mp3\OpenWithProgids", Some("metalmpv.mp3")), None);
        assert_eq!(get(r".mp3\OpenWithProgids", Some("Other.mp3")).unwrap(), "");
        assert_eq!(reg::get(&r.registered_apps, Some(APP_NAME)), None);
        assert_eq!(get(".wv", None), None);
        assert_eq!(get(".mp4", None).unwrap(), "Other.mp4");
        assert_eq!(get(r"MKV_auto_file\DefaultIcon", None), None);
        assert_eq!(get("MKV_auto_file", None), None);
        assert_eq!(
            get(r"MKV_auto_file\shell\open\command", None).unwrap(),
            open_command(exe)
        );
        // Unregistering twice is fine.
        unregister_in(&r).unwrap();
        reg::delete_tree(&root).unwrap();
    }

    /// Which program opens each type, and which extensions registering
    /// would claim, on this machine (read only), with `--nocapture`.
    #[test]
    #[ignore]
    fn print_defaults() {
        let exe = std::env::var("METALMPV_EXE")
            .ok()
            .map(std::path::PathBuf::from);
        let r = Roots::real();
        for (t, opener) in FILE_TYPES.iter().zip(defaults(exe.as_deref())) {
            let claimed: Vec<&str> = t
                .extensions
                .iter()
                .copied()
                .filter(|e| unclaimed(&r, e, &prog_id(t.id)))
                .collect();
            println!("{:5} {opener:?}, unclaimed: {}", t.id, claimed.join(" "));
        }
    }

    /// Every extension the player opens is registered, once.
    #[test]
    fn every_media_extension_is_registered() {
        let mut registered: Vec<&str> = FILE_TYPES
            .iter()
            .flat_map(|t| t.extensions.iter().copied())
            .collect();
        let mut listed = [
            crate::playlist::MEDIA_EXTENSIONS,
            crate::playlist::PLAYLIST_EXTENSIONS,
        ]
        .concat();
        registered.sort_unstable();
        listed.sort_unstable();
        assert_eq!(registered, listed);
        // The icon IDs are the resources build.rs embeds.
        assert_eq!(icon_id(0), 101);
    }
}
