//! A small safe layer over libmpv: the core ([`Mpv`]) with asynchronous
//! commands and properties and its events as owned values, and the OpenGL
//! render context ([`RenderContext`]).
//!
//! Threading (render.h, "Threading"): the render context is used on the UI
//! thread, so nothing called from that thread may wait for the core. Only
//! the functions that are safe there are wrapped: `mpv_command_async`,
//! `mpv_set_property_async`, `mpv_get_property_async`,
//! `mpv_observe_property` and `mpv_wait_event` with a zero timeout, besides
//! the `mpv_render_*` ones. The synchronous `mpv_command` and
//! `mpv_get/set_property` are deliberately left out.

pub mod sys;

use std::ffi::{CStr, CString, c_char, c_int, c_void};

pub use sys::Api;

/// An observed property's or a property reply's value.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    /// Not available now (no file, no video).
    None,
    Flag(bool),
    Int(i64),
    Double(f64),
    Str(String),
}

impl Value {
    pub fn flag(&self) -> bool {
        matches!(self, Value::Flag(true))
    }

    pub fn double(&self) -> Option<f64> {
        match *self {
            Value::Double(v) => Some(v),
            Value::Int(v) => Some(v as f64),
            _ => None,
        }
    }

    pub fn int(&self) -> Option<i64> {
        match *self {
            Value::Int(v) => Some(v),
            Value::Double(v) => Some(v as i64),
            _ => None,
        }
    }

    pub fn string(&self) -> Option<&str> {
        match self {
            Value::Str(s) => Some(s),
            _ => None,
        }
    }
}

/// Why a file stopped playing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EndReason {
    Eof,
    Stop,
    Quit,
    /// The file could not be played; the error is `mpv_error_string`'s.
    Error,
    Other,
}

/// An event of the core, copied out of libmpv's memory.
#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    Shutdown,
    Log {
        prefix: String,
        level: String,
        text: String,
    },
    /// A property change, or the reply to `get_property_async`; `id` is the
    /// number given when it was observed or asked for.
    Property {
        id: u64,
        value: Value,
    },
    CommandReply {
        id: u64,
        error: Option<String>,
    },
    StartFile,
    FileLoaded,
    EndFile {
        reason: EndReason,
        error: Option<String>,
    },
    Seek,
    PlaybackRestart,
    /// Events were lost: observed properties must be asked for again.
    QueueOverflow,
    Other(i32),
}

/// An mpv core (`mpv_handle`), destroyed when dropped. A render context
/// made from it must be freed first (see [`RenderContext`]).
pub struct Mpv {
    api: &'static Api,
    handle: *mut sys::mpv_handle,
    /// The data of the wakeup callback, kept alive while it may be called.
    wakeup: Option<Box<Box<dyn Fn() + Send + Sync>>>,
}

// SAFETY: the client API is thread-safe; the handle is a pointer to a
// core that synchronises its own state.
unsafe impl Send for Mpv {}
unsafe impl Sync for Mpv {}

impl Mpv {
    /// Create a core with `options` (`--name=value` without the dashes)
    /// and start it. An option the build does not know is logged and
    /// skipped; any other failure is returned.
    pub fn new(api: &'static Api, options: &[(&str, &str)]) -> Result<Self, String> {
        let handle = unsafe { (api.mpv_create)() };
        if handle.is_null() {
            return Err("mpv_create failed".into());
        }
        let mpv = Self {
            api,
            handle,
            wakeup: None,
        };
        for (name, value) in options {
            let (n, v) = (cstring(name), cstring(value));
            let code = unsafe { (api.mpv_set_option_string)(handle, n.as_ptr(), v.as_ptr()) };
            if code == sys::MPV_ERROR_OPTION_NOT_FOUND {
                log::info!("mpv option {name} is not known to this build");
            } else if code < 0 {
                log::warn!("mpv option {name}={value}: {}", mpv.error_string(code));
            }
        }
        let code = unsafe { (api.mpv_initialize)(handle) };
        if code < 0 {
            return Err(format!("mpv_initialize: {}", mpv.error_string(code)));
        }
        Ok(mpv)
    }

