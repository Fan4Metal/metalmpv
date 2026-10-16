//! The Video, sound and subtitles panel (`U` or `F7`, the sliders button
//! of the control bar): a window over its button, as the tracks window is,
//! that a press outside it closes; not an egui popup, so that playback and
//! the keys go on while it is open (egui's popups take the keys). Its head
//! is the three tabs and Save the frame (the camera; its menu opens the
//! folder). How each setting applies is said in its tooltip.
//!
//! Video: the picture's adjustments (`adjust.rs`): the colour, the zoom and
//! the pan, the sharpness, the noise reduction and the debanding, the
//! aspect ratio, the deinterlacing, HDR on an ordinary screen, the turns
//! and flips; Reset all, and whether they are kept between runs. Sound
//! (`audio.rs`): the output device, the channels, compressed sound to a
//! receiver, the delay, the boost,
//! the normalization, clearer speech, and the ten-band equalizer with its
//! presets and the user's own; Reset all, the device and the receiver
//! aside. Subtitles (`sub_style.rs`): the looks,
//! ready-made and the user's own, a preview, the font and the size, the
//! colours, outline, shadow and box, the
//! height, the delay, the black bars, and the style for ASS too.
//!
//! The values are rows of a label, a slider lit from its origin to the
//! knob, the value (in the accent colour while off its default) and the
//! round arrow back to it; on/off are switches, as in the settings.

use std::ops::RangeInclusive;

use egui::{
    Align, Align2, Color32, FontId, Layout, Rect, RichText, Sense, Shape, Stroke, Ui, Vec2, pos2,
    vec2,
};

use super::{icons, popover, style};
use crate::adjust::{self, Adjust, Aspect, Color, Deinterlace, ToneMap};
use crate::audio::{
    self, Channels, EQ_BANDS, EQ_RANGE, Equalizer, OwnEq, Passthrough, Preset, band_label,
};
use crate::input::Cmd;
use crate::sub_style::{self, SubStyle};

/// The width of the panel's content.
const WIDTH: f32 = 360.0;
/// The width of the labels' column.
const LABEL: f32 = 110.0;
/// The width of a value.
const VALUE: f32 = 50.0;
/// The width of a delay's value: "-12.50 s". A field that grew with a
/// minus widened the panel, which shook as it was laid out again.
const DELAY_VALUE: f32 = 64.0;
/// The delays' sliders reach this far either way; the keys go further.
const DELAY_RANGE: f64 = 5.0;
/// The gutter of the scroll bar while the tabs scroll.
const SCROLL_GUTTER: f32 = 14.0;
/// The least height the tabs are given, however low the window.
const MIN_ROOM: f32 = 120.0;
/// The height of the row where a name is typed.
const NAME_ROW: f32 = 28.0;
/// The panel's `egui::Area`.
const PANEL_ID: &str = "video_panel";
/// A slider's knob, its radius.
const KNOB: f32 = 6.0;

/// The panel's tab.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Tab {
    #[default]
    Video,
    Sound,
    Subtitles,
}

impl Tab {
    const ALL: [Tab; 3] = [Tab::Video, Tab::Sound, Tab::Subtitles];

    fn label(self) -> &'static str {
        match self {
            Tab::Video => tr!("Video", "Видео"),
            Tab::Sound => tr!("Sound", "Звук"),
            Tab::Subtitles => tr!("Subtitles", "Субтитры"),
        }
    }
}

#[derive(Default)]
pub struct VideoPanel {
    pub open: bool,
    tab: Tab,
    /// The name being typed for what is to be saved (the subtitles' look,
    /// the equalizer's gains).
    naming: Option<String>,
    /// Where the panel was drawn last, for the presses outside it.
    rect: Option<egui::Rect>,
    /// The tabs scroll: the window is too low for the tallest.
    scrolls: bool,
    /// A value was being typed in the last frame: egui drops the focus on
    /// Esc before the keys are read, and that Esc closed the panel too.
    typed: bool,
}

/// What the Sound and Subtitles tabs edit: settings, kept between runs.
pub struct Kept<'a> {
    /// The output device (`settings.audio_device`; empty: Windows'
    /// default) and the devices there are.
    pub audio_device: &'a mut String,
    pub devices: &'a [audio::Device],
    pub channels: &'a mut Channels,
    pub passthrough: &'a mut Passthrough,
    pub volume_max: &'a mut f64,
    pub normalize: &'a mut bool,
    pub normalize_strength: &'a mut audio::Normalize,
    pub voice_db: &'a mut i64,
    pub voice_boost: &'a mut bool,
    pub upmix: &'a mut bool,
    pub upmix_lfe: &'a mut audio::Upmix,
    pub subwoofer: &'a mut audio::Subwoofer,
    pub equalizer: &'a mut Equalizer,
    /// The equalizer's gains the user saved.
    pub eq_presets: &'a mut Vec<OwnEq>,
    pub subs: &'a mut SubStyle,
    /// The subtitles' looks the user saved.
    pub sub_presets: &'a mut Vec<sub_style::Own>,
    pub sub_ass_override: &'a mut bool,
    pub sub_margins: &'a mut bool,
    /// The picture's adjustments are kept between runs.
    pub keep_adjust: &'a mut bool,
    /// The navigator over the picture's corner while it is zoomed in.
    pub navigator: &'a mut bool,
}

/// What the panel shows of the playback now.
#[derive(Clone, Copy, Debug)]
pub struct Now<'a> {
    /// A frame can be saved.
    pub has_video: bool,
    /// The sound's and the subtitles' delays, in seconds.
    pub audio_delay: f64,
    pub sub_delay: f64,
    /// The audio track playing, what the device gets of it and its bitrate
    /// of the moment (asked for every second while the panel is open).
    pub audio: Option<&'a crate::tracks::Track>,
    pub audio_out: Option<&'a crate::player::AudioOut>,
    pub audio_bitrate: Option<f64>,
    /// Clearer speech is on: in stereo its filter adds a centre, and the
    /// device gets three channels.
    pub voice_boost: bool,
    /// Stereo to 5.1 is on and takes the track: the device gets six.
    pub upmix: bool,
    /// The position, the lines of the subtitle file shown (one of its own)
    /// and whether subtitles are shown: the timeline of the delay.
    pub time: Option<f64>,
    pub sub_lines: Option<&'a [crate::sub_lines::Line]>,
    pub subs_on: bool,
    /// They are of a file of their own.
    pub subs_external: bool,
    /// The picture's shape and its area, for the navigator.
    pub geometry: super::navigator::Geometry,
}

impl VideoPanel {
    /// A name or a value is being typed: the player's keys wait, and the
    /// field keeps the focus.
    pub fn typing(&self, ctx: &egui::Context) -> bool {
        self.open && (self.naming.is_some() || self.typed || popover::has_focus(ctx, PANEL_ID))
    }

    pub fn toggle(&mut self) {
        self.open = !self.open;
    }

    /// Open the panel on `tab`.
    pub fn open_on(&mut self, tab: Tab) {
        self.open = true;
        if self.tab != tab {
            self.naming = None;
        }
        self.tab = tab;
    }

