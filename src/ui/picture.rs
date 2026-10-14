//! The Video, sound and subtitles panel (`U` or `F7`, the sliders button
//! of the control bar): a window over its button, as the tracks window is,
//! that a press outside it closes; not an egui popup, so that playback and
//! the keys go on while it is open (egui's popups take the keys). Its head
//! is the three tabs, a note on the tab (ⓘ) and Save the frame (the
//! camera; its menu opens the folder).
//!
//! Video: the picture's adjustments (`adjust.rs`): the colour, the zoom and
//! the pan, the sharpness, the noise reduction and the debanding, the
//! aspect ratio, the deinterlacing, HDR on an ordinary screen, the turns
//! and flips; Reset all, and whether they are kept between runs. Sound
//! (`audio.rs`): the output device, the channels, the delay, the boost,
//! the normalization, clearer speech, and the ten-band equalizer with its
//! presets and the user's own. Subtitles (`sub_style.rs`): the looks,
//! ready-made and the user's own, a preview, the font and the size (also
//! on the settings' page), the colours, outline, shadow and box, the
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

use super::{icons, style};
use crate::adjust::{self, Adjust, Aspect, Color, Deinterlace, ToneMap};
use crate::audio::{self, Channels, EQ_BANDS, EQ_RANGE, Equalizer, OwnEq, Preset, band_label};
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
/// The room between the panel and its button.
const GAP: f32 = 6.0;
/// The panel's corner from the window's bottom right one when there is no
/// button (only the picture shown).
const CORNER: f32 = 12.0;
/// The delays' sliders reach this far either way; the keys go further.
const DELAY_RANGE: f64 = 5.0;
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

    /// What the tab's ⓘ says.
    fn note(self) -> &'static str {
        match self {
            Tab::Video => tr!(
                "The adjustments apply to every file until the program closes, or between runs with Remember. The sharpness, the noise reduction and the flips run on the processor (hardware decoding then copies each frame back for them); the rest on the graphics card.",
                "Настройки действуют для всех файлов до закрытия программы, а с «Запоминать» — и между запусками. Резкость, шумоподавление и отражения выполняет процессор (при аппаратном декодировании кадры для них копируются обратно в память), остальное — видеокарта."
            ),
            Tab::Sound => tr!(
                "The device, the channels and the boost are those of the settings' Sound page. The equalizer, the normalization and clearer speech do not apply to sound sent to a receiver undecoded. The delay is not kept between runs; the rest is.",
                "Устройство, каналы и усиление — те же, что на странице «Звук» настроек. Эквалайзер, нормализация и усиление речи не действуют на звук, передаваемый на ресивер без декодирования. Задержка между запусками не запоминается, остальное — запоминается."
            ),
            Tab::Subtitles => tr!(
                "The size and the height apply to all subtitles; the font and the colours to text ones (SRT and the like), and to ASS ones with Style for ASS too. The delay is not kept between runs; the rest is.",
                "Размер и высота действуют на все субтитры; шрифт и цвета — на текстовые (SRT и подобные), а с «Стиль и для ASS» — и на ASS. Задержка между запусками не запоминается, остальное — запоминается."
            ),
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
}

/// What the Sound and Subtitles tabs edit: settings, kept between runs.
pub struct Kept<'a> {
    /// The output device (`settings.audio_device`; empty: Windows'
    /// default) and the devices there are.
    pub audio_device: &'a mut String,
    pub devices: &'a [audio::Device],
    pub channels: &'a mut Channels,
    pub volume_max: &'a mut f64,
    pub normalize: &'a mut bool,
    pub voice_boost: &'a mut bool,
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
}

/// What the panel shows of the playback now.
#[derive(Clone, Copy, Debug)]
pub struct Now {
    /// A frame can be saved.
    pub has_video: bool,
    /// The sound's and the subtitles' delays, in seconds.
    pub audio_delay: f64,
    pub sub_delay: f64,
}

