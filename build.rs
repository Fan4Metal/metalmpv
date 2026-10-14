//! Generates the application icon and embeds it (plus version info) into the
//! Windows executable, passes the version shown in the UI to the program,
//! and puts libmpv-2.dll (fetched by tools/get_libmpv.py) next to the exe.

#[path = "src/filetypes.rs"]
mod filetypes;
#[path = "src/icon.rs"]
mod icon;
#[path = "src/type_icon.rs"]
mod type_icon;

fn main() {
    println!("cargo:rerun-if-changed=src/icon.rs");
    println!("cargo:rerun-if-changed=src/filetypes.rs");
    println!("cargo:rerun-if-changed=src/type_icon.rs");
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rustc-env=METALMPV_VERSION={}", version());

    let out_dir = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap());
    copy_libmpv(&out_dir);
    // The script is rerun at every commit (the version): the icons are
    // drawn again only when what draws them changed (a stamp of their
    // sources' hash) or a file is missing.
    let stamp = out_dir.join(format!("icons_{:016x}.stamp", icon_sources_hash()));
    let windows = std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows");
    let drawn = stamp.exists();
    // The icon as raw RGBA for the window icon and the start screen (see
    // `main::embedded_icon`), so that nothing big is rasterised at start-up.
    for size in [64u32, 80, 96, 112, 128, 140, 168, 196, 224] {
        let path = out_dir.join(format!("app_icon_{size}.rgba"));
        if !(drawn && path.exists()) {
            std::fs::write(&path, icon::rgba(size)).expect("write an icon");
        }
    }

    if !windows {
        let _ = std::fs::write(&stamp, "");
        return;
    }
    let ico_path = out_dir.join("app.ico");
    if !(drawn && ico_path.exists()) {
        std::fs::write(&ico_path, icon::ico(&icon::ICO_SIZES)).expect("write app.ico");
    }
    #[cfg(windows)]
    {
        let mut res = winresource::WindowsResource::new();
        // The icons of the registered file types, referenced by the
        // registry as `"metalmpv.exe",-<id>` (assoc.rs). The app icon keeps
        // ID 1, the lowest, which Explorer shows for the executable.
        for (i, t) in filetypes::FILE_TYPES.iter().enumerate() {
            let path = out_dir.join(format!("type_{}.ico", t.id));
            if !(drawn && path.exists()) {
                let layers: Vec<(u32, Vec<u8>)> = icon::ICO_SIZES
                    .iter()
                    .map(|&s| {
                        let px = type_icon::rgba(t, s);
                        let image = if s >= 256 {
                            png(&px, s)
                        } else {
                            icon::bmp(&px, s)
                        };
                        (s, image)
                    })
                    .collect();
                std::fs::write(&path, icon::ico_of(&layers)).expect("write a file type icon");
            }
            res.set_icon_with_id(path.to_str().unwrap(), &filetypes::icon_id(i).to_string());
        }
        res.set_icon(ico_path.to_str().unwrap())
            .set("FileDescription", "MetalMPV - video player")
            .set("ProductName", "MetalMPV")
            .set("LegalCopyright", "Copyright (C) 2026 Fan4_Metal");
        if let Err(e) = res.compile() {
            // A missing resource compiler must not break the build.
            println!("cargo:warning=icon not embedded: {e}");
        }
    }
    if !drawn {
        // Older stamps go: one per state of the sources.
        if let Ok(entries) = std::fs::read_dir(&out_dir) {
            for e in entries.flatten() {
                let name = e.file_name().to_string_lossy().into_owned();
                if name.starts_with("icons_") && name.ends_with(".stamp") {
                    let _ = std::fs::remove_file(e.path());
                }
            }
        }
        let _ = std::fs::write(&stamp, "");
    }
}

/// FNV-1a of what draws the icons: their code, the types and this script.
fn icon_sources_hash() -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for file in [
        "src/icon.rs",
        "src/type_icon.rs",
        "src/filetypes.rs",
        "build.rs",
    ] {
        for byte in std::fs::read(file).unwrap_or_default() {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }
    hash
}