    /// Draw the panel while it is open, editing `adjust` and `kept` in
    /// place; the commands of its buttons. It stands over `button` (the
    /// control bar's, where it was drawn this frame), its right edge on the
    /// button's, else at the window's bottom right corner; as tall as its
    /// tallest tab whichever is shown, and when the window is too low for
    /// that, as tall as there is room above the button, the tabs scrolling
    /// under the head. A press outside it
    /// and its button closes it, unless a popup of its own is open (a
    /// list, the colour picker, the camera's menu): the press is that one's.
    pub fn show(
        &mut self,
        ctx: &egui::Context,
        button: Option<egui::Rect>,
        adjust: &mut Adjust,
        kept: Kept<'_>,
        now: Now<'_>,
    ) -> Vec<Cmd> {
        let mut out = Vec::new();
        if !self.open {
            self.rect = None;
            self.naming = None;
            self.typed = false;
            return out;
        }
        if popover::pressed_outside(ctx, self.rect, button) {
            self.open = false;
            self.rect = None;
            self.naming = None;
            self.typed = false;
            return out;
        }
        // A gutter for the scroll bar while the tabs scroll, so that it
        // does not run over the values (decided last frame: the width
        // places the panel).
        let content = WIDTH + if self.scrolls { SCROLL_GUTTER } else { 0.0 };
        let corner = popover::corner(ctx, button);
        // The height there is for the head and a tab, over the button.
        let margins = popover::frame(ctx).total_margin().sum().y;
        let limit = corner.y - ctx.content_rect().top() - popover::EDGE - margins;
        let shown = popover::show(ctx, PANEL_ID, corner, content, |ui| {
            let top = ui.cursor().top();
            self.head(ui, now.has_video, &mut out);
            ui.add_space(4.0);
            let room = (limit - (ui.cursor().top() - top)).max(MIN_ROOM);
            // Every tab laid out unseen (an invisible Ui paints nothing and
            // takes no input) for the tallest: the panel keeps that height
            // whichever is shown, as one held by its bottom edge jumped
            // when they changed. Ids of their own: with the shown tab's, a
            // list open there opened here too, and its search field took
            // each key twice.
            let mut kept = kept;
            let mut tallest = 0.0_f32;
            for tab in Tab::ALL {
                let rect = ui.available_rect_before_wrap();
                let mut unseen = ui.new_child(
                    egui::UiBuilder::new()
                        .id_salt(("unseen", tab as u8))
                        .max_rect(rect)
                        .invisible(),
                );
                let (mut no_name, mut no_cmds) = (None, Vec::new());
                tab_contents(
                    &mut unseen,
                    tab,
                    adjust,
                    &mut kept,
                    now,
                    &mut no_name,
                    &mut no_cmds,
                );
                tallest = tallest.max(unseen.min_rect().height());
            }
            // The row where a name is typed, while it is.
            if self.naming.is_some() {
                tallest += NAME_ROW;
            }
            self.scrolls = tallest > room;
            if self.scrolls {
                // Too low a window: the tabs scroll under the head, the
                // panel as high as there is room over the button.
                egui::ScrollArea::vertical()
                    .id_salt(("video_panel", self.tab as u8))
                    .max_height(room)
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.set_width(WIDTH);
                        tab_contents(
                            ui,
                            self.tab,
                            adjust,
                            &mut kept,
                            now,
                            &mut self.naming,
                            &mut out,
                        );
                    });
            } else {
                // The room as a least height, not space added after: that
                // came with the spacing between items, and the shorter tabs
                // were taller by it.
                ui.scope(|ui| {
                    ui.set_min_height(tallest);
                    tab_contents(
                        ui,
                        self.tab,
                        adjust,
                        &mut kept,
                        now,
                        &mut self.naming,
                        &mut out,
                    );
                });
            }
        });
        self.rect = Some(shown);
        self.typed = popover::has_focus(ctx, PANEL_ID);
        out
    }

    /// The tabs, then at the right the camera: Save the frame, its menu
    /// opening the folder.
    fn head(&mut self, ui: &mut Ui, has_video: bool, out: &mut Vec<Cmd>) {
        ui.horizontal(|ui| {
            let before = self.tab;
            tabs(ui, &mut self.tab, WIDTH - RESET - 12.0);
            if self.tab != before {
                self.naming = None;
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                let tip = format!(
                    "{}\n{}",
                    crate::keymap::with_key(tr!("Save the frame", "Сохранить кадр"), Cmd::SaveFrame),
                    tr!(
                        "Into Pictures\\MetalMPV, as decoded, without subtitles. A right click opens the folder.",
                        "В «Изображения\\MetalMPV», как декодирован, без субтитров. Правый щелчок открывает папку."
                    )
                );
                let camera = super::tool(ui, &tip, true, |p, o, c| icons::camera(p, o, 6.5, c));
                if camera.clicked() && has_video {
                    out.push(Cmd::SaveFrame);
                }
                camera.context_menu(|ui| {
                    let text = tr!("Open the folder", "Открыть папку");
                    if icon_button(ui, icons::open, text, None, |b| b.frame(false)).clicked() {
                        out.push(Cmd::ShowFrames);
                        ui.close();
                    }
                });
            });
        });
    }
}

/// The tabs ([`popover::tabs`]).
fn tabs(ui: &mut Ui, current: &mut Tab, width: f32) {
    let labels = Tab::ALL.map(Tab::label);
    let mut i = Tab::ALL.iter().position(|t| t == current).unwrap_or(0);
    popover::tabs(ui, &labels, &mut i, width, "panel_tab");
    *current = Tab::ALL[i];
}

/// A tab's contents.
fn tab_contents(
    ui: &mut Ui,
    tab: Tab,
    adjust: &mut Adjust,
    kept: &mut Kept<'_>,
    now: Now<'_>,
    naming: &mut Option<String>,
    out: &mut Vec<Cmd>,
) {
    match tab {
        Tab::Video => video_tab(ui, adjust, kept, now),
        Tab::Sound => sound_tab(ui, kept, now, naming, out),
        Tab::Subtitles => subtitles_tab(ui, kept, now, naming, out),
    }
}

/// The navigator's room on the Video tab.
const NAVIGATOR_HEIGHT: f32 = 120.0;

fn video_tab(ui: &mut Ui, adjust: &mut Adjust, kept: &mut Kept<'_>, now: Now<'_>) {
    if section(ui, "video_colour", tr!("Colour", "Цвет"), "") {
        for c in Color::ALL {
            let mut value = adjust.get(c);
            int_row(
                ui,
                c.label(),
                &mut value,
                &Spec::centred(-100.0..=100.0, 0.0),
            );
            adjust.set(c, value);
        }
    }

    if section(ui, "video_zoom", tr!("Zoom and pan", "Масштаб и сдвиг"), "") {
        let zoom = Spec {
            step: 5.0,
            suffix: "%",
            ..Spec::centred(range_of(adjust::ZOOM_RANGE), 100.0)
        };
        int_row(ui, tr!("Zoom", "Масштаб"), &mut adjust.zoom, &zoom);
        ui.add_space(4.0);
        // Of one height whatever the zoom: the panel is as tall as its
        // tallest tab, and the navigator's shape follows the zoom.
        let (rect, _) =
            ui.allocate_exact_size(vec2(ui.available_width(), NAVIGATOR_HEIGHT), Sense::hover());
        let size = now.geometry.size(adjust, rect.size());
        super::navigator::show(
            ui,
            Rect::from_center_size(rect.center(), size),
            ui.id().with("panel_navigator"),
            now.geometry,
            adjust,
            style::CARD_BG,
        );
        ui.add_space(4.0);
        let tip = tr!(
            "The frame is the part of the picture on screen: drag it, or click where it should go; a double click centres the picture (Alt+arrows move it too). Right and down when positive, in percent of the picture as zoomed",
            "Рамка — часть кадра на экране: её можно перетащить или щёлкнуть, куда её поставить; двойной щелчок возвращает кадр в центр (сдвигают и Alt+стрелки). Положительный — вправо и вниз, в процентах увеличенного кадра"
        );
        labelled_tip(ui, tr!("Pan", "Сдвиг"), Some(tip), |ui| {
            for value in [&mut adjust.pan_x, &mut adjust.pan_y] {
                ui.scope(|ui| {
                    if value.abs() > 1e-9 {
                        ui.visuals_mut().override_text_color = Some(style::accent_hover());
                    }
                    let drag = egui::DragValue::new(value)
                        .speed(0.5)
                        .max_decimals(1)
                        .range(-adjust::PAN_MAX..=adjust::PAN_MAX)
                        .suffix("%");
                    ui.add_sized(vec2(64.0, ui.spacing().interact_size.y), drag)
                        .on_hover_text(tip);
                });
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let off = adjust.pan_x != 0.0 || adjust.pan_y != 0.0;
                if reset(ui, off, tr!("To the centre", "В центр")) {
                    (adjust.pan_x, adjust.pan_y) = (0.0, 0.0);
                }
            });
        });
        switch_line(
            ui,
            kept.navigator,
            tr!("Navigator on the picture", "Навигатор на изображении"),
            Some(tr!(
                "While the picture is zoomed in past the window, the navigator shows in its bottom right corner",
                "Пока кадр увеличен больше окна, навигатор виден в правом нижнем углу изображения"
            )),
        );
    }

    if section(ui, "video_enhancement", tr!("Enhancement", "Улучшение"), "") {
        let strength = |tip| Spec {
            suffix: "%",
            tip: Some(tip),
            ..Spec::centred(0.0..=100.0, 0.0)
        };
        int_row(
            ui,
            tr!("Sharpness", "Резкость"),
            &mut adjust.sharpen,
            &strength(tr!(
                "AMD's contrast adaptive sharpening (CAS), on the processor: hardware decoding then copies each frame back for it",
                "Адаптивная резкость AMD (CAS), на процессоре: при аппаратном декодировании кадры для неё копируются обратно в память"
            )),
        );
        int_row(
            ui,
            tr!("Noise reduction", "Шумоподавление"),
            &mut adjust.denoise,
            &strength(tr!(
                "Smooths out grain and compression noise (hqdn3d); 40% is FFmpeg's usual strength, more blurs. On the processor, as the sharpness",
                "Сглаживает зерно и шум сжатия (hqdn3d); 40 % — обычная сила FFmpeg, больше — размывает. На процессоре, как резкость"
            )),
        );
        switch_line(
            ui,
            &mut adjust.deband,
            tr!("Smooth gradient banding", "Сглаживание полос градиента"),
            Some(tr!(
                "Removes the steps in skies and dark scenes, on the graphics card",
                "Убирает ступеньки на небе и в тёмных сценах, на видеокарте"
            )),
        );
    }

    if section(ui, "video_picture", tr!("Picture", "Изображение"), "") {
        combo_row(
            ui,
            tr!("Shape", "Форма кадра"),
            "video_aspect",
            adjust.aspect.label(),
            None,
            |ui| {
                for a in Aspect::ALL {
                    ui.selectable_value(&mut adjust.aspect, a, a.label());
                }
            },
        );
        combo_row(
            ui,
            tr!("Deinterlacing", "Чересстрочность"),
            "video_deinterlace",
            adjust.deinterlace.label(),
            Some(tr!(
                "Weaves interlaced pictures (TV recordings) into whole frames; Auto: the frames marked interlaced",
                "Сшивает чересстрочные кадры (ТВ-записи) в целые; «Авто» — только помеченные как чересстрочные"
            )),
            |ui| {
                for d in Deinterlace::ALL {
                    ui.selectable_value(&mut adjust.deinterlace, d, d.label());
                }
            },
        );
        combo_row(
            ui,
            tr!("HDR on SDR", "HDR на SDR"),
            "video_tone_map",
            adjust.tone_map.label(),
            Some(tr!(
                "How an HDR picture is brought down to an ordinary screen",
                "Как HDR-изображение приводится к обычному экрану"
            )),
            |ui| {
                for t in ToneMap::ALL {
                    ui.selectable_value(&mut adjust.tone_map, t, t.label());
                }
            },
        );
    }

    if section(
        ui,
        "video_rotation",
        tr!("Rotation and flips", "Поворот и отражение"),
        &format!("{}°", adjust.rotate),
    ) {
        // Four icons in a row, named in their tooltips: the turns, then the
        // flips (lit while on).
        let (flipped, mirrored) = (adjust.flip, adjust.mirror);
        let size = Some(vec2(52.0, 28.0));
        ui.horizontal(|ui| {
            let left = |p: &_, o, s, c| icons::turn(p, o, s, c, false);
            let right = |p: &_, o, s, c| icons::turn(p, o, s, c, true);
            let flip = |p: &_, o, s, c| icons::flip(p, o, s, c, false);
            let mirror = |p: &_, o, s, c| icons::flip(p, o, s, c, true);
            if icon_button(ui, left, "", size, |b| b)
                .on_hover_text(tr!("Turn left by 90°", "Повернуть влево на 90°"))
                .clicked()
            {
                adjust.turn(-1);
            }
            if icon_button(ui, right, "", size, |b| b)
                .on_hover_text(tr!("Turn right by 90°", "Повернуть вправо на 90°"))
                .clicked()
            {
                adjust.turn(1);
            }
            ui.add_space(12.0);
            if icon_button(ui, flip, "", size, |b| b.selected(flipped))
                .on_hover_text(tr!(
                    "Upside down (on the processor)",
                    "Перевернуть вертикально (на процессоре)"
                ))
                .clicked()
            {
                adjust.flip = !adjust.flip;
            }
            if icon_button(ui, mirror, "", size, |b| b.selected(mirrored))
                .on_hover_text(tr!(
                    "Mirror (on the processor)",
                    "Отразить зеркально (на процессоре)"
                ))
                .clicked()
            {
                adjust.mirror = !adjust.mirror;
            }
        });
    }

    ui.add_space(10.0);
    ui.horizontal(|ui| {
        if reset_all(ui, !adjust.is_default()) {
            *adjust = Adjust::default();
        }
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            switch_line_rtl(
                ui,
                kept.keep_adjust,
                tr!("Remember", "Запоминать"),
                tr!(
                    "The adjustments apply to every file until the program closes; on, they are kept between runs too",
                    "Настройки изображения действуют для всех файлов до закрытия программы; включено — сохраняются и между запусками"
                ),
            );
        });
    });
}

