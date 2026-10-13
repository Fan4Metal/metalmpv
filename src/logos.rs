//! The pictures of IPTV channels (an M3U list's `tvg-logo`), shown in the
//! playlist. A list is anybody's text, so a logo is fetched under rules of
//! its own (`allowed`, `fetch`): only `https://` on the usual port, never a
//! name or an address of the local network (nor a name that resolves to
//! one), through WinHTTP (the system's TLS and proxy), a redirect followed
//! only to an address allowed too, at most 512 KB, and only a PNG or JPEG
//! picture of at most 2048×2048, decoded by the `image` crate. The
//! pictures are kept in `logos` next to the settings for 30 days, so a list
//! opened again asks nobody (older ones are deleted, a damaged one fetched
//! again). Fetched on up to four threads of their own, started with the
//! first logo asked for; nothing at all with the setting off
//! (`channel_logos`). At most [`MAX_TEXTURES`] are kept in memory.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

use egui::{ColorImage, TextureHandle};

/// The most a logo may weigh.
const MAX_BYTES: usize = 512 * 1024;
/// The largest picture decoded.
const MAX_SIDE: u32 = 2048;
/// The side of the texture kept, in pixels.
const THUMB: u32 = 64;
const TIMEOUT_MS: i32 = 10_000;
/// How long a fetched logo is kept on disk.
const KEEP: Duration = Duration::from_secs(30 * 24 * 3600);
const WORKERS: usize = 4;
/// The most textures kept; those not shown for a while go beyond it (and
/// come back from the disk when scrolled to again).
const MAX_TEXTURES: usize = 400;
/// Redirects followed at most.
const MAX_REDIRECTS: usize = 3;

enum Slot {
    Loading,
    /// The texture and the frame it was last asked for in.
    Ready(TextureHandle, u64),
    Failed,
}

#[derive(Default)]
pub struct Logos {
    slots: HashMap<String, Slot>,
    /// The requests and the results, made with the first request.
    pool: Option<Pool>,
}

struct Pool {
    requests: Sender<String>,
    results: Receiver<(String, Option<ColorImage>)>,
}

impl Logos {
    /// The texture of the logo at `url` once fetched; asks for it the
    /// first time.
    pub fn get(&mut self, ctx: &egui::Context, url: &str) -> Option<&TextureHandle> {
        self.take_results(ctx);
        if !self.slots.contains_key(url) {
            if allowed(url).is_none() {
                log::debug!("logo not fetched: {url}");
                self.slots.insert(url.to_owned(), Slot::Failed);
                return None;
            }
            self.slots.insert(url.to_owned(), Slot::Loading);
            let pool = self.pool.get_or_insert_with(|| Pool::start(ctx));
            let _ = pool.requests.send(url.to_owned());
        }
        let frame = ctx.cumulative_frame_nr();
        match self.slots.get_mut(url) {
            Some(Slot::Ready(texture, used)) => {
                *used = frame;
                Some(texture)
            }
            _ => None,
        }
    }

    /// Drop the textures shown longest ago while there are too many.
    fn trim(&mut self) {
        let mut ready: Vec<(u64, String)> = self
            .slots
            .iter()
            .filter_map(|(url, slot)| match slot {
                Slot::Ready(_, used) => Some((*used, url.clone())),
                _ => None,
            })
            .collect();
        if ready.len() <= MAX_TEXTURES {
            return;
        }
        ready.sort_unstable();
        for (_, url) in ready.into_iter().take(MAX_TEXTURES / 4) {
            self.slots.remove(&url);
        }
    }

    fn take_results(&mut self, ctx: &egui::Context) {
        let Some(pool) = &self.pool else {
            return;
        };
        let mut added = false;
        while let Ok((url, image)) = pool.results.try_recv() {
            let slot = match image {
                Some(image) => {
                    added = true;
                    Slot::Ready(
                        ctx.load_texture(
                            format!("logo {url}"),
                            image,
                            egui::TextureOptions::LINEAR,
                        ),
                        ctx.cumulative_frame_nr(),
                    )
                }
                None => Slot::Failed,
            };
            self.slots.insert(url, slot);
        }
        if added {
            self.trim();
        }
    }
}

impl Pool {
    fn start(ctx: &egui::Context) -> Self {
        let (requests, queue) = channel::<String>();
        let (done, results) = channel();
        let queue = Arc::new(Mutex::new(queue));
        let _ = std::thread::Builder::new()
            .name("logos cleanup".into())
            .spawn(prune);
        for n in 0..WORKERS {
            let (queue, done, ctx) = (queue.clone(), done.clone(), ctx.clone());
            let _ = std::thread::Builder::new()
                .name(format!("logos {n}"))
                .spawn(move || {
                    loop {
                        let next = queue.lock().ok().and_then(|q| q.recv().ok());
                        let Some(url) = next else {
                            return;
                        };
                        let image = load(&url).map_err(|e| log::info!("logo {url}: {e}")).ok();
                        if done.send((url, image)).is_err() {
                            return;
                        }
                        ctx.request_repaint();
                    }
                });
        }
        Self { requests, results }
    }
}