    pub fn api(&self) -> &'static Api {
        self.api
    }

    fn error_string(&self, code: c_int) -> String {
        error_string(self.api, code)
    }

    /// Call `wake` (from any thread, possibly from inside a libmpv call on
    /// this thread) when events are waiting. It must not call libmpv.
    pub fn set_wakeup(&mut self, wake: impl Fn() + Send + Sync + 'static) {
        let data: Box<Box<dyn Fn() + Send + Sync>> = Box::new(Box::new(wake));
        let ptr = (&raw const *data) as *mut c_void;
        unsafe { (self.api.mpv_set_wakeup_callback)(self.handle, Some(call_boxed), ptr) };
        // Replaced after the new callback is set: the old one is not
        // called any more.
        self.wakeup = Some(data);
    }

    /// Run a command (`["seek", "10", "relative+keyframes"]`) without
    /// waiting for it; the reply comes as [`Event::CommandReply`] with `id`.
    pub fn command(&self, id: u64, args: &[&str]) {
        let owned: Vec<CString> = args.iter().map(|a| cstring(a)).collect();
        let mut ptrs: Vec<*const c_char> = owned.iter().map(|c| c.as_ptr()).collect();
        ptrs.push(std::ptr::null());
        let code = unsafe { (self.api.mpv_command_async)(self.handle, id, ptrs.as_mut_ptr()) };
        if code < 0 {
            log::warn!("mpv command {args:?}: {}", self.error_string(code));
        }
    }

    /// Set a property without waiting; the value is copied at once.
    pub fn set(&self, name: &str, value: &Value) {
        let n = cstring(name);
        let set = |format: sys::mpv_format, data: *mut c_void| unsafe {
            (self.api.mpv_set_property_async)(self.handle, 0, n.as_ptr(), format, data)
        };
        let code = match value {
            Value::None => return,
            Value::Flag(v) => {
                let mut flag = c_int::from(*v);
                set(sys::MPV_FORMAT_FLAG, (&raw mut flag).cast())
            }
            Value::Int(v) => {
                let mut int = *v;
                set(sys::MPV_FORMAT_INT64, (&raw mut int).cast())
            }
            Value::Double(v) => {
                let mut double = *v;
                set(sys::MPV_FORMAT_DOUBLE, (&raw mut double).cast())
            }
            Value::Str(v) => {
                let string = cstring(v);
                let mut ptr = string.as_ptr();
                set(sys::MPV_FORMAT_STRING, (&raw mut ptr).cast())
            }
        };
        if code < 0 {
            log::warn!("mpv set {name}: {}", self.error_string(code));
        }
    }

    /// Ask for a property; the answer comes as [`Event::Property`] with `id`.
    pub fn get(&self, id: u64, name: &str, format: sys::mpv_format) {
        let n = cstring(name);
        unsafe { (self.api.mpv_get_property_async)(self.handle, id, n.as_ptr(), format) };
    }

    /// Report changes of property `name` as [`Event::Property`] with `id`.
    pub fn observe(&self, id: u64, name: &str, format: sys::mpv_format) {
        let n = cstring(name);
        let code = unsafe { (self.api.mpv_observe_property)(self.handle, id, n.as_ptr(), format) };
        if code < 0 {
            log::warn!("mpv observe {name}: {}", self.error_string(code));
        }
    }

    /// Have log messages of `level` ("warn", "info", …) and above sent as
    /// [`Event::Log`].
    pub fn request_log_messages(&self, level: &str) {
        let l = cstring(level);
        unsafe { (self.api.mpv_request_log_messages)(self.handle, l.as_ptr()) };
    }

    /// The next waiting event, without waiting for one.
    pub fn next_event(&self) -> Option<Event> {
        // SAFETY: the event stays valid until the next mpv_wait_event on
        // this handle; it is copied before returning.
        let event = unsafe { &*(self.api.mpv_wait_event)(self.handle, 0.0) };
        if event.event_id == sys::MPV_EVENT_NONE {
            return None;
        }
        Some(unsafe { self.convert(event) })
    }

    unsafe fn convert(&self, event: &sys::mpv_event) -> Event {
        let error = (event.error < 0).then(|| self.error_string(event.error));
        match event.event_id {
            sys::MPV_EVENT_SHUTDOWN => Event::Shutdown,
            sys::MPV_EVENT_LOG_MESSAGE => {
                let m = unsafe { &*(event.data as *const sys::mpv_event_log_message) };
                Event::Log {
                    prefix: unsafe { text(m.prefix) },
                    level: unsafe { text(m.level) },
                    text: unsafe { text(m.text) }.trim_end().to_owned(),
                }
            }
            sys::MPV_EVENT_PROPERTY_CHANGE | sys::MPV_EVENT_GET_PROPERTY_REPLY => {
                let p = unsafe { &*(event.data as *const sys::mpv_event_property) };
                let value = if event.error < 0 {
                    Value::None
                } else {
                    unsafe { value(p) }
                };
                Event::Property {
                    id: event.reply_userdata,
                    value,
                }
            }
            sys::MPV_EVENT_COMMAND_REPLY => Event::CommandReply {
                id: event.reply_userdata,
                error,
            },
            sys::MPV_EVENT_START_FILE => Event::StartFile,
            sys::MPV_EVENT_FILE_LOADED => Event::FileLoaded,
            sys::MPV_EVENT_END_FILE => {
                let e = unsafe { &*(event.data as *const sys::mpv_event_end_file) };
                let reason = match e.reason {
                    sys::MPV_END_FILE_REASON_EOF => EndReason::Eof,
                    sys::MPV_END_FILE_REASON_STOP => EndReason::Stop,
                    sys::MPV_END_FILE_REASON_QUIT => EndReason::Quit,
                    sys::MPV_END_FILE_REASON_ERROR => EndReason::Error,
                    _ => EndReason::Other,
                };
                let error = (e.error < 0).then(|| self.error_string(e.error));
                Event::EndFile { reason, error }
            }
            sys::MPV_EVENT_SEEK => Event::Seek,
            sys::MPV_EVENT_PLAYBACK_RESTART => Event::PlaybackRestart,
            sys::MPV_EVENT_QUEUE_OVERFLOW => Event::QueueOverflow,
            other => Event::Other(other),
        }
    }

    pub(crate) fn raw(&self) -> *mut sys::mpv_handle {
        self.handle
    }
}