fn sound_tab(
    ui: &mut Ui,
    kept: &mut Kept<'_>,
    now: Now<'_>,
    naming: &mut Option<String>,
    out: &mut Vec<Cmd>,
) {
    if section(ui, "sound_now", tr!("Now playing", "Сейчас играет"), "") {
        now_playing(ui, &now);
    }

    if section(ui, "sound_output", tr!("Output", "Вывод"), "") {
        let device_tip = tr!(
            "The sound moves to it at once; kept between runs.",
            "Звук переходит на него сразу; запоминается между запусками."
        );
        labelled_tip(
            ui,
            tr!("Device", "Устройство"),
            Some(device_tip),
            |ui| {
                let width = ui.available_width();
                super::settings::device_choice(
                    ui,
                    "panel_audio_device",
                    kept.audio_device,
                    kept.devices,
                    width,
                );
            },
        );
        if !super::settings::device_present(kept.audio_device, kept.devices) {
            note(
                ui,
                tr!(
                    "Not connected: the sound goes to Windows' default until it is back.",
                    "Не подключено: звук идёт через устройство Windows по умолчанию, пока оно не вернётся."
                ),
            );
        }
        combo_row(
            ui,
            tr!("Channels", "Каналы"),
            "panel_channels",
            kept.channels.label(),
            Some(tr!(
                "Auto: as the device is set up in Windows, more channels mixed down to it; a layout chosen by hand is what the sound is mixed to",
                "«Авто» — как устройство настроено в Windows, лишние каналы сводятся под него; раскладка, выбранная вручную, — то, под что сводится звук"
            )),
            |ui| {
                for c in Channels::ALL {
                    ui.selectable_value(kept.channels, c, c.label());
                }
            },
        );
        combo_row(
            ui,
            tr!("Receiver", "Ресивер"),
            "panel_passthrough",
            kept.passthrough.label(),
            Some(tr!(
                "Dolby and DTS sound goes out compressed and the receiver decodes it; the volume, normalization, equalizer and channels here do not apply to it, and Windows gives the device to the player alone. A format the device does not take is decoded as usual",
                "Звук Dolby и DTS уходит в сжатом виде, его декодирует ресивер; громкость, нормализация, эквалайзер и каналы программы на него не действуют, а Windows отдаёт устройство только плееру. Формат, который устройство не принимает, декодируется как обычно"
            )),
            |ui| {
                for p in Passthrough::ALL {
                    ui.selectable_value(kept.passthrough, p, p.label());
                }
            },
        );
        let mut delay = now.audio_delay;
        if value_row(
            ui,
            tr!("Delay", "Задержка"),
            &mut delay,
            &Spec::delay(&delay_tip(
                tr!(
                    "Later when positive; not kept between runs.",
                    "Положительная — звук позже; между запусками не запоминается."
                ),
                Cmd::AudioDelay(1.0),
                Cmd::AudioDelay(-1.0),
            )),
        ) {
            out.push(Cmd::SetAudioDelay(delay));
        }
    }

    if section(ui, "sound_volume", tr!("Volume", "Громкость"), "") {
        combo_row(
            ui,
            tr!("Boost", "Усиление"),
            "panel_volume_max",
            &audio::boost_label(*kept.volume_max),
            Some(tr!(
                "How far the volume goes past 100%; a loud passage may then distort",
                "Насколько громкость может превышать 100 %; на громких местах возможны искажения"
            )),
            |ui| {
                for b in audio::BOOSTS {
                    ui.selectable_value(kept.volume_max, b, audio::boost_label(b));
                }
            },
        );
        switch_line(
            ui,
            kept.normalize,
            tr!("Normalization", "Нормализация"),
            Some(tr!(
                "Evens out the volume as the film plays: quiet dialogue louder, loud scenes quieter. Not for sound sent to a receiver undecoded",
                "Выравнивает громкость по ходу фильма: тихие диалоги громче, громкие сцены тише. Не действует на звук, передаваемый на ресивер без декодирования"
            )),
        );
        // A function's settings only while it is on: the tab grew long.
        if *kept.normalize {
            combo_row(
                ui,
                tr!("Strength", "Сила"),
                "panel_normalize_strength",
                kept.normalize_strength.label(),
                Some(tr!(
                    "Soft: raises quiet passages 3 times at most and follows slowly, the change hardly heard. Medium: 5 times. Strong: 10 times and quickly, whispers and blasts alike, for the night",
                    "Мягкая: поднимает тихие места не больше чем в 3 раза и подстраивается медленно, изменения почти не слышны. Средняя: в 5 раз. Сильная: в 10 раз и быстро, шёпот и взрывы почти вровень, для ночного просмотра"
                )),
                |ui| {
                    for n in audio::Normalize::ALL {
                        ui.selectable_value(kept.normalize_strength, n, n.label());
                    }
                },
            );
        }
        switch_line(
            ui,
            kept.voice_boost,
            tr!("Clearer speech", "Усилить речь"),
            Some(tr!(
                "Raises the voice over the music: in stereo through FFmpeg's dialogue enhancer, in 5.1 and more through the centre channel; mono stays as it is. Not for sound sent to a receiver undecoded",
                "Поднимает голос над музыкой: в стерео — фильтром FFmpeg для диалогов, в 5.1 и больше — центральным каналом; моно остаётся как есть. Не действует на звук, передаваемый на ресивер без декодирования"
            )),
        );
        if *kept.voice_boost {
            let strength = Spec {
                suffix: tr!(" dB", " дБ"),
                tip: Some(tr!(
                    "How much louder the voice is than the rest: the centre channel is raised up to 12 dB, past that the other channels are lowered instead, so that a loud voice does not distort; in stereo the dialogue enhancer works as strongly first",
                    "Насколько голос громче остального: центральный канал поднимается до 12 дБ, сильнее — вместо этого приглушаются остальные каналы, чтобы громкий голос не хрипел; в стерео сначала с той же силой работает фильтр для диалогов"
                )),
                ..Spec::centred(range_of(audio::VOICE_RANGE), audio::VOICE_DEFAULT as f64)
            };
            int_row(ui, tr!("Strength", "Сила"), kept.voice_db, &strength);
        }
        let takes = kept.channels.takes_upmix();
        ui.add_enabled_ui(takes, |ui| {
        let tip = if takes {
            tr!(
                "Spreads a stereo track over 5.1: what both sides share, mostly the voice, to the centre, what differs to the surrounds, the low end to the subwoofer. For speakers that take 5.1 or more (as set up in Windows, or Channels above); other tracks stay as they are, and sound sent to a receiver undecoded too",
                "Раскладывает стереодорожку на 5.1: общее для обеих сторон, в основном голос, — в центр, различия — в тыловые, низкие частоты — на сабвуфер. Для акустики, которая принимает 5.1 и больше (по настройке Windows или «Каналам» выше); другие дорожки остаются как есть, как и звук, передаваемый на ресивер без декодирования"
            )
        } else {
            tr!(
                "Not while Channels above makes the sound mono or stereo",
                "Недоступно, пока «Каналы» выше сводят звук в моно или стерео"
            )
        };
        switch_line(ui, kept.upmix, tr!("Stereo to 5.1", "Стерео в 5.1"), Some(tip));
    });
        // The subwoofer of the upmix, while it is on.
        if takes && *kept.upmix {
            let d = audio::Upmix::default();
            let cut = Spec {
                step: 5.0,
                suffix: tr!(" Hz", " Гц"),
                tip: Some(tr!(
                    "Below it the low end goes to the subwoofer, fading out up to twice that. 80 Hz is the cinemas' crossover; higher, the bass starts to sound as if it came from the subwoofer",
                    "Ниже неё низкие частоты уходят на сабвуфер, затухая до удвоенной частоты. 80 Гц — разделение, принятое в кинотеатрах; выше бас начинает звучать так, будто идёт от сабвуфера"
                )),
                ..Spec::centred(range_of(audio::LFE_CUT_RANGE), d.lfe_cut as f64)
            };
            int_row(
                ui,
                tr!("Subwoofer up to", "Сабвуфер до"),
                &mut kept.upmix_lfe.lfe_cut,
                &cut,
            );
            switch_line(
                ui,
                &mut kept.upmix_lfe.bass_only,
                tr!("Bass to the subwoofer only", "Басы только в сабвуфер"),
                Some(tr!(
                    "The low end taken out of the other speakers, not only copied to the subwoofer: for small front speakers",
                    "Низкие частоты убираются из остальных колонок, а не только дублируются в сабвуфер: для небольших фронтальных колонок"
                )),
            );
        }
    }

    if section(ui, "sound_subwoofer", tr!("Subwoofer", "Сабвуфер"), "") {
        let d = audio::Subwoofer::default();
        let level = Spec {
            suffix: tr!(" dB", " дБ"),
            tip: Some(tr!(
                "The subwoofer channel of 5.1 and 7.1 tracks against the track's own level; with Stereo to 5.1, against the standard one (10 dB under the others: receivers and sound systems play it that much louder). Lower if the bass booms",
                "Канал сабвуфера дорожек 5.1 и 7.1 относительно уровня в самой дорожке; со «Стерео в 5.1» — относительно стандартного (на 10 дБ ниже остальных: ресиверы и звуковые системы воспроизводят его настолько громче). Если бас бухает, ниже"
            )),
            ..Spec::centred(range_of(audio::LFE_LEVEL_RANGE), d.level as f64)
        };
        int_row(
            ui,
            tr!("Level", "Уровень"),
            &mut kept.subwoofer.level,
            &level,
        );
        switch_line(
            ui,
            &mut kept.subwoofer.infrasonic,
            tr!("Cut the infrasound", "Срез инфранизких"),
            Some(tr!(
                "Takes out what lies under the frequency below in all channels: a subwoofer hardly plays it, but its cone is driven to the limit, and the bass bubbles and chuffs. Films with deep bass have much of it. Not for sound sent to a receiver undecoded",
                "Убирает во всех каналах то, что ниже частоты под переключателем: сабвуфер это почти не воспроизводит, но его диффузор доходит до предела хода, и бас булькает и хлопает. В фильмах с глубоким басом такого много. Не действует на звук, передаваемый на ресивер без декодирования"
            )),
        );
        if kept.subwoofer.infrasonic {
            let cut = Spec {
                suffix: tr!(" Hz", " Гц"),
                tip: Some(tr!(
                    "Half as loud at it and fading by 24 dB an octave below. Higher for a small subwoofer, lower for a large one",
                    "На ней звук вдвое тише, ниже затухает на 24 дБ на октаву. Для небольшого сабвуфера выше, для большого ниже"
                )),
                ..Spec::centred(range_of(audio::INFRASONIC_RANGE), d.infrasonic_hz as f64)
            };
            int_row(
                ui,
                tr!("Below", "Ниже"),
                &mut kept.subwoofer.infrasonic_hz,
                &cut,
            );
        }
    }

    if section(ui, "sound_equalizer", tr!("Equalizer", "Эквалайзер"), "") {
        eq_head(ui, kept.equalizer, kept.eq_presets, naming);
        bands(ui, kept.equalizer);
    }

    // What the sound is made into; where it goes (the device, a receiver)
    // and the user's own gains stay.
    let d = crate::settings::Settings::default();
    let changed = *kept.channels != d.audio_channels
        || *kept.volume_max != d.volume_max
        || *kept.normalize != d.normalize
        || *kept.normalize_strength != d.normalize_strength
        || *kept.voice_boost != d.voice_boost
        || *kept.voice_db != d.voice_db
        || *kept.upmix != d.upmix
        || *kept.upmix_lfe != d.upmix_lfe
        || *kept.subwoofer != d.subwoofer
        || *kept.equalizer != d.equalizer
        || now.audio_delay != 0.0;
    ui.add_space(10.0);
    if reset_all(ui, changed) {
        *kept.channels = d.audio_channels;
        *kept.volume_max = d.volume_max;
        *kept.normalize = d.normalize;
        *kept.normalize_strength = d.normalize_strength;
        *kept.voice_boost = d.voice_boost;
        *kept.voice_db = d.voice_db;
        *kept.upmix = d.upmix;
        *kept.upmix_lfe = d.upmix_lfe;
        *kept.subwoofer = d.subwoofer;
        *kept.equalizer = d.equalizer;
        if now.audio_delay != 0.0 {
            out.push(Cmd::SetAudioDelay(0.0));
        }
    }
}

