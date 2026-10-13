//! The pictures of IPTV channels (an M3U list's `tvg-logo`), shown in the
//! playlist. A list is anybody's text, so a logo is fetched under rules of
//! its own (`allowed`, `fetch`): only `https://` on the usual port, never a
//! name or an address of the local network, through WinHTTP (the system's
//! TLS and proxy; it does not follow a redirect from HTTPS to HTTP), at
//! most 512 KB, and only a PNG or JPEG picture of at most 2048×2048,
//! decoded by the `image` crate. The pictures are kept in `logos` next to
//! the settings for 30 days, so a list opened again asks nobody. Fetched on
//! up to four threads of their own, started with the first logo asked
//! for; nothing at all with the setting off (`channel_logos`).

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

enum Slot {
    Loading,
    Ready(TextureHandle),
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
        match self.slots.get(url) {
            Some(Slot::Ready(texture)) => Some(texture),
            _ => None,
        }
    }

    fn take_results(&mut self, ctx: &egui::Context) {
        let Some(pool) = &self.pool else {
            return;
        };
        while let Ok((url, image)) = pool.results.try_recv() {
            let slot = match image {
                Some(image) => Slot::Ready(ctx.load_texture(
                    format!("logo {url}"),
                    image,
                    egui::TextureOptions::LINEAR,
                )),
                None => Slot::Failed,
            };
            self.slots.insert(url, slot);
        }
    }
}

impl Pool {
    fn start(ctx: &egui::Context) -> Self {
        let (requests, queue) = channel::<String>();
        let (done, results) = channel();
        let queue = Arc::new(Mutex::new(queue));
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
    let bytes = match fresh {
        Some(bytes) => bytes,
        None => {
            let bytes = fetch(url)?;
            if let Some(file) = &cached {
                let _ = std::fs::create_dir_all(file.parent().unwrap_or(file));
                let _ = std::fs::write(file, &bytes);
            }
            bytes
        }
    };
    decode(&bytes)
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
    if host.is_empty() || local(&host) {
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

fn fetch(url: &str) -> Result<Vec<u8>, String> {
    let (host, path) = allowed(url).ok_or("not an address logos are fetched from")?;
    let (status, body) = crate::win::https_get(
        &host,
        &path,
        &format!("{}/{}", crate::APP_NAME, crate::VERSION),
        "Accept: image/png, image/jpeg",
        TIMEOUT_MS,
        MAX_BYTES,
    )?;
    if status != 200 {
        return Err(format!("HTTP {status}"));
    }
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