impl VideoPanel {
    /// A name is being typed: the player's keys wait.
    pub fn typing(&self) -> bool {
        self.open && self.naming.is_some()
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
    /// button's, else at the window's bottom right corner; shown whole,
    /// kept on the window when the room above is short. A press outside it
    /// and its button closes it, unless a popup of its own is open (a
    /// list, the colour picker, the camera's menu): the press is that one's.
    pub fn show(
        &mut self,
        ctx: &egui::Context,
        button: Option<egui::Rect>,
        adjust: &mut Adjust,
        kept: Kept<'_>,
        now: Now,
    ) -> Vec<Cmd> {
        let mut out = Vec::new();
        if !self.open {
            self.rect = None;
            self.naming = None;
            return out;
        }
        let pressed = ctx.input(|i| {
            i.pointer
                .any_pressed()
                .then(|| i.pointer.press_origin())
                .flatten()
        });
        if let Some(p) = pressed
            && !egui::Popup::is_any_open(ctx)
            && !self.rect.is_some_and(|r| r.contains(p))
            && !button.is_some_and(|b| b.contains(p))
        {
            self.open = false;
            self.rect = None;
            self.naming = None;
            return out;
        }
        let screen = ctx.content_rect();
        let frame = egui::Frame::popup(&ctx.style_of(egui::Theme::Dark))
            .fill(style::PANEL_BG)
            .inner_margin(12);
        // The left edge from the width the panel is meant to have, not the
        // one measured last frame: content a little wider moved the panel,
        // and it shook as it was laid out again.
        let outer = WIDTH + frame.total_margin().sum().x;
        let right_bottom = match button {
            Some(b) => b.right_top() - vec2(0.0, GAP),
            None => screen.right_bottom() - vec2(CORNER, CORNER),
        };
        let at = right_bottom - vec2(outer, 0.0);
        let shown = egui::Area::new(egui::Id::new("video_panel"))
            .order(egui::Order::Foreground)
            .pivot(Align2::LEFT_BOTTOM)
            .fixed_pos(at)
            .constrain_to(screen)
            .show(ctx, |ui| {
                frame.show(ui, |ui| {
                    ui.set_width(WIDTH);
                    self.head(ui, now.has_video, &mut out);
                    ui.add_space(4.0);
                    match self.tab {
                        Tab::Video => video_tab(ui, adjust, kept.keep_adjust),
                        Tab::Sound => sound_tab(ui, kept, now, &mut self.naming, &mut out),
                        Tab::Subtitles => subtitles_tab(ui, kept, now, &mut self.naming, &mut out),
                    }
                });
            });
        self.rect = Some(shown.response.rect);
        out
    }

    /// The tabs, then at the right the tab's note (ⓘ) and the camera:
    /// Save the frame, its menu opening the folder.
    fn head(&mut self, ui: &mut Ui, has_video: bool, out: &mut Vec<Cmd>) {
        ui.horizontal(|ui| {
            let before = self.tab;
            tabs(ui, &mut self.tab, WIDTH - 2.0 * RESET - 16.0);
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
                    if icon_button(ui, icons::folder, text, None, |b| b.frame(false)).clicked() {
                        out.push(Cmd::ShowFrames);
                        ui.close();
                    }
                });
                super::tool(ui, self.tab.note(), true, |p, o, c| icons::info(p, o, 6.5, c));
            });
        });
    }
}

/// The tabs as one control across `width`: equal parts over a line, the
/// one shown bright and underlined in the accent colour.
fn tabs(ui: &mut Ui, current: &mut Tab, width: f32) {
    let height = 30.0;
    let (rect, _) = ui.allocate_exact_size(vec2(width, height), Sense::hover());
    let painter = ui.painter().clone();
    painter.hline(
        rect.x_range(),
        rect.bottom() - 0.5,
        Stroke::new(1.0, style::LINE),
    );
    let part = width / Tab::ALL.len() as f32;
    for (i, tab) in Tab::ALL.into_iter().enumerate() {
        let r = Rect::from_min_size(
            pos2(rect.left() + part * i as f32, rect.top()),
            vec2(part, height),
        );
        let response = ui.interact(r, ui.id().with(("panel_tab", i)), Sense::click());
        if response.clicked() {
            *current = tab;
        }
        let on = *current == tab;
        if response.hovered() && !on {
            painter.rect_filled(r.shrink2(vec2(2.0, 3.0)), 4.0, style::HOVER);
        }
        let color = if on {
            style::TEXT_BRIGHT
        } else if response.hovered() {
            style::TEXT
        } else {
            style::TEXT_WEAK
        };
        painter.text(
            r.center() - vec2(0.0, 1.0),
            Align2::CENTER_CENTER,
            tab.label(),
            FontId::proportional(14.0),
            color,
        );
        if on {
            let half = part * 0.3;
            let y = r.bottom() - 1.0;
            painter.line_segment(
                [pos2(r.center().x - half, y), pos2(r.center().x + half, y)],
                Stroke::new(2.0, style::accent()),
            );
        }
    }
}