/// The audio track playing: its channels drawn as speakers round the
/// listener, beside them its format as the decoder names it (the codec's
/// full name in the tooltip), its channels, rate and bitrate, and what the
/// device gets of it.
fn now_playing(ui: &mut Ui, now: &Now<'_>) {
    let lang = crate::i18n::lang();
    let Some(track) = now.audio else {
        note(ui, tr!("No sound is playing.", "Звук не играет."));
        return;
    };
    // What the device gets, when mixed to other channels (passthrough
    // says 2.0 for a stream of any).
    let out = now
        .audio_out
        .filter(|o| !o.format.as_deref().is_some_and(|f| f.starts_with("spdif")));
    let channels = track.channels.unwrap_or(2);
    let mixed = out
        .and_then(|o| o.channel_count)
        .filter(|n| *n > 0 && *n != channels);
    // Two icons and the arrow take less each.
    let scale = if mixed.is_some() { 0.8 } else { 1.0 };
    ui.horizontal_top(|ui| {
        let icon = speakers(ui, channels, track.layout.as_deref(), scale);
        icon.on_hover_text(match &track.layout {
            Some(layout) => tr!(format!("Channels: {layout}"), format!("Каналы: {layout}")),
            None => tr!(
                format!("Channels: {channels}"),
                format!("Каналов: {channels}")
            ),
        });
        if let Some(n) = mixed {
            let layout = out.and_then(|o| o.channels.as_deref());
            arrow(ui, 50.0 * scale);
            let name = layout.map_or_else(|| n.to_string(), str::to_owned);
            speakers(ui, n, layout, scale).on_hover_text(if now.upmix && channels == 2 {
                tr!(
                    format!("{name} for the device: Stereo to 5.1 spreads the track"),
                    format!("{name} для устройства: «Стерео в 5.1» раскладывает дорожку")
                )
            } else if now.voice_boost && channels == 2 {
                tr!(
                    format!("{name} for the device: Clearer speech adds a centre"),
                    format!("{name} для устройства: «Усилить речь» добавляет центр")
                )
            } else {
                tr!(
                    format!("Mixed to {name} for the device (the Channels setting)"),
                    format!("Сводится в {name} для устройства (настройка «Каналы»)")
                )
            });
        }
        ui.add_space(8.0);
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 2.0;
            let format = track
                .format_name()
                .unwrap_or_else(|| tr!("Unknown", "Неизвестный").to_owned());
            let title = ui.label(RichText::new(format).size(15.0).color(style::TEXT_BRIGHT));
            if let Some(desc) = &track.codec_desc {
                title.on_hover_text(desc);
            }
            let mut parts = vec![
                track
                    .channels_label(lang)
                    .unwrap_or_else(|| crate::tracks::channels_in(lang, channels)),
            ];
            if let Some(rate) = track.samplerate.filter(|r| *r > 0) {
                let khz = format!(
                    "{:.*}",
                    crate::tracks::khz_decimals(rate),
                    rate as f64 / 1000.0
                );
                parts.push(tr!(
                    format!("{khz} kHz"),
                    format!("{} кГц", khz.replace('.', ","))
                ));
            }
            let bitrate = now
                .audio_bitrate
                .or(track.bitrate.filter(|b| *b > 0).map(|b| b as f64));
            if let Some(b) = bitrate {
                parts.push(crate::info::bitrate_in(lang, b));
            }
            ui.label(RichText::new(parts.join(" · ")).color(style::TEXT));
            if let Some(out) = now
                .audio_out
                .and_then(|o| crate::info::audio_out_in(lang, o))
            {
                ui.label(
                    RichText::new(tr!(
                        format!("To the device: {out}"),
                        format!("На устройство: {out}")
                    ))
                    .color(style::TEXT_WEAK),
                );
            }
        });
    });
}