impl Drop for Mpv {
    fn drop(&mut self) {
        unsafe {
            (self.api.mpv_set_wakeup_callback)(self.handle, None, std::ptr::null_mut());
            (self.api.mpv_terminate_destroy)(self.handle);
        }
    }
}

/// The OpenGL render context of a core. Every call needs the OpenGL
/// context it was made with to be current on the calling thread; it must
/// be freed ([`RenderContext::free`]) while that context lives and before
/// the core is destroyed.
pub struct RenderContext {
    api: &'static Api,
    ctx: *mut sys::mpv_render_context,
    update: Option<Box<Box<dyn Fn() + Send + Sync>>>,
}

// SAFETY: used only on the UI thread, where the OpenGL context is current;
// `Send` lets it sit in the state of an egui paint callback.
unsafe impl Send for RenderContext {}
unsafe impl Sync for RenderContext {}

impl RenderContext {
    /// Make the render context of `mpv` for the current OpenGL context;
    /// `get_proc_address` finds OpenGL functions by name.
    pub fn new(mpv: &Mpv, get_proc_address: sys::GetProcAddress) -> Result<Self, String> {
        let api = mpv.api();
        let mut init = sys::mpv_opengl_init_params {
            get_proc_address: Some(get_proc_address),
            get_proc_address_ctx: std::ptr::null_mut(),
        };
        let mut params = [
            sys::mpv_render_param {
                type_: sys::MPV_RENDER_PARAM_API_TYPE,
                data: sys::MPV_RENDER_API_TYPE_OPENGL.as_ptr() as *mut c_void,
            },
            sys::mpv_render_param {
                type_: sys::MPV_RENDER_PARAM_OPENGL_INIT_PARAMS,
                data: (&raw mut init).cast(),
            },
            sys::mpv_render_param {
                type_: sys::MPV_RENDER_PARAM_INVALID,
                data: std::ptr::null_mut(),
            },
        ];
        let mut ctx = std::ptr::null_mut();
        let code =
            unsafe { (api.mpv_render_context_create)(&mut ctx, mpv.raw(), params.as_mut_ptr()) };
        if code < 0 || ctx.is_null() {
            return Err(format!(
                "mpv_render_context_create: {}",
                error_string(api, code)
            ));
        }
        Ok(Self {
            api,
            ctx,
            update: None,
        })
    }

