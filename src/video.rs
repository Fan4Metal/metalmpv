//! The picture: mpv renders through its OpenGL render API into a texture of
//! the app's own, which egui shows like any image.
//!
//! - The render context is made in `App::new`, where eframe's OpenGL
//!   context is current, before the first file is loaded (mpv's video
//!   output fails without it).
//! - Rendering happens in an egui paint callback, where the context is
//!   current for sure. A new frame is rendered only when mpv says so
//!   (`mpv_render_context_update`) or the size changed; repaints of the
//!   interface alone (the OSD, a hover) show the texture as it is.
//! - The texture has the size of the video area in pixels; mpv letterboxes
//!   the picture in it. A size change re-specifies the storage of the same
//!   texture name: egui knows the texture by that name, and eframe offers
//!   no way to free a texture registered with
//!   `register_native_glow_texture`, so a new one per size would leak.
//! - After rendering, the default framebuffer is bound again; egui_glow
//!   restores the rest of its state after a callback itself.

use std::ffi::{c_char, c_void};
use std::sync::{Arc, Mutex, OnceLock};

use eframe::egui_glow;
use eframe::glow::{self, HasContext};

use crate::mpv::{Mpv, RenderContext};

pub struct Video {
    shared: Arc<Mutex<Shared>>,
    /// The texture as egui knows it, registered at the first frame
    /// (`eframe::Frame` is needed for that).
    id: Option<egui::TextureId>,
}

/// What the paint callback works on.
struct Shared {
    render: RenderContext,
    fbo: glow::Framebuffer,
    texture: glow::Texture,
    /// The size of the texture's storage in pixels.
    allocated: [i32; 2],
    /// The size of the video area in pixels this frame.
    wanted: [i32; 2],
    /// A frame was rendered in the previous paint: report its swap.
    swap_pending: bool,
    /// For the timing of the first frame (`METALMPV_TRACE`).
    rendered_once: bool,
}

impl Video {
    /// Make the render context of `mpv` in the current OpenGL context `gl`;
    /// `ctx` is woken when mpv has a new frame.
    pub fn new(gl: &glow::Context, mpv: &Mpv, ctx: &egui::Context) -> Result<Self, String> {
        let started = std::time::Instant::now();
        let mut render = RenderContext::new(mpv, get_proc_address)?;
        log::debug!(
            "render context made in {:.0} ms",
            started.elapsed().as_secs_f64() * 1e3
        );
        let wake = ctx.clone();
        render.set_update_callback(move || wake.request_repaint());
        let (fbo, texture) = unsafe {
            let texture = gl.create_texture()?;
            gl.bind_texture(glow::TEXTURE_2D, Some(texture));
            for (name, value) in [
                (glow::TEXTURE_MIN_FILTER, glow::LINEAR),
                (glow::TEXTURE_MAG_FILTER, glow::LINEAR),
                (glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE),
                (glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE),
            ] {
                gl.tex_parameter_i32(glow::TEXTURE_2D, name, value as i32);
            }
            allocate(gl, 1, 1);
            gl.bind_texture(glow::TEXTURE_2D, None);
            let fbo = gl.create_framebuffer()?;
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fbo));
            gl.framebuffer_texture_2d(
                glow::FRAMEBUFFER,
                glow::COLOR_ATTACHMENT0,
                glow::TEXTURE_2D,
                Some(texture),
                0,
            );
            gl.bind_framebuffer(glow::FRAMEBUFFER, None);
            (fbo, texture)
        };
        let shared = Shared {
            render,
            fbo,
            texture,
            allocated: [1, 1],
            wanted: [0, 0],
            swap_pending: false,
            rendered_once: false,
        };
        Ok(Self {
            shared: Arc::new(Mutex::new(shared)),
            id: None,
        })
    }

    /// Render mpv's picture for `rect` (in points) and show it if `visible`.
    /// Called for every frame a file is loaded, before mpv says the video's
    /// size: mpv waits for its first frame to be rendered (up to 200 ms)
    /// before it publishes the size.
    pub fn show(
        &mut self,
        ui: &egui::Ui,
        frame: &mut eframe::Frame,
        rect: egui::Rect,
        visible: bool,
    ) {
        let px = pixel_size(rect, ui.ctx().pixels_per_point());
        if px[0] <= 0 || px[1] <= 0 {
            return;
        }
        let id = *self.id.get_or_insert_with(|| {
            let texture = self
                .shared
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .texture;
            frame.register_native_glow_texture(texture)
        });
        self.shared.lock().unwrap_or_else(|e| e.into_inner()).wanted = px;
        let shared = self.shared.clone();
        let callback = egui_glow::CallbackFn::new(move |_info, painter| {
            shared
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .paint(painter.gl());
        });
        let painter = ui.painter_at(rect);
        painter.add(egui::PaintCallback {
            rect,
            callback: Arc::new(callback),
        });
        if visible {
            let uv = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
            painter.image(id, rect, uv, egui::Color32::WHITE);
        }
    }

    /// Free the render context and the framebuffer; call it in
    /// `App::on_exit` with the context still current, before the core is
    /// destroyed. The texture is egui's and freed by its painter.
    pub fn free(&mut self, gl: Option<&glow::Context>) {
        let mut shared = self.shared.lock().unwrap_or_else(|e| e.into_inner());
        shared.render.free();
        if let Some(gl) = gl {
            unsafe { gl.delete_framebuffer(shared.fbo) };
        }
    }
}