/// A track's channels as speakers round the listener (a ring in the
/// middle): those it has lit in the accent colour, the rest of 7.1's
/// places faint; the low frequencies a wider block at the front right.
fn speakers(ui: &mut Ui, channels: i64, layout: Option<&str>, scale: f32) -> egui::Response {
    #[derive(Clone, Copy, PartialEq)]
    enum At {
        FrontLeft,
        FrontRight,
        Centre,
        Low,
        SideLeft,
        SideRight,
        BackLeft,
        BackRight,
        BackCentre,
    }
    use At::*;
    let lfe = layout.is_some_and(|l| l.contains(".1"));
    let lit: &[At] = match channels {
        1 => &[Centre],
        2 => &[FrontLeft, FrontRight],
        3 if lfe => &[FrontLeft, FrontRight, Low],
        3 => &[FrontLeft, FrontRight, Centre],
        4 if lfe => &[FrontLeft, FrontRight, Centre, Low],
        4 => &[FrontLeft, FrontRight, BackLeft, BackRight],
        5 if lfe => &[FrontLeft, FrontRight, Low, SideLeft, SideRight],
        5 => &[FrontLeft, FrontRight, Centre, SideLeft, SideRight],
        6 => &[FrontLeft, FrontRight, Centre, Low, SideLeft, SideRight],
        7 => &[
            FrontLeft, FrontRight, Centre, Low, SideLeft, SideRight, BackCentre,
        ],
        _ => &[
            FrontLeft, FrontRight, Centre, Low, SideLeft, SideRight, BackLeft, BackRight,
        ],
    };
    let (rect, response) = ui.allocate_exact_size(vec2(60.0, 50.0) * scale, Sense::hover());
    let painter = ui.painter();
    let o = rect.center();
    let at = |x: f32, y: f32| o + vec2(x * 26.0, y * 21.0) * scale;
    let places = [
        (FrontLeft, at(-0.75, -0.8)),
        (FrontRight, at(0.75, -0.8)),
        (Centre, at(0.0, -0.95)),
        (SideLeft, at(-1.0, 0.15)),
        (SideRight, at(1.0, 0.15)),
        (BackLeft, at(-0.6, 0.9)),
        (BackRight, at(0.6, 0.9)),
        (BackCentre, at(0.0, 0.95)),
    ];
    let color = |place: At| {
        if lit.contains(&place) {
            style::accent()
        } else {
            style::TRACK
        }
    };
    for (place, p) in places {
        // The back centre only where there is one: it would sit on the
        // back pair's way.
        if place == BackCentre && !lit.contains(&BackCentre) {
            continue;
        }
        painter.rect_filled(
            Rect::from_center_size(p, vec2(7.0, 7.0) * scale),
            2.0,
            color(place),
        );
    }
    if lit.contains(&Low) {
        painter.rect_filled(
            Rect::from_center_size(at(0.42, -0.35), vec2(9.0, 5.0) * scale),
            1.5,
            color(Low),
        );
    }
    painter.circle_stroke(
        at(0.0, 0.15),
        4.0 * scale,
        Stroke::new(1.3, style::TEXT_WEAK),
    );
    response
}

/// An arrow to the right, `height` high, between the track's speakers and
/// the device's.
fn arrow(ui: &mut Ui, height: f32) {
    let (rect, _) = ui.allocate_exact_size(vec2(16.0, height), Sense::hover());
    let stroke = Stroke::new(1.5, style::TEXT_WEAK);
    let (y, left, right) = (rect.center().y, rect.left() + 2.0, rect.right() - 2.0);
    let painter = ui.painter();
    painter.line_segment([pos2(left, y), pos2(right, y)], stroke);
    painter.line_segment([pos2(right - 4.0, y - 4.0), pos2(right, y)], stroke);
    painter.line_segment([pos2(right - 4.0, y + 4.0), pos2(right, y)], stroke);
}

/// The equalizer's switch, its presets and the user's own in one list, and
/// beside it saving the gains as one's own and deleting one's own; the
/// name is typed in a row under it.
fn eq_head(ui: &mut Ui, eq: &mut Equalizer, own: &mut Vec<OwnEq>, naming: &mut Option<String>) {
    let mine = own.iter().position(|o| o.gains == eq.gains);
    let preset = Preset::of(&eq.gains);
    ui.horizontal(|ui| {
        super::settings::switch(ui, &mut eq.on).on_hover_text(tr!(
            "On or off. Not for sound sent to a receiver undecoded",
            "Включён или выключен. Не действует на звук, передаваемый на ресивер без декодирования"
        ));
        ui.add_space(4.0);
        let name = match (mine, preset) {
            (Some(i), _) => own[i].name.clone(),
            (None, Some(p)) => p.label().to_owned(),
            (None, None) => tr!("Custom", "Другой").to_owned(),
        };
        let width = ui.available_width() - 2.0 * RESET - 3.0 * ui.spacing().item_spacing.x;
        egui::ComboBox::from_id_salt("eq_preset")
            .selected_text(name)
            .width(width)
            .show_ui(ui, |ui| {
                for p in Preset::ALL {
                    let on = mine.is_none() && preset == Some(p);
                    if ui.selectable_label(on, p.label()).clicked() {
                        eq.gains = p.gains();
                        eq.on = true;
                    }
                }
                if !own.is_empty() {
                    ui.separator();
                    ui.label(
                        RichText::new(tr!("Mine", "Мои"))
                            .small()
                            .color(style::TEXT_WEAK),
                    );
                    for (i, o) in own.iter().enumerate() {
                        if ui.selectable_label(mine == Some(i), &o.name).clicked() {
                            eq.gains = o.gains;
                            eq.on = true;
                        }
                    }
                }
            });
        if super::tool(
            ui,
            tr!("Save as my own…", "Сохранить как свой…"),
            naming.is_none(),
            |p, o, c| icons::plus(p, o, 6.0, c),
        )
        .clicked()
        {
            *naming = Some(mine.map(|i| own[i].name.clone()).unwrap_or_default());
        }
        if super::tool(
            ui,
            tr!("Delete these gains of mine", "Удалить этот свой эквалайзер"),
            mine.is_some(),
            |p, o, c| icons::trash(p, o, 6.0, c),
        )
        .clicked()
            && let Some(i) = mine
        {
            own.remove(i);
        }
    });
    let full = own.len() >= audio::MAX_OWN_EQ;
    let names: Vec<String> = own.iter().map(|o| o.name.clone()).collect();
    if let Some(name) = name_row(
        ui,
        naming,
        tr!("My equalizer", "Мой эквалайзер"),
        |n| full && !names.iter().any(|o| o == n),
    ) {
        OwnEq::save(own, &name, eq.gains);
    }
}

/// The bands: a vertical slider each with its gain over it and its
/// frequency and reset under it, a scale of ±12 dB at the left, the 0 dB
/// line across, and a curve through the knobs drawn under them.
fn bands(ui: &mut Ui, eq: &mut Equalizer) {
    let on = eq.on;
    ui.add_space(6.0);
    ui.add_enabled_ui(on, |ui| {
        let scale = 28.0;
        let column = (WIDTH - scale) / EQ_BANDS.len() as f32;
        let under = ui.painter().add(Shape::Noop);
        let mut knobs = Vec::new();
        let mut travel = None;
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            ui.spacing_mut().slider_width = 130.0;
            let (gutter, _) = ui.allocate_exact_size(vec2(scale, 10.0), Sense::hover());
            for (i, hz) in EQ_BANDS.iter().enumerate() {
                ui.allocate_ui_with_layout(
                    vec2(column, 216.0),
                    Layout::top_down(Align::Center),
                    |ui| {
                        let gain = eq.gains[i];
                        let text = if gain > 0 {
                            format!("+{gain}")
                        } else {
                            gain.to_string()
                        };
                        let color = if gain != 0 && on {
                            style::accent_hover()
                        } else {
                            style::TEXT_WEAK
                        };
                        ui.label(RichText::new(text).size(11.0).color(color));
                        let response = ui
                            .add(
                                egui::Slider::new(&mut eq.gains[i], -EQ_RANGE..=EQ_RANGE)
                                    .vertical()
                                    .show_value(false),
                            )
                            .on_hover_text(tr!(
                                format!("{hz} Hz; a double click puts it back to 0"),
                                format!("{hz} Гц; двойной щелчок возвращает 0")
                            ));
                        let r = response.rect;
                        let radius = r.width() / 2.5;
                        let (bottom, top) = (r.bottom() - radius, r.top() + radius);
                        let t = (eq.gains[i] + EQ_RANGE) as f32 / (2 * EQ_RANGE) as f32;
                        knobs.push(pos2(r.center().x, egui::lerp(bottom..=top, t)));
                        travel = Some((top, bottom));
                        ui.label(RichText::new(band_label(*hz)).size(11.0));
                        let off = eq.gains[i] != 0;
                        if response.double_clicked()
                            || reset(ui, off, tr!("Back to 0", "Вернуть 0"))
                        {
                            eq.gains[i] = 0;
                        }
                    },
                );
            }
            if let Some((top, bottom)) = travel {
                let x = gutter.right() - 6.0;
                let font = FontId::proportional(10.0);
                let painter = ui.painter();
                for (y, text) in [(top, "+12"), ((top + bottom) / 2.0, "0"), (bottom, "−12")] {
                    painter.text(
                        pos2(x, y),
                        Align2::RIGHT_CENTER,
                        text,
                        font.clone(),
                        style::TEXT_WEAK,
                    );
                }
            }
        });
        let mut shapes = Vec::new();
        if let (Some((top, bottom)), Some(first), Some(last)) =
            (travel, knobs.first(), knobs.last())
        {
            let zero = (top + bottom) / 2.0;
            let span = (first.x - column / 2.0)..=(last.x + column / 2.0);
            shapes.push(Shape::hline(span, zero, Stroke::new(1.0, style::LINE)));
        }
        let curve = if on {
            style::accent().gamma_multiply(0.8)
        } else {
            style::TEXT_WEAK.gamma_multiply(0.4)
        };
        shapes.push(Shape::line(smooth(&knobs), Stroke::new(2.0, curve)));
        ui.painter().set(under, Shape::Vec(shapes));
    });
}