/// Straight RGBA pixels `px` of `size` x `size` as a PNG file.
#[cfg(windows)]
fn png(px: &[u8], size: u32) -> Vec<u8> {
    let mut out = Vec::new();
    let mut encoder = png::Encoder::new(&mut out, size, size);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.set_compression(png::Compression::High);
    let mut writer = encoder.write_header().expect("PNG header");
    writer.write_image_data(px).expect("PNG data");
    writer.finish().expect("PNG end");
    out
}

/// The version shown in the UI: Cargo's, plus the commit for a development
/// version ("0.1.0-dev (v0.1.0-2-g4bd494d)"), so that builds between
/// releases can be told apart. A release version is shown as is.
fn version() -> String {
    let version = std::env::var("CARGO_PKG_VERSION").unwrap();
    if !version.contains('-') {
        return version;
    }
    // `--always` falls back to the abbreviated hash when no tag is reachable
    // (a shallow checkout fetches none). No `--dirty`: this script is not rerun
    // when only sources change, so the mark would be stale.
    match git(&["describe", "--tags", "--always"]) {
        Some(describe) => {
            watch_git();
            format!("{version} ({describe})")
        }
        None => version,
    }
}

/// Reruns this script when a commit is made, the branch is switched or a tag
/// is added: HEAD, the branch it points to, packed refs and the tags.
fn watch_git() {
    let Some(dirs) = git(&["rev-parse", "--git-dir", "--git-common-dir"]) else {
        return;
    };
    let mut dirs = dirs.lines().map(std::path::PathBuf::from);
    let (Some(git_dir), Some(common)) = (dirs.next(), dirs.next()) else {
        return;
    };
    let head = git_dir.join("HEAD");
    let mut paths = vec![
        head.clone(),
        common.join("packed-refs"),
        common.join("refs").join("tags"),
    ];
    if let Some(branch) = std::fs::read_to_string(&head)
        .ok()
        .and_then(|h| h.strip_prefix("ref: ").map(|r| r.trim().to_owned()))
    {
        paths.push(common.join(branch));
    }
    // A path that does not exist would make Cargo rerun the script every time.
    for path in paths.iter().filter(|p| p.exists()) {
        println!("cargo:rerun-if-changed={}", path.display());
    }
}

/// Output of a git command, trimmed; `None` when git is missing or fails.
fn git(args: &[&str]) -> Option<String> {
    let out = std::process::Command::new("git").args(args).output().ok()?;
    let text = String::from_utf8(out.stdout).ok()?.trim().to_owned();
    (out.status.success() && !text.is_empty()).then_some(text)
}

/// Copy `vendor\libmpv\libmpv-2.dll` into the folder of the exe
/// (`target\<profile>`), where the program loads it from; skipped when the
/// copy there is the same. Without the DLL the build goes on: the program
/// starts and says what is missing.
fn copy_libmpv(out_dir: &std::path::Path) {
    let source = std::path::Path::new("vendor/libmpv/libmpv-2.dll");
    // OUT_DIR is target\<profile>\build\<package>-<hash>\out.
    let Some(target) = out_dir.ancestors().nth(3) else {
        return;
    };
    let Ok(meta) = source.metadata() else {
        // A missing path would rerun the script at every build (drawing
        // every icon again); the folder tells when the DLL arrives.
        let vendor = std::path::Path::new("vendor");
        let watched = if vendor.exists() { vendor } else { source };
        println!("cargo:rerun-if-changed={}", watched.display());
        println!("cargo:warning=libmpv-2.dll not found: run `python tools/get_libmpv.py`");
        return;
    };
    println!("cargo:rerun-if-changed={}", source.display());
    let dest = target.join("libmpv-2.dll");
    // A copy deleted from the exe's folder is made again.
    println!("cargo:rerun-if-changed={}", dest.display());
    let same = dest
        .metadata()
        .is_ok_and(|d| d.len() == meta.len() && d.modified().ok() == meta.modified().ok());
    if !same && let Err(e) = std::fs::copy(source, &dest) {
        println!(
            "cargo:warning=cannot copy libmpv-2.dll to {}: {e}",
            dest.display()
        );
    }
}