impl Shared {
    fn paint(&mut self, gl: &glow::Context) {
        if self.render.is_freed() {
            return;
        }
        if std::mem::take(&mut self.swap_pending) {
            self.render.report_swap();
        }
        let resized = self.wanted != self.allocated;
        if resized {
            unsafe {
                gl.bind_texture(glow::TEXTURE_2D, Some(self.texture));
                allocate(gl, self.wanted[0], self.wanted[1]);
                gl.bind_texture(glow::TEXTURE_2D, None);
            }
            self.allocated = self.wanted;
        }
        // Asked every paint: without it mpv would not know the frame was
        // taken.
        let new_frame = self.render.update();
        if new_frame || resized {
            let [w, h] = self.allocated;
            let started = std::time::Instant::now();
            self.render.render(self.fbo.0.get(), w, h);
            unsafe { gl.bind_framebuffer(glow::FRAMEBUFFER, None) };
            if new_frame && !self.rendered_once {
                self.rendered_once = true;
                log::debug!(
                    "first frame rendered at {:.0} ms ({:.1} ms)",
                    crate::since_start(),
                    started.elapsed().as_secs_f64() * 1e3
                );
            }
            self.swap_pending = true;
        }
    }
}

/// Give the bound texture storage of `w` x `h` RGBA pixels: plain RGBA8 as
/// egui_glow's own textures (no sRGB conversion on the way).
unsafe fn allocate(gl: &glow::Context, w: i32, h: i32) {
    unsafe {
        gl.tex_image_2d(
            glow::TEXTURE_2D,
            0,
            glow::RGBA8 as i32,
            w,
            h,
            0,
            glow::RGBA,
            glow::UNSIGNED_BYTE,
            glow::PixelUnpackData::Slice(None),
        );
    }
}

/// The size in pixels egui_glow gives `rect` (its edges are rounded
/// separately, `ViewportInPixels::from_points`).
pub fn pixel_size(rect: egui::Rect, ppp: f32) -> [i32; 2] {
    let px = |v: f32| (v * ppp).round() as i32;
    [
        px(rect.max.x) - px(rect.min.x),
        px(rect.max.y) - px(rect.min.y),
    ]
}

/// Find an OpenGL function for mpv: extensions and functions past OpenGL 1.1
/// through `wglGetProcAddress`, the rest from opengl32.dll itself.
unsafe extern "C" fn get_proc_address(_ctx: *mut c_void, name: *const c_char) -> *mut c_void {
    use windows_sys::Win32::System::LibraryLoader::{
        GetModuleHandleW, GetProcAddress, LoadLibraryW,
    };
    type WglGetProcAddress = unsafe extern "system" fn(*const c_char) -> *mut c_void;
    static OPENGL: OnceLock<(usize, Option<WglGetProcAddress>)> = OnceLock::new();
    let (module, wgl) = *OPENGL.get_or_init(|| unsafe {
        let dll = crate::win::wide("opengl32.dll");
        let mut module = GetModuleHandleW(dll.as_ptr());
        if module.is_null() {
            module = LoadLibraryW(dll.as_ptr());
        }
        let wgl = GetProcAddress(module, c"wglGetProcAddress".as_ptr().cast()).map(|f| {
            std::mem::transmute::<unsafe extern "system" fn() -> isize, WglGetProcAddress>(f)
        });
        (module as usize, wgl)
    });
    if let Some(wgl) = wgl {
        let p = unsafe { wgl(name) };
        // Some drivers return 1, 2, 3 or -1 instead of null for "not here".
        if !matches!(p as isize, -1..=3) {
            return p;
        }
    }
    match unsafe { GetProcAddress(module as *mut c_void, name.cast()) } {
        Some(f) => f as *mut c_void,
        None => std::ptr::null_mut(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pixel_size_rounds_edges() {
        let r = egui::Rect::from_min_max(egui::pos2(0.4, 10.2), egui::pos2(100.6, 50.7));
        assert_eq!(pixel_size(r, 1.0), [101, 41]);
        assert_eq!(pixel_size(r, 1.5), [150, 61]);
    }
}