/// The logo at `url`, from the disk if fetched lately, else from its site.
fn load(url: &str) -> Result<ColorImage, String> {
    let cached = cache_file(url);
    let fresh = cached.as_ref().and_then(|f| {
        let age = SystemTime::now()
            .duration_since(f.metadata().ok()?.modified().ok()?)
            .ok()?;
        (age < KEEP).then(|| std::fs::read(f).ok()).flatten()
    });
    if let Some(bytes) = fresh {
        match decode(&bytes) {
            Ok(image) => return Ok(image),
            // Cut short (the program ended while writing it): fetched again.
            Err(e) => {
                log::info!("logo {url}: the kept copy is damaged ({e})");
                if let Some(file) = &cached {
                    let _ = std::fs::remove_file(file);
                }
            }
        }
    }
    let bytes = fetch(url)?;
    if let Some(file) = &cached {
        let _ = std::fs::create_dir_all(file.parent().unwrap_or(file));
        // Written aside and renamed: never half a file under its name.
        let part = file.with_extension("part");
        if std::fs::write(&part, &bytes).is_ok() && std::fs::rename(&part, file).is_err() {
            let _ = std::fs::remove_file(&part);
        }
    }
    decode(&bytes)
}

/// Delete the logos kept longer than [`KEEP`] (and pieces left by a
/// write cut short).
fn prune() {
    let Some(dir) = cache_file("").and_then(|f| f.parent().map(PathBuf::from)) else {
        return;
    };
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return;
    };
    let now = SystemTime::now();
    let mut removed = 0;
    for entry in entries.flatten() {
        let path = entry.path();
        let old = entry
            .metadata()
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| now.duration_since(t).ok())
            .is_some_and(|age| age >= KEEP);
        let part = path.extension().is_some_and(|e| e == "part");
        if (old || part) && path.is_file() && std::fs::remove_file(&path).is_ok() {
            removed += 1;
        }
    }
    if removed > 0 {
        log::debug!("{removed} old logos deleted");
    }
}

/// The host and the path (with the query) of `url` if a logo may be
/// fetched from it: `https://`, no user name, the usual port, a host that
/// is neither of the local network nor an address in it.
fn allowed(url: &str) -> Option<(String, String)> {
    let rest = url.strip_prefix("https://")?;
    let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let (authority, tail) = rest.split_at(end);
    if authority.contains('@') || authority.starts_with('[') {
        return None;
    }
    let host = match authority.split_once(':') {
        Some((host, "443")) => host,
        Some(_) => return None,
        None => authority,
    }
    .to_ascii_lowercase();
    if host.is_empty() || host.ends_with('.') || local(&host) || odd_number(&host) {
        return None;
    }
    let path = tail.split('#').next().unwrap_or_default();
    let path = if path.starts_with('/') {
        path.to_owned()
    } else {
        format!("/{path}")
    };
    Some((host, path))
}

/// A name or an IPv4 address of this computer or of a local network.
fn local(host: &str) -> bool {
    if host == "localhost"
        || !host.contains('.')
        || [".local", ".lan", ".home", ".internal", ".localhost"]
            .iter()
            .any(|s| host.ends_with(s))
    {
        return true;
    }
    match host.parse::<std::net::Ipv4Addr>() {
        Ok(ip) => {
            ip.is_private()
                || ip.is_loopback()
                || ip.is_link_local()
                || ip.is_unspecified()
                || ip.is_broadcast()
                // 100.64/10, shared by carriers.
                || (ip.octets()[0] == 100 && (64..128).contains(&ip.octets()[1]))
        }
        Err(_) => false,
    }
}

/// A host of numbers that is not a plain dotted quad (`127.1`,
/// `0177.0.0.1`, `0x7f.1`): resolvers may take it for a local address.
fn odd_number(host: &str) -> bool {
    host.parse::<std::net::Ipv4Addr>().is_err()
        && host.split('.').all(|label| {
            !label.is_empty()
                && (label.bytes().all(|b| b.is_ascii_digit())
                    || label.starts_with("0x") && label[2..].bytes().all(|b| b.is_ascii_hexdigit()))
        })
}

/// Whether `host` resolves to an address of this computer or of a local
/// network (a public name may point there).
fn resolves_locally(host: &str) -> bool {
    use std::net::{IpAddr, ToSocketAddrs};
    let Ok(addresses) = (host, 443).to_socket_addrs() else {
        // Not resolved: WinHTTP will say so.
        return false;
    };
    addresses.map(|a| a.ip()).any(|ip| match ip {
        IpAddr::V4(v4) => local(&v4.to_string()),
        IpAddr::V6(v6) => {
            let first = v6.segments()[0];
            v6.is_loopback()
                || v6.is_unspecified()
                || (first & 0xfe00) == 0xfc00 // unique local
                || (first & 0xffc0) == 0xfe80 // link-local
                || v6.to_ipv4_mapped().is_some_and(|v4| local(&v4.to_string()))
        }
    })
}