    /// Call `update` (from mpv's threads) when a new frame is ready or the
    /// render context wants [`RenderContext::update`] called. It must not
    /// call libmpv.
    pub fn set_update_callback(&mut self, update: impl Fn() + Send + Sync + 'static) {
        let data: Box<Box<dyn Fn() + Send + Sync>> = Box::new(Box::new(update));
        let ptr = (&raw const *data) as *mut c_void;
        unsafe {
            (self.api.mpv_render_context_set_update_callback)(self.ctx, Some(call_boxed), ptr)
        };
        self.update = Some(data);
    }

    /// Whether a new video frame should be rendered.
    pub fn update(&self) -> bool {
        unsafe {
            (self.api.mpv_render_context_update)(self.ctx) & sys::MPV_RENDER_UPDATE_FRAME != 0
        }
    }

    /// Render the current frame into framebuffer `fbo` of `w` x `h` pixels,
    /// not flipped: into an FBO, mpv puts the picture's top row first in
    /// memory, which egui shows at the top of a texture (`FLIP_Y` is for
    /// the default framebuffer, whose rows go up). Does not wait for the
    /// frame's display time.
    pub fn render(&self, fbo: u32, w: i32, h: i32) {
        let mut target = sys::mpv_opengl_fbo {
            fbo: fbo as c_int,
            w,
            h,
            internal_format: 0,
        };
        let mut block: c_int = 0;
        let mut params = [
            sys::mpv_render_param {
                type_: sys::MPV_RENDER_PARAM_OPENGL_FBO,
                data: (&raw mut target).cast(),
            },
            sys::mpv_render_param {
                type_: sys::MPV_RENDER_PARAM_BLOCK_FOR_TARGET_TIME,
                data: (&raw mut block).cast(),
            },
            sys::mpv_render_param {
                type_: sys::MPV_RENDER_PARAM_INVALID,
                data: std::ptr::null_mut(),
            },
        ];
        let code = unsafe { (self.api.mpv_render_context_render)(self.ctx, params.as_mut_ptr()) };
        if code < 0 {
            log::warn!(
                "mpv_render_context_render: {}",
                error_string(self.api, code)
            );
        }
    }

    /// The frame rendered last has been put on the screen.
    pub fn report_swap(&self) {
        unsafe { (self.api.mpv_render_context_report_swap)(self.ctx) };
    }

    /// Free the context; later calls do nothing.
    pub fn free(&mut self) {
        if !self.ctx.is_null() {
            unsafe {
                (self.api.mpv_render_context_set_update_callback)(
                    self.ctx,
                    None,
                    std::ptr::null_mut(),
                );
                (self.api.mpv_render_context_free)(self.ctx);
            }
            self.ctx = std::ptr::null_mut();
            self.update = None;
        }
    }

    pub fn is_freed(&self) -> bool {
        self.ctx.is_null()
    }
}

/// The trampoline of both callbacks: `data` is a `Box<dyn Fn()>` kept
/// alive by [`Mpv`] or [`RenderContext`].
unsafe extern "C" fn call_boxed(data: *mut c_void) {
    let f = unsafe { &*(data as *const Box<dyn Fn() + Send + Sync>) };
    f();
}

fn error_string(api: &Api, code: c_int) -> String {
    unsafe { text((api.mpv_error_string)(code)) }
}

/// A C string with any interior NUL dropped (paths and options have none).
fn cstring(s: &str) -> CString {
    CString::new(s.replace('\0', "")).unwrap_or_default()
}

unsafe fn text(p: *const c_char) -> String {
    if p.is_null() {
        String::new()
    } else {
        unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned()
    }
}

unsafe fn value(p: &sys::mpv_event_property) -> Value {
    if p.data.is_null() {
        return Value::None;
    }
    unsafe {
        match p.format {
            sys::MPV_FORMAT_FLAG => Value::Flag(*(p.data as *const c_int) != 0),
            sys::MPV_FORMAT_INT64 => Value::Int(*(p.data as *const i64)),
            sys::MPV_FORMAT_DOUBLE => Value::Double(*(p.data as *const f64)),
            sys::MPV_FORMAT_STRING => Value::Str(text(*(p.data as *const *const c_char))),
            _ => Value::None,
        }
    }
}