fn video_tab(ui: &mut Ui, adjust: &mut Adjust, keep: &mut bool) {
    heading(ui, tr!("Colour", "Цвет"));
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

    heading(ui, tr!("Zoom and pan", "Масштаб и сдвиг"));
    let zoom = Spec {
        step: 5.0,
        suffix: "%",
        ..Spec::centred(range_of(adjust::ZOOM_RANGE), 100.0)
    };
    int_row(ui, tr!("Zoom", "Масштаб"), &mut adjust.zoom, &zoom);
    let pan = |tip| Spec {
        suffix: "%",
        tip: Some(tip),
        ..Spec::centred(range_of(adjust::PAN_RANGE), 0.0)
    };
    int_row(
        ui,
        tr!("Across", "По горизонтали"),
        &mut adjust.pan_x,
        &pan(tr!("Right when positive", "Положительный — вправо")),
    );
    int_row(
        ui,
        tr!("Up and down", "По вертикали"),
        &mut adjust.pan_y,
        &pan(tr!("Down when positive", "Положительный — вниз")),
    );

    heading(ui, tr!("Enhancement", "Улучшение"));
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
            "AMD's contrast adaptive sharpening (CAS)",
            "Адаптивная резкость AMD (CAS)"
        )),
    );
    int_row(
        ui,
        tr!("Noise reduction", "Шумоподавление"),
        &mut adjust.denoise,
        &strength(tr!(
            "Smooths out grain and compression noise (hqdn3d); 40% is FFmpeg's usual strength, more blurs",
            "Сглаживает зерно и шум сжатия (hqdn3d); 40 % — обычная сила FFmpeg, больше — размывает"
        )),
    );
    switch_line(
        ui,
        &mut adjust.deband,
        tr!("Smooth gradient banding", "Сглаживание полос градиента"),
        Some(tr!(
            "Removes the steps in skies and dark scenes",
            "Убирает ступеньки на небе и в тёмных сценах"
        )),
    );

    heading(ui, tr!("Picture", "Изображение"));
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

    heading_with(
        ui,
        tr!("Rotation and flips", "Поворот и отражение"),
        &format!("{}°", adjust.rotate),
    );
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
            .on_hover_text(tr!("Upside down", "Перевернуть вертикально"))
            .clicked()
        {
            adjust.flip = !adjust.flip;
        }
        if icon_button(ui, mirror, "", size, |b| b.selected(mirrored))
            .on_hover_text(tr!("Mirror", "Отразить зеркально"))
            .clicked()
        {
            adjust.mirror = !adjust.mirror;
        }
    });

    ui.add_space(10.0);
    ui.horizontal(|ui| {
        if reset_all(ui, !adjust.is_default()) {
            *adjust = Adjust::default();
        }
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            switch_line_rtl(
                ui,
                keep,
                tr!("Remember", "Запоминать"),
                tr!(
                    "Keep the adjustments between runs of the program",
                    "Сохранять настройки изображения между запусками программы"
                ),
            );
        });
    });
}

fn sound_tab(
    ui: &mut Ui,
    kept: Kept<'_>,
    now: Now,
    naming: &mut Option<String>,
    out: &mut Vec<Cmd>,
) {
    heading(ui, tr!("Output", "Вывод"));
    labelled(ui, tr!("Device", "Устройство"), |ui| {
        let width = ui.available_width();
        super::settings::device_choice(
            ui,
            "panel_audio_device",
            kept.audio_device,
            kept.devices,
            width,
        );
    });
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
            "Auto: as the device is set up in Windows, more channels mixed down to it",
            "«Авто» — как устройство настроено в Windows, лишние каналы сводятся под него"
        )),
        |ui| {
            for c in Channels::ALL {
                ui.selectable_value(kept.channels, c, c.label());
            }
        },
    );
    let mut delay = now.audio_delay;
    if value_row(
        ui,
        tr!("Delay", "Задержка"),
        &mut delay,
        &Spec::delay(tr!(
            "Later when positive. Also Ctrl+= and Ctrl+-",
            "Положительная — звук позже. Также Ctrl+= и Ctrl+-"
        )),
    ) {
        out.push(Cmd::SetAudioDelay(delay));
    }

    heading(ui, tr!("Volume", "Громкость"));
    combo_row(
        ui,
        tr!("Boost", "Усиление"),
        "panel_volume_max",
        &audio::boost_label(*kept.volume_max),
        Some(tr!(
            "How far the volume goes past 100%",
            "Насколько громкость может превышать 100 %"
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
            "Evens out the volume as the film plays: quiet dialogue louder, loud scenes quieter",
            "Выравнивает громкость по ходу фильма: тихие диалоги громче, громкие сцены тише"
        )),
    );
    switch_line(
        ui,
        kept.voice_boost,
        tr!("Clearer speech", "Усилить речь"),
        Some(tr!(
            "Raises the voice over the music: in stereo through FFmpeg's dialogue enhancer, in 5.1 and more through the centre channel; mono stays as it is",
            "Поднимает голос над музыкой: в стерео — фильтром FFmpeg для диалогов, в 5.1 и больше — центральным каналом; моно остаётся как есть"
        )),
    );

    heading(ui, tr!("Equalizer", "Эквалайзер"));
    eq_head(ui, kept.equalizer, kept.eq_presets, naming);
    bands(ui, kept.equalizer);
}