fn fetch(url: &str) -> Result<Vec<u8>, String> {
    let mut url = url.to_owned();
    let mut redirects = 0;
    let body = loop {
        let (host, path) = allowed(&url).ok_or("not an address logos are fetched from")?;
        if resolves_locally(&host) {
            return Err(format!("{host} is on the local network"));
        }
        let (status, body, location) = crate::win::https_get_once(
            &host,
            &path,
            &format!("{}/{}", crate::APP_NAME, crate::VERSION),
            "Accept: image/png, image/jpeg",
            TIMEOUT_MS,
            MAX_BYTES,
        )?;
        match (status, location) {
            (200, _) => break body,
            // Followed only to an address a logo may come from.
            (300..400, Some(next)) if redirects < MAX_REDIRECTS => {
                redirects += 1;
                url = if next.starts_with('/') {
                    format!("https://{host}{next}")
                } else {
                    next
                };
            }
            (status, _) => return Err(format!("HTTP {status}")),
        }
    };
    // Checked before it is kept: only a picture goes to the disk.
    decode(&body)?;
    Ok(body)
}

/// A PNG or JPEG picture, at most [`MAX_SIDE`] a side, as a texture of at
/// most [`THUMB`] a side.
fn decode(bytes: &[u8]) -> Result<ColorImage, String> {
    let format = image::guess_format(bytes).map_err(|e| e.to_string())?;
    if !matches!(format, image::ImageFormat::Png | image::ImageFormat::Jpeg) {
        return Err(format!("not a PNG or JPEG picture: {format:?}"));
    }
    let mut reader = image::ImageReader::with_format(std::io::Cursor::new(bytes), format);
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(MAX_SIDE);
    limits.max_image_height = Some(MAX_SIDE);
    limits.max_alloc = Some(64 * 1024 * 1024);
    reader.limits(limits);
    let picture = reader.decode().map_err(|e| e.to_string())?;
    let rgba = picture.thumbnail(THUMB, THUMB).to_rgba8();
    Ok(ColorImage::from_rgba_unmultiplied(
        [rgba.width() as usize, rgba.height() as usize],
        rgba.as_raw(),
    ))
}

/// Where the logo at `url` is kept: `logos\<hash>` next to the settings.
fn cache_file(url: &str) -> Option<PathBuf> {
    let settings = crate::settings::location().file.as_ref()?;
    let mut hash: u64 = 0xCBF2_9CE4_8422_2325;
    for b in url.bytes() {
        hash ^= u64::from(b);
        hash = hash.wrapping_mul(0x0000_0100_0000_01B3);
    }
    Some(
        settings
            .parent()?
            .join("logos")
            .join(format!("{hash:016x}")),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_https_off_the_local_network() {
        assert_eq!(
            allowed("https://iptvx.one/picons/pervy.png"),
            Some(("iptvx.one".into(), "/picons/pervy.png".into()))
        );
        assert_eq!(
            allowed("https://Example.com:443?x=1#frag"),
            Some(("example.com".into(), "/?x=1".into()))
        );
        for url in [
            "http://iptvx.one/a.png",
            "file:///C:/Windows/a.png",
            "https://example.com:8443/a.png",
            "https://user@example.com/a.png",
            "https://localhost/a.png",
            "https://router/a.png",
            "https://nas.local/a.png",
            "https://192.168.1.1/a.png",
            "https://10.0.0.5/a.png",
            "https://127.0.0.1/a.png",
            "https://169.254.1.1/a.png",
            "https://[::1]/a.png",
            "https:///a.png",
            "https://localhost./a.png",
            "https://example.com./a.png",
            "https://127.1/a.png",
            "https://0177.0.0.1/a.png",
            "https://0x7f.1/a.png",
        ] {
            assert_eq!(allowed(url), None, "{url}");
        }
        assert!(allowed("https://8.8.8.8/a.png").is_some());
    }

    #[test]
    fn only_pictures_decode() {
        assert!(decode(b"<html>not a picture</html>").is_err());
        let mut png = Vec::new();
        image::RgbaImage::new(200, 100)
            .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
            .unwrap();
        let image = decode(&png).unwrap();
        assert_eq!(image.size, [64, 32]);
        // Larger than allowed: refused before it is decoded.
        let mut big = Vec::new();
        image::GrayImage::new(4000, 10)
            .write_to(&mut std::io::Cursor::new(&mut big), image::ImageFormat::Png)
            .unwrap();
        assert!(decode(&big).is_err());
    }

    /// Fetches a logo of the list for real: `cargo test logo_of_a_site
    /// -- --ignored --nocapture`.
    #[test]
    #[ignore = "goes to the network"]
    fn logo_of_a_site() {
        let bytes = fetch("https://iptvx.one/picons/pervy.png").unwrap();
        let image = decode(&bytes).unwrap();
        eprintln!("{} bytes, {:?}", bytes.len(), image.size);
    }
}