/// A curve through `points` (Catmull-Rom, eight steps a span).
fn smooth(points: &[egui::Pos2]) -> Vec<egui::Pos2> {
    if points.len() < 3 {
        return points.to_vec();
    }
    let at = |i: isize| points[i.clamp(0, points.len() as isize - 1) as usize].to_vec2();
    let mut out = Vec::new();
    for i in 0..points.len() as isize - 1 {
        let (p0, p1, p2, p3) = (at(i - 1), at(i), at(i + 1), at(i + 2));
        for step in 0..8 {
            let t = step as f32 / 8.0;
            let (t2, t3) = (t * t, t * t * t);
            let v = 0.5
                * ((2.0 * p1)
                    + (p2 - p0) * t
                    + (2.0 * p0 - 5.0 * p1 + 4.0 * p2 - p3) * t2
                    + (3.0 * p1 - p0 - 3.0 * p2 + p3) * t3);
            out.push(v.to_pos2());
        }
    }
    out.push(*points.last().expect("three points or more"));
    out
}

fn subtitles_tab(
    ui: &mut Ui,
    kept: &mut Kept<'_>,
    now: Now<'_>,
    naming: &mut Option<String>,
    out: &mut Vec<Cmd>,
) {
    let style = &mut *kept.subs;
    let d = SubStyle::default();
    let text_only = tr!(
        "For text subtitles (SRT and the like), and ASS ones with Style for ASS too",
        "Для текстовых субтитров (SRT и подобных), а со «Стиль и для ASS» — и для ASS"
    );
    if section(ui, "subs_style", tr!("Style", "Стиль"), "") {
        looks(ui, style, kept.sub_presets, naming);
        ui.add_space(4.0);
        preview(ui, style);
    }

    if section(ui, "subs_text", tr!("Text", "Текст"), "") {
        labelled_tip(ui, tr!("Font", "Шрифт"), Some(text_only), |ui| {
            let width = ui.available_width() - RESET - ui.spacing().item_spacing.x;
            if let Some(family) =
                super::settings::font_choice(ui, "panel_sub_font", &style.font, width)
            {
                style.font = family.map(|f| f.name.clone()).unwrap_or_default();
            }
            if reset(ui, style.font != d.font, tr!("Default", "По умолчанию")) {
                style.font.clear();
            }
        });
        let size = Spec {
            step: 5.0,
            suffix: "%",
            tip: Some(tr!("For all subtitles", "Для всех субтитров")),
            ..Spec::centred(sub_style::SCALE_RANGE, d.scale)
        };
        value_row(ui, tr!("Size", "Размер"), &mut style.scale, &size);
        switch_line(ui, &mut style.bold, tr!("Bold", "Жирные"), None);
    }

    if section(ui, "subs_colour", tr!("Colour", "Цвет"), "") {
        color_row(
            ui,
            tr!("Letters", "Буквы"),
            text_only,
            &mut style.color,
            d.color,
        );
        // With the box, the outline's colour and thickness are the box's
        // colour and margin.
        let (outline, thickness) = if style.boxed {
            (tr!("Box", "Подложка"), tr!("Margin", "Поля"))
        } else {
            (tr!("Outline", "Обводка"), tr!("Thickness", "Толщина"))
        };
        color_row(
            ui,
            outline,
            text_only,
            &mut style.outline_color,
            d.outline_color,
        );
        let thick = Spec {
            step: 0.05,
            decimals: 2,
            default: d.outline,
            ..Spec::centred(sub_style::OUTLINE_RANGE, 0.0)
        };
        value_row(ui, thickness, &mut style.outline, &thick);
        let shadow = Spec {
            step: 0.25,
            decimals: 2,
            ..Spec::centred(sub_style::SHADOW_RANGE, d.shadow)
        };
        value_row(ui, tr!("Shadow", "Тень"), &mut style.shadow, &shadow);
        switch_line(
            ui,
            &mut style.boxed,
            tr!("A box behind the text", "Подложка под текстом"),
            Some(tr!(
                "A slightly see-through box behind each line in place of the outline, of the outline's colour",
                "Полупрозрачная плашка под каждой строкой вместо обводки, цвета обводки"
            )),
        );
    }

    if section(ui, "subs_place", tr!("Place", "Положение"), "") {
        let height = Spec {
            tip: Some(tr!(
                "For all subtitles; 100 is the bottom, 0 the top",
                "Для всех субтитров; 100 — внизу, 0 — вверху"
            )),
            ..Spec::centred(sub_style::POS_RANGE, d.pos)
        };
        value_row(ui, tr!("Height", "Высота"), &mut style.pos, &height);
        switch_line(
            ui,
            kept.sub_margins,
            tr!("In the black bars", "В чёрных полях"),
            Some(tr!(
                "Where the picture has black bars, the subtitles go in them, ASS ones too; off, over the picture",
                "Если у изображения есть чёрные поля, субтитры, в том числе ASS, идут в них; выключено — поверх изображения"
            )),
        );
        switch_line(
            ui,
            kept.sub_ass_override,
            tr!("Style for ASS too", "Стиль и для ASS"),
            Some(tr!(
                "The font, the colours and the outline above replace those of ASS subtitles too (their signs and effects lose their look)",
                "Шрифт, цвета и обводка выше заменяют и стили субтитров ASS (надписи и эффекты в них теряют свой вид)"
            )),
        );
    }

    if section(ui, "subs_timing", tr!("Timing", "Синхронизация"), "") {
        let mut delay = now.sub_delay;
        if value_row(
            ui,
            tr!("Delay", "Задержка"),
            &mut delay,
            &Spec::delay(&delay_tip(
                tr!(
                    "Later when positive; not kept between runs.",
                    "Положительная — субтитры позже; между запусками не запоминается."
                ),
                Cmd::SubDelay(1.0),
                Cmd::SubDelay(-1.0),
            )),
        ) {
            out.push(Cmd::SetSubDelay(delay));
        }
        super::sub_timeline::show(
            ui,
            super::sub_timeline::Timeline {
                lines: now.sub_lines,
                time: now.time,
                delay: now.sub_delay,
                subs_on: now.subs_on,
                subs_external: now.subs_external,
            },
            out,
        );
    }

    ui.add_space(10.0);
    if reset_all(ui, *style != d) {
        *style = d;
    }
}

/// The ready-made looks and the user's own (the whole style, by name) in
/// one list; beside it, saving the look as one's own and, for one's own,
/// deleting it; the name is typed in a row under it.
fn looks(
    ui: &mut Ui,
    style: &mut SubStyle,
    own: &mut Vec<sub_style::Own>,
    naming: &mut Option<String>,
) {
    let mine = own.iter().position(|o| o.style == *style);
    let preset = sub_style::Preset::of(style);
    labelled(ui, tr!("Ready-made", "Заготовка"), |ui| {
        let name = match (mine, preset) {
            (Some(i), _) => own[i].name.clone(),
            (None, Some(p)) => p.label().to_owned(),
            (None, None) => tr!("Custom", "Другой").to_owned(),
        };
        let width = ui.available_width() - 2.0 * RESET - 3.0 * ui.spacing().item_spacing.x;
        egui::ComboBox::from_id_salt("sub_preset")
            .selected_text(name)
            .width(width)
            .show_ui(ui, |ui| {
                for p in sub_style::Preset::ALL {
                    let on = mine.is_none() && preset == Some(p);
                    if ui.selectable_label(on, p.label()).clicked() {
                        p.apply(style);
                    }
                }
                if !own.is_empty() {
                    ui.separator();
                    ui.label(
                        RichText::new(tr!("Mine", "Мои"))
                            .small()
                            .color(style::TEXT_WEAK),
                    );
                    for (i, o) in own.iter().enumerate() {
                        if ui.selectable_label(mine == Some(i), &o.name).clicked() {
                            *style = o.style.clone();
                        }
                    }
                }
            });
        if super::tool(
            ui,
            tr!("Save as my own…", "Сохранить как свою…"),
            naming.is_none(),
            |p, o, c| icons::plus(p, o, 6.0, c),
        )
        .clicked()
        {
            *naming = Some(mine.map(|i| own[i].name.clone()).unwrap_or_default());
        }
        if super::tool(
            ui,
            tr!("Delete this look of mine", "Удалить эту свою заготовку"),
            mine.is_some(),
            |p, o, c| icons::trash(p, o, 6.0, c),
        )
        .clicked()
            && let Some(i) = mine
        {
            own.remove(i);
        }
    });
    let full = own.len() >= sub_style::MAX_OWN;
    let names: Vec<String> = own.iter().map(|o| o.name.clone()).collect();
    if let Some(name) = name_row(
        ui,
        naming,
        tr!("My look", "Моя заготовка"),
        |n| full && !names.iter().any(|o| o == n),
    ) {
        sub_style::Own::save(own, &name, style);
    }
}