/// The equalizer's switch, its presets and the user's own in one list, and
/// beside it saving the gains as one's own and deleting one's own; the
/// name is typed in a row under it.
fn eq_head(ui: &mut Ui, eq: &mut Equalizer, own: &mut Vec<OwnEq>, naming: &mut Option<String>) {
    let mine = own.iter().position(|o| o.gains == eq.gains);
    let preset = Preset::of(&eq.gains);
    ui.horizontal(|ui| {
        super::settings::switch(ui, &mut eq.on).on_hover_text(tr!("On", "Включён"));
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
    kept: Kept<'_>,
    now: Now,
    naming: &mut Option<String>,
    out: &mut Vec<Cmd>,
) {
    let style = kept.subs;
    let d = SubStyle::default();
    heading(ui, tr!("Style", "Стиль"));
    looks(ui, style, kept.sub_presets, naming);
    ui.add_space(4.0);
    preview(ui, style);

    heading(ui, tr!("Text", "Текст"));
    labelled(ui, tr!("Font", "Шрифт"), |ui| {
        let width = ui.available_width() - RESET - ui.spacing().item_spacing.x;
        if let Some(family) = super::settings::font_choice(ui, "panel_sub_font", &style.font, width)
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
        ..Spec::centred(sub_style::SCALE_RANGE, d.scale)
    };
    value_row(ui, tr!("Size", "Размер"), &mut style.scale, &size);
    switch_line(ui, &mut style.bold, tr!("Bold", "Жирные"), None);

    heading(ui, tr!("Colour", "Цвет"));
    color_row(ui, tr!("Letters", "Буквы"), &mut style.color, d.color);
    // With the box, the outline's colour and thickness are the box's
    // colour and margin.
    let (outline, thickness) = if style.boxed {
        (tr!("Box", "Подложка"), tr!("Margin", "Поля"))
    } else {
        (tr!("Outline", "Обводка"), tr!("Thickness", "Толщина"))
    };
    color_row(ui, outline, &mut style.outline_color, d.outline_color);
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

    heading(ui, tr!("Place and timing", "Положение и время"));
    let height = Spec {
        tip: Some(tr!("100 is the bottom", "100 — внизу")),
        ..Spec::centred(sub_style::POS_RANGE, d.pos)
    };
    value_row(ui, tr!("Height", "Высота"), &mut style.pos, &height);
    let mut delay = now.sub_delay;
    if value_row(
        ui,
        tr!("Delay", "Задержка"),
        &mut delay,
        &Spec::delay(tr!(
            "Later when positive. Also Shift+. and Shift+,",
            "Положительная — субтитры позже. Также Shift+. и Shift+,"
        )),
    ) {
        out.push(Cmd::SetSubDelay(delay));
    }
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
    labelled(ui, label, |ui| {
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
        let v = ((lo + t * (hi - lo)) / spec.step).round() * spec.step;
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
        let mut response = ui.add_sized(
            vec2(LABEL, ui.spacing().interact_size.y),
            egui::Label::new(label),
        );
        if let Some(tip) = tip {
            response = response.on_hover_text(tip);
        }
        let _ = response;
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

/// A group's title: small and muted, as the tracks' kinds in the
/// information window, with room above it.
fn heading(ui: &mut Ui, text: &str) {
    heading_with(ui, text, "");
}

/// A group's title with `note` (a value) at the right end of its line.
fn heading_with(ui: &mut Ui, text: &str, note: &str) {
    ui.add_space(10.0);
    ui.horizontal(|ui| {
        ui.label(RichText::new(text).small().strong().color(style::TEXT_WEAK));
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.label(RichText::new(note).small().color(style::TEXT_WEAK));
        });
    });
    ui.add_space(2.0);
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
fn color_row(ui: &mut Ui, label: &str, color: &mut [u8; 3], default: [u8; 3]) {
    labelled(ui, label, |ui| {
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
type Icon = fn(&egui::Painter, egui::Pos2, f32, egui::Color32);

/// The side of a button's icon.
const ICON: f32 = 16.0;

/// A button with `icon` before its text (or alone, `text` empty), its content centred in `size`
/// when given; `setup` adds to the button (selected, a shortcut). The icon
/// takes the text's colour, faded with it when the button is disabled.
fn icon_button(
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
    ui.horizontal(|ui| {
        ui.add_sized(
            vec2(LABEL, ui.spacing().interact_size.y),
            egui::Label::new(label),
        );
        add(ui);
    });
}