/// The row where a name is typed for what is to be saved, while `naming`
/// holds it: Enter or the tick saves (the name returned), Esc or the cross
/// gives up; the player's keys wait meanwhile. `drops_oldest`: saving that
/// name drops the oldest of a full list, which is said under it.
fn name_row(
    ui: &mut Ui,
    naming: &mut Option<String>,
    hint: &str,
    drops_oldest: impl Fn(&str) -> bool,
) -> Option<String> {
    let name = naming.as_mut()?;
    let (mut done, mut saved) = (false, None);
    labelled(ui, tr!("Name", "Название"), |ui| {
        let width = ui.available_width() - 2.0 * RESET - 3.0 * ui.spacing().item_spacing.x;
        let field = ui.add(
            egui::TextEdit::singleline(name)
                .hint_text(hint)
                .desired_width(width),
        );
        if ui.memory(|m| m.focused().is_none()) {
            field.request_focus();
        }
        let filled = !name.trim().is_empty();
        let enter = field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        let ok = super::tool(ui, tr!("Save", "Сохранить"), filled, |p, o, c| {
            icons::check(p, o, 6.0, c)
        });
        if (enter || ok.clicked()) && filled {
            saved = Some(name.trim().to_owned());
            done = true;
        }
        let cancel = super::tool(ui, tr!("Cancel", "Отмена"), true, |p, o, c| {
            icons::close(p, o, 5.0, c)
        });
        if cancel.clicked() || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            done = true;
        }
    });
    if drops_oldest(name.trim()) {
        note(
            ui,
            tr!(
                "The list is full: the oldest of mine gives way.",
                "Список полон: самая старая своя уступит место."
            ),
        );
    }
    if done {
        *naming = None;
    }
    saved
}

/// What a value's row shows and does.
#[derive(Clone)]
struct Spec<'a> {
    range: RangeInclusive<f64>,
    step: f64,
    decimals: usize,
    suffix: &'a str,
    /// What the round arrow puts back.
    default: f64,
    /// Where the rail is lit from.
    origin: f64,
    /// The value kept within the range (a delay set further by the keys
    /// shows as it is).
    clamp: bool,
    tip: Option<&'a str>,
    value_width: f32,
}

impl<'a> Spec<'a> {
    /// Whole steps, lit from and put back to `origin`.
    fn centred(range: RangeInclusive<f64>, origin: f64) -> Self {
        Self {
            range,
            step: 1.0,
            decimals: 0,
            suffix: "",
            default: origin,
            origin,
            clamp: true,
            tip: None,
            value_width: VALUE,
        }
    }

    /// A delay in seconds, ±[`DELAY_RANGE`] on the slider.
    fn delay(tip: &'a str) -> Self {
        Self {
            step: audio::DELAY_STEP / 2.0,
            decimals: 2,
            suffix: tr!(" s", " с"),
            clamp: false,
            tip: Some(tip),
            value_width: DELAY_VALUE,
            ..Self::centred(-DELAY_RANGE..=DELAY_RANGE, 0.0)
        }
    }
}

/// A range of whole numbers as one of `f64`.
fn range_of(range: RangeInclusive<i64>) -> RangeInclusive<f64> {
    *range.start() as f64..=*range.end() as f64
}

/// A value's row: the label, the slider ([`rail`]), the value, which can
/// be dragged or typed too (in the accent colour while off its default),
/// and the round arrow back to the default (also a double click on the
/// slider). Whether the value changed.
fn value_row(ui: &mut Ui, label: &str, value: &mut f64, spec: &Spec) -> bool {
    let before = *value;
    labelled_tip(ui, label, spec.tip, |ui| {
        let spacing = ui.spacing().item_spacing.x;
        let width = WIDTH - LABEL - spec.value_width - RESET - 3.0 * spacing;
        let mut response = rail(ui, value, spec, width);
        if let Some(tip) = spec.tip {
            response = response.on_hover_text(tip);
        }
        let off = (*value - spec.default).abs() > 1e-9;
        ui.scope(|ui| {
            if off {
                ui.visuals_mut().override_text_color = Some(style::accent_hover());
            }
            let mut drag = egui::DragValue::new(value)
                .speed(spec.step)
                .fixed_decimals(spec.decimals)
                .suffix(spec.suffix);
            if spec.clamp {
                drag = drag.range(spec.range.clone());
            }
            ui.add_sized(vec2(spec.value_width, ui.spacing().interact_size.y), drag);
        });
        if response.double_clicked()
            || reset(ui, off, tr!("Back to the default", "Вернуть исходное"))
        {
            *value = spec.default;
        }
    });
    (*value - before).abs() > 1e-12
}

/// [`value_row`] of a whole number.
fn int_row(ui: &mut Ui, label: &str, value: &mut i64, spec: &Spec) -> bool {
    let mut v = *value as f64;
    let changed = value_row(ui, label, &mut v, spec);
    *value = v.round() as i64;
    changed
}

/// A delay's tooltip: `text`, then the keys of `later` and `earlier` as
/// they are set.
fn delay_tip(text: &str, later: Cmd, earlier: Cmd) -> String {
    let keys: Vec<String> = [later, earlier]
        .map(crate::keymap::shortcut)
        .into_iter()
        .filter(|k| !k.is_empty())
        .collect();
    match keys.as_slice() {
        [] => text.to_owned(),
        [one] => tr!(
            format!("{text} Also {one}."),
            format!("{text} Также {one}.")
        ),
        [a, b, ..] => tr!(
            format!("{text} Also {a} and {b}."),
            format!("{text} Также {a} и {b}.")
        ),
    }
}

/// A slider `width` wide: a rail lit in the accent colour from the spec's
/// origin to the knob, so a step off it shows at once, with a tick at the
/// origin when it is inside the range. A press or a drag sets the value to
/// the nearest step.
fn rail(ui: &mut Ui, value: &mut f64, spec: &Spec, width: f32) -> egui::Response {
    let height = ui.spacing().interact_size.y;
    let (rect, mut response) = ui.allocate_exact_size(vec2(width, height), Sense::click_and_drag());
    let (lo, hi) = (*spec.range.start(), *spec.range.end());
    let (left, right) = (rect.left() + KNOB, rect.right() - KNOB);
    let x_of = |v: f64| egui::lerp(left..=right, ((v - lo) / (hi - lo)).clamp(0.0, 1.0) as f32);
    if (response.dragged() || response.clicked())
        && let Some(p) = response.interact_pointer_pos()
    {
        let t = f64::from(((p.x - left) / (right - left)).clamp(0.0, 1.0));
        // To the spec's decimals as well: 33 × 0.05 is 1.6500000000000001,
        // which no longer matched a ready look's 1.65.
        let v = ((lo + t * (hi - lo)) / spec.step).round() * spec.step;
        let v = egui::emath::round_to_decimals(v, spec.decimals);
        if (v - *value).abs() > 1e-9 {
            *value = v;
            response.mark_changed();
        }
    }
    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        let y = rect.center().y;
        let track = Rect::from_x_y_ranges(left..=right, (y - 2.0)..=(y + 2.0));
        painter.rect_filled(track, 2.0, style::TRACK);
        let (from, to) = (x_of(spec.origin), x_of(*value));
        if (from - to).abs() > 0.5 {
            let lit = Rect::from_x_y_ranges(from.min(to)..=from.max(to), (y - 2.0)..=(y + 2.0));
            let color = if ui.is_enabled() {
                style::accent()
            } else {
                style::TEXT_WEAK
            };
            painter.rect_filled(lit, 2.0, color);
        }
        if spec.origin > lo && spec.origin < hi {
            painter.vline(
                from,
                (y - 5.0)..=(y + 5.0),
                Stroke::new(1.0, style::TEXT_WEAK),
            );
        }
        let active = response.hovered() || response.dragged();
        let knob = if active {
            style::TEXT_BRIGHT
        } else {
            style::TEXT
        };
        painter.circle_filled(pos2(to, y), if active { KNOB + 1.0 } else { KNOB }, knob);
    }
    response
}

/// A switch with its text after it (a click on either turns it); `tip`
/// on both.
fn switch_line(ui: &mut Ui, on: &mut bool, text: &str, tip: Option<&str>) {
    ui.horizontal(|ui| {
        let mut switch = super::settings::switch(ui, on);
        let mut label = ui.add(egui::Label::new(text).sense(Sense::click()));
        if label.clicked() {
            *on = !*on;
        }
        if let Some(tip) = tip {
            switch = switch.on_hover_text(tip);
            label = label.on_hover_text(tip);
        }
        let _ = (switch, label);
    });
}

/// [`switch_line`] laid out from the right: the text, then the switch.
fn switch_line_rtl(ui: &mut Ui, on: &mut bool, text: &str, tip: &str) {
    super::settings::switch(ui, on).on_hover_text(tip);
    if ui
        .add(egui::Label::new(text).sense(Sense::click()))
        .on_hover_text(tip)
        .clicked()
    {
        *on = !*on;
    }
}

/// A row of a list to choose from: the label, the list as wide as the row
/// allows, `tip` on the label.
fn combo_row(
    ui: &mut Ui,
    label: &str,
    id: &str,
    selected: &str,
    tip: Option<&str>,
    items: impl FnOnce(&mut Ui),
) {
    ui.horizontal(|ui| {
        row_label(ui, label, tip);
        egui::ComboBox::from_id_salt(id)
            .selected_text(selected)
            .width(ui.available_width())
            .show_ui(ui, items);
    });
}

/// A small note under a row.
fn note(ui: &mut Ui, text: &str) {
    ui.label(RichText::new(text).size(12.0).color(style::TEXT_WEAK));
}

/// A group's title, which folds the group (open at first; the state in
/// egui memory under a global id, so that the tabs laid out unseen to
/// measure the panel fold alike), with `note` (a value) at the right end of
/// its line. True while the group is open.
fn section(ui: &mut Ui, key: &str, text: &str, note: &str) -> bool {
    let id = egui::Id::new(("panel_section", key));
    let open = ui.ctx().data_mut(|d| *d.get_persisted_mut_or(id, true));
    ui.add_space(10.0);
    let height = 16.0;
    let (rect, response) =
        ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::click());
    let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
    let open = if response.clicked() {
        ui.ctx().data_mut(|d| d.insert_persisted(id, !open));
        !open
    } else {
        open
    };
    let color = if response.hovered() {
        style::TEXT
    } else {
        style::TEXT_WEAK
    };
    let painter = ui.painter();
    // The arrow: right when folded, down when open.
    let centre = pos2(rect.left() + 4.0, rect.center().y);
    let points = if open {
        [vec2(-3.5, -1.75), vec2(0.0, 1.75), vec2(3.5, -1.75)]
    } else {
        [vec2(-1.75, -3.5), vec2(1.75, 0.0), vec2(-1.75, 3.5)]
    }
    .map(|p| centre + p)
    .to_vec();
    painter.add(egui::Shape::line(points, Stroke::new(1.5, color)));
    let font = egui::TextStyle::Small.resolve(ui.style());
    painter.text(
        pos2(rect.left() + 14.0, rect.center().y),
        Align2::LEFT_CENTER,
        text,
        font.clone(),
        color,
    );
    painter.text(
        pos2(rect.right(), rect.center().y),
        Align2::RIGHT_CENTER,
        note,
        font,
        style::TEXT_WEAK,
    );
    ui.add_space(2.0);
    open
}

/// Sample text drawn large as the subtitles look, on a ground from dark to
/// light as a picture under them: the look, not the size, which is the
/// same whatever is chosen. An approximation: egui's font, the outline
/// drawn round the letters, the box of the outline's colour a little
/// see-through.
fn preview(ui: &mut Ui, style: &SubStyle) {
    let (rect, response) = ui.allocate_exact_size(vec2(WIDTH, 72.0), Sense::hover());
    response.on_hover_text(tr!(
        "The look of the subtitles, roughly: the font is the interface's, the size fixed.",
        "Вид субтитров, примерно: шрифт интерфейса, размер постоянный."
    ));
    let painter = ui.painter_at(rect);
    let (dark, light) = (
        Color32::from_rgb(22, 26, 32),
        Color32::from_rgb(150, 158, 168),
    );
    let mut ground = egui::Mesh::default();
    for (corner, color) in [
        (rect.left_top(), dark),
        (rect.right_top(), light),
        (rect.right_bottom(), light),
        (rect.left_bottom(), dark),
    ] {
        ground.colored_vertex(corner, color);
    }
    ground.add_triangle(0, 1, 2);
    ground.add_triangle(0, 2, 3);
    painter.add(ground);

    let rgb = |[r, g, b]: [u8; 3]| Color32::from_rgb(r, g, b);
    let size = 32.0;
    let text = tr!("Sample subtitles", "Пример субтитров");
    let galley =
        painter.layout_no_wrap(text.to_owned(), FontId::proportional(size), Color32::WHITE);
    let at = rect.center() - galley.size() / 2.0;
    // Points of outline per unit of mpv's.
    let unit = size / 18.0 * 0.8;
    let outline = style.outline as f32 * unit;
    let draw = |offset: Vec2, color: Color32| {
        painter.galley_with_override_text_color(at + offset, galley.clone(), color);
    };
    let ring = |radius: f32| -> Vec<Vec2> {
        if radius <= 0.0 {
            return Vec::new();
        }
        (0..16)
            .map(|i| {
                let a = i as f32 * std::f32::consts::TAU / 16.0;
                vec2(a.cos(), a.sin()) * radius
            })
            .collect()
    };
    if style.boxed {
        let [r, g, b] = style.outline_color;
        let fill = Color32::from_rgba_unmultiplied(r, g, b, 0xC0);
        let text_rect = Rect::from_min_size(at, galley.size());
        painter.rect_filled(text_rect.expand(outline.max(2.0)), 2.0, fill);
    } else {
        if style.shadow > 0.0 {
            let shadow = Color32::from_black_alpha(200);
            let offset = Vec2::splat(style.shadow as f32 * unit);
            for o in ring(outline) {
                draw(offset + o, shadow);
            }
            draw(offset, shadow);
        }
        for radius in [outline, outline / 2.0] {
            for o in ring(radius) {
                draw(o, rgb(style.outline_color));
            }
        }
    }
    let letters = rgb(style.color);
    draw(Vec2::ZERO, letters);
    if style.bold {
        draw(vec2(0.7, 0.0), letters);
    }
}

/// A colour row: the swatch that opens a picker, the reset at the end.
fn color_row(ui: &mut Ui, label: &str, tip: &str, color: &mut [u8; 3], default: [u8; 3]) {
    labelled_tip(ui, label, Some(tip), |ui| {
        egui::widgets::color_picker::color_edit_button_srgb(ui, color);
        ui.add_space((ui.available_width() - RESET).max(0.0));
        if reset(ui, *color != default, tr!("Default", "По умолчанию")) {
            *color = default;
        }
    });
}

/// The width of [`reset`].
const RESET: f32 = 26.0;

/// The round arrow beside a slider that puts it back to its default
/// (named by `tip`); greyed while it is there. True when clicked.
fn reset(ui: &mut Ui, enabled: bool, tip: &str) -> bool {
    let enabled = enabled && ui.is_enabled();
    super::tool(ui, tip, enabled, |p, o, c| icons::reset(p, o, 6.0, c)).clicked()
}

/// A button's icon (`icons.rs`): the painter, the centre, the half width,
/// the colour.
pub(super) type Icon = fn(&egui::Painter, egui::Pos2, f32, egui::Color32);

/// The side of a button's icon.
const ICON: f32 = 16.0;

/// A button with `icon` before its text (or alone, `text` empty), its content centred in `size`
/// when given; `setup` adds to the button (selected, a shortcut). The icon
/// takes the text's colour, faded with it when the button is disabled.
pub(super) fn icon_button(
    ui: &mut Ui,
    icon: Icon,
    text: &str,
    size: Option<egui::Vec2>,
    setup: impl FnOnce(egui::Button<'_>) -> egui::Button<'_>,
) -> egui::Response {
    let id = ui.next_auto_id().with("icon");
    let icon_atom = egui::Atom::custom(id, vec2(ICON, ICON));
    let button = setup(if text.is_empty() {
        egui::Button::new(icon_atom)
    } else {
        egui::Button::new((icon_atom, text))
    });
    // The content is aligned by the layout it is put in.
    let shown = match size {
        Some(size) => {
            let centred = egui::Layout::left_to_right(egui::Align::Center)
                .with_main_align(egui::Align::Center);
            ui.allocate_ui_with_layout(size, centred, |ui| button.min_size(size).atom_ui(ui))
                .inner
        }
        None => button.atom_ui(ui),
    };
    if let Some(rect) = shown.rect(id) {
        let color = ui.style().interact(&shown.response).text_color();
        icon(ui.painter(), rect.center(), ICON * 0.4, color);
    }
    shown.response
}

/// Reset all, with the round arrow; whether it was clicked.
fn reset_all(ui: &mut Ui, enabled: bool) -> bool {
    ui.add_enabled_ui(enabled, |ui| {
        let text = tr!("Reset all", "Сбросить всё");
        icon_button(ui, icons::reset, text, None, |b| b)
    })
    .inner
    .clicked()
}

/// A row: the label in a fixed-width column, then what `add` adds.
fn labelled(ui: &mut Ui, label: &str, add: impl FnOnce(&mut Ui)) {
    labelled_tip(ui, label, None, add);
}

/// [`labelled`] with `tip` on the label.
fn labelled_tip(ui: &mut Ui, label: &str, tip: Option<&str>, add: impl FnOnce(&mut Ui)) {
    ui.horizontal(|ui| {
        row_label(ui, label, tip);
        add(ui);
    });
}

/// A row's label: at the left of a column [`LABEL`] wide, `tip` on it.
fn row_label(ui: &mut Ui, label: &str, tip: Option<&str>) {
    let size = vec2(LABEL, ui.spacing().interact_size.y);
    let left = Layout::left_to_right(Align::Center);
    ui.allocate_ui_with_layout(size, left, |ui| {
        ui.set_min_size(size);
        let response = ui.add(egui::Label::new(label).truncate());
        if let Some(tip) = tip {
            response.on_hover_text(tip);
        }
    });
}
