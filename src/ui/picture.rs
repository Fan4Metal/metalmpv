//! The Video, sound and subtitles panel (`U` or `F7`, the sliders button of the control
//! bar): a window over its button, as the tracks window is, that a press
//! outside it closes; not an egui popup, so that playback and the keys go
//! on while it is open (egui's popups take the keys), with three tabs. Video: the picture's adjustments (`adjust.rs`):
//! the colour's sliders (a double click or the round arrow beside one puts
//! it back to 0), the sharpness, the noise reduction and the debanding,
//! the aspect ratio, the rotation and the flips, saving the frame, and
//! Reset all. Sound: the ten-band equalizer with its presets and the
//! normalization (`audio.rs`). Subtitles: how they look (`sub_style.rs`;
//! the font and the size also on the settings' page).

use egui::{RichText, Ui, vec2};

use super::{icons, style};
use crate::adjust::{Adjust, Aspect, Color};
use crate::audio::{EQ_BANDS, EQ_RANGE, Equalizer, Preset, band_label};
use crate::input::Cmd;
use crate::sub_style::{self, SubStyle};

/// The width of the window's content (egui's `min_width`/`max_width` are
/// the window's, with its frame: content wider than that pushed the frame
/// out past the title bar).
const WIDTH: f32 = 400.0;
/// The width of the labels' column.
const LABEL: f32 = 110.0;
/// The width of a slider's value.
const VALUE: f32 = 44.0;
/// The room between the panel and its button.
const GAP: f32 = 6.0;
/// The panel's corner from the window's bottom right one when there is no
/// button (only the picture shown).
const CORNER: f32 = 12.0;

/// The panel's tab.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Tab {
    #[default]
    Video,
    Sound,
    Subtitles,
}

#[derive(Default)]
pub struct VideoPanel {
    pub open: bool,
    tab: Tab,
    /// Where the panel was drawn last, for the presses outside it.
    rect: Option<egui::Rect>,
}

/// What the Sound and Subtitles tabs edit: settings, kept between runs.
pub struct Kept<'a> {
    /// The output device (`settings.audio_device`; empty: Windows'
    /// default) and the devices there are.
    pub audio_device: &'a mut String,
    pub devices: &'a [crate::audio::Device],
    pub equalizer: &'a mut Equalizer,
    pub normalize: &'a mut bool,
    pub subs: &'a mut SubStyle,
}

impl VideoPanel {
    pub fn toggle(&mut self) {
        self.open = !self.open;
    }

    /// Open the panel on `tab`.
    pub fn open_on(&mut self, tab: Tab) {
        self.open = true;
        self.tab = tab;
    }

    /// Draw the panel while it is open, editing `adjust` and `kept` in
    /// place; the commands of its buttons. It stands over `button` (the
    /// control bar's, where it was drawn this frame), its right edge on the
    /// button's, else at the window's bottom right corner; shown whole,
    /// kept on the window when the room above is short. A press outside it and its button closes
    /// it, unless a popup of its own is open (a list, the colour picker):
    /// the press is that one's. `has_video`: a frame can be saved.
    pub fn show(
        &mut self,
        ctx: &egui::Context,
        button: Option<egui::Rect>,
        adjust: &mut Adjust,
        kept: Kept<'_>,
        has_video: bool,
    ) -> Vec<Cmd> {
        let mut out = Vec::new();
        if !self.open {
            self.rect = None;
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
            return out;
        }
        let screen = ctx.content_rect();
        let at = match button {
            Some(b) => b.right_top() - vec2(0.0, GAP),
            None => screen.right_bottom() - vec2(CORNER, CORNER),
        };
        let frame = egui::Frame::popup(&ctx.style_of(egui::Theme::Dark))
            .fill(style::PANEL_BG)
            .inner_margin(12);
        let shown = egui::Area::new(egui::Id::new("video_panel"))
            .order(egui::Order::Foreground)
            .pivot(egui::Align2::RIGHT_BOTTOM)
            .fixed_pos(at)
            .constrain_to(screen)
            .show(ctx, |ui| {
                frame.show(ui, |ui| {
                    ui.set_width(WIDTH);
                    ui.horizontal(|ui| {
                        ui.selectable_value(&mut self.tab, Tab::Video, tr!("Video", "Видео"));
                        ui.selectable_value(&mut self.tab, Tab::Sound, tr!("Sound", "Звук"));
                        ui.selectable_value(
                            &mut self.tab,
                            Tab::Subtitles,
                            tr!("Subtitles", "Субтитры"),
                        );
                    });
                    ui.separator();
                    match self.tab {
                        Tab::Video => video_tab(ui, adjust, has_video, &mut out),
                        Tab::Sound => sound_tab(ui, kept),
                        Tab::Subtitles => subtitles_tab(ui, kept.subs),
                    }
                });
            });
        self.rect = Some(shown.response.rect);
        out
    }
}

fn video_tab(ui: &mut Ui, adjust: &mut Adjust, has_video: bool, out: &mut Vec<Cmd>) {
    // The label, the slider, its value and the reset fill the row.
    let gaps = 3.0 * ui.spacing().item_spacing.x;
    ui.spacing_mut().slider_width = WIDTH - LABEL - VALUE - RESET - gaps;
    ui.spacing_mut().interact_size.x = VALUE;
    heading(ui, tr!("Colour", "Цвет"));
    for c in Color::ALL {
        let mut value = adjust.get(c);
        labelled(ui, c.label(), |ui| {
            let response = ui
                .add(egui::Slider::new(&mut value, -100..=100))
                .on_hover_text(tr!(
                    "A double click puts it back to 0",
                    "Двойной щелчок возвращает 0"
                ));
            if response.double_clicked() || reset(ui, value != 0, tr!("Back to 0", "Вернуть 0"))
            {
                value = 0;
            }
        });
        adjust.set(c, value);
    }

    heading(ui, tr!("Enhancement", "Улучшение"));
    labelled(ui, tr!("Sharpness", "Резкость"), |ui| {
        let response = ui
            .add(egui::Slider::new(&mut adjust.sharpen, 0..=100).suffix("%"))
            .on_hover_text(tr!(
                "AMD's contrast adaptive sharpening, on the processor; a double click turns it off",
                "Адаптивная резкость AMD (CAS), на процессоре; двойной щелчок выключает"
            ));
        let on = adjust.sharpen != 0;
        if response.double_clicked() || reset(ui, on, tr!("Off", "Выключить")) {
            adjust.sharpen = 0;
        }
    });
    ui.checkbox(
        &mut adjust.denoise,
        tr!("Noise reduction", "Шумоподавление"),
    )
    .on_hover_text(tr!(
        "Smooths out grain and compression noise (hqdn3d), on the processor",
        "Сглаживает зерно и шум сжатия (hqdn3d), на процессоре"
    ));
    ui.checkbox(
        &mut adjust.deband,
        tr!("Smooth gradient banding", "Сглаживание полос градиента"),
    )
    .on_hover_text(tr!(
        "Removes the steps in skies and dark scenes, on the graphics card",
        "Убирает ступеньки на небе и в тёмных сценах, на видеокарте"
    ));

    heading(ui, tr!("Aspect ratio", "Пропорции"));
    labelled(ui, tr!("Shape", "Форма кадра"), |ui| {
        egui::ComboBox::from_id_salt("video_aspect")
            .selected_text(adjust.aspect.label())
            .width(WIDTH - LABEL - 30.0)
            .show_ui(ui, |ui| {
                for a in Aspect::ALL {
                    ui.selectable_value(&mut adjust.aspect, a, a.label());
                }
            });
    });

    heading_with(
        ui,
        tr!("Rotation and flips", "Поворот и отражение"),
        &format!("{}°", adjust.rotate),
    );
    let [left, right] = pair(
        ui,
        |ui, size| {
            let turn = |p: &_, o, s, c| icons::turn(p, o, s, c, false);
            icon_button(ui, turn, tr!("Turn left", "Повернуть влево"), size, |b| b)
        },
        |ui, size| {
            let turn = |p: &_, o, s, c| icons::turn(p, o, s, c, true);
            icon_button(ui, turn, tr!("Turn right", "Повернуть вправо"), size, |b| b)
        },
    );
    if left.clicked() {
        adjust.turn(-1);
    }
    if right.clicked() {
        adjust.turn(1);
    }
    let (flipped, mirrored) = (adjust.flip, adjust.mirror);
    let [flip, mirror] = pair(
        ui,
        |ui, size| {
            let flip = |p: &_, o, s, c| icons::flip(p, o, s, c, false);
            let text = tr!("Upside down", "Перевернуть вертикально");
            icon_button(ui, flip, text, size, |b| b.selected(flipped))
        },
        |ui, size| {
            let mirror = |p: &_, o, s, c| icons::flip(p, o, s, c, true);
            let text = tr!("Mirror", "Отразить зеркально");
            icon_button(ui, mirror, text, size, |b| b.selected(mirrored))
        },
    );
    if flip.clicked() {
        adjust.flip = !adjust.flip;
    }
    if mirror.clicked() {
        adjust.mirror = !adjust.mirror;
    }

    heading(ui, tr!("Frame", "Кадр"));
    let [save, folder] = pair(
        ui,
        |ui, size| {
            ui.add_enabled_ui(has_video, |ui| {
                let text = tr!("Save the frame", "Сохранить кадр");
                icon_button(ui, icons::camera, text, size, |b| {
                    b.shortcut_text(crate::keymap::shortcut(Cmd::SaveFrame))
                })
            })
            .inner
        },
        |ui, size| {
            let text = tr!("Open the folder", "Открыть папку");
            icon_button(ui, icons::folder, text, size, |b| b)
        },
    );
    if save
        .on_hover_text(tr!(
            "Into Pictures\\MetalMPV, as decoded, without subtitles",
            "В «Изображения\\MetalMPV», как декодирован, без субтитров"
        ))
        .clicked()
    {
        out.push(Cmd::SaveFrame);
    }
    if folder.clicked() {
        out.push(Cmd::ShowFrames);
    }

    ui.add_space(10.0);
    if reset_all(ui, !adjust.is_default()) {
        *adjust = Adjust::default();
    }
    ui.add_space(2.0);
    ui.label(
        RichText::new(tr!(
            "The settings apply to every file until the program closes.",
            "Настройки действуют для всех файлов до закрытия программы."
        ))
        .size(12.0)
        .color(style::TEXT_WEAK),
    );
}

fn sound_tab(ui: &mut Ui, kept: Kept<'_>) {
    let (eq, normalize) = (kept.equalizer, kept.normalize);
    heading(ui, tr!("Output", "Вывод"));
    labelled(ui, tr!("Device", "Устройство"), |ui| {
        super::settings::device_choice(
            ui,
            "panel_audio_device",
            kept.audio_device,
            kept.devices,
            WIDTH - LABEL - 30.0,
        );
    });
    if !super::settings::device_present(kept.audio_device, kept.devices) {
        ui.label(
            RichText::new(tr!(
                "Not connected: the sound goes to Windows' default until it is back.",
                "Не подключено: звук идёт через устройство Windows по умолчанию, пока оно не вернётся."
            ))
            .size(12.0)
            .color(style::TEXT_WEAK),
        );
    }

    heading(ui, tr!("Equalizer", "Эквалайзер"));
    ui.horizontal(|ui| {
        ui.checkbox(&mut eq.on, tr!("On", "Включён"));
        ui.add_space(12.0);
        let preset = Preset::of(&eq.gains);
        let name = preset.map_or(tr!("Own", "Свой"), Preset::label);
        egui::ComboBox::from_id_salt("eq_preset")
            .selected_text(name)
            .width(150.0)
            .show_ui(ui, |ui| {
                for p in Preset::ALL {
                    if ui.selectable_label(preset == Some(p), p.label()).clicked() {
                        eq.gains = p.gains();
                        eq.on = true;
                    }
                }
            });
    });
    ui.add_space(6.0);
    ui.add_enabled_ui(eq.on, |ui| {
        ui.spacing_mut().slider_width = 130.0;
        ui.spacing_mut().item_spacing.x = 0.0;
        let column = WIDTH / EQ_BANDS.len() as f32;
        ui.horizontal(|ui| {
            for (i, hz) in EQ_BANDS.iter().enumerate() {
                ui.allocate_ui_with_layout(
                    vec2(column, 216.0),
                    egui::Layout::top_down(egui::Align::Center),
                    |ui| {
                        let gain = eq.gains[i];
                        let text = if gain > 0 {
                            format!("+{gain}")
                        } else {
                            gain.to_string()
                        };
                        ui.label(RichText::new(text).size(11.0).color(style::TEXT_WEAK));
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
                        ui.label(RichText::new(band_label(*hz)).size(11.0));
                        let on = eq.gains[i] != 0;
                        if response.double_clicked() || reset(ui, on, tr!("Back to 0", "Вернуть 0"))
                        {
                            eq.gains[i] = 0;
                        }
                    },
                );
            }
        });
    });
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        let flat = ui
            .add_enabled_ui(eq.gains != [0; 10], |ui| {
                icon_button(ui, icons::flat, tr!("Flat", "Выровнять"), None, |b| b)
            })
            .inner;
        if flat.clicked() {
            eq.gains = [0; 10];
        }
        ui.label(
            RichText::new(tr!("dB, kept between runs.", "дБ, запоминается."))
                .size(12.0)
                .color(style::TEXT_WEAK),
        );
    });

    heading(ui, tr!("Volume", "Громкость"));
    ui.checkbox(normalize, tr!("Normalization", "Нормализация"))
        .on_hover_text(tr!(
            "Evens out the volume as the film plays: quiet dialogue louder, loud scenes quieter.",
            "Выравнивает громкость по ходу фильма: тихие диалоги громче, громкие сцены тише."
        ));
    ui.add_space(4.0);
    ui.label(
        RichText::new(tr!(
            "Neither applies to sound sent to a receiver undecoded.",
            "Не действуют на звук, передаваемый на ресивер без декодирования."
        ))
        .size(12.0)
        .color(style::TEXT_WEAK),
    );
}

fn subtitles_tab(ui: &mut Ui, style: &mut SubStyle) {
    let d = SubStyle::default();
    let spacing = ui.spacing().item_spacing.x;
    ui.spacing_mut().slider_width = WIDTH - LABEL - VALUE - RESET - 3.0 * spacing;
    ui.spacing_mut().interact_size.x = VALUE;
    heading(ui, tr!("Style", "Стиль"));
    labelled(ui, tr!("Ready-made", "Заготовка"), |ui| {
        let preset = sub_style::Preset::of(style);
        let name = preset.map_or(tr!("Own", "Свой"), sub_style::Preset::label);
        egui::ComboBox::from_id_salt("sub_preset")
            .selected_text(name)
            .width(WIDTH - LABEL - 2.0 * spacing)
            .show_ui(ui, |ui| {
                for p in sub_style::Preset::ALL {
                    if ui.selectable_label(preset == Some(p), p.label()).clicked() {
                        p.apply(style);
                    }
                }
            });
    });

    heading(ui, tr!("Text", "Текст"));
    labelled(ui, tr!("Font", "Шрифт"), |ui| {
        let width = WIDTH - LABEL - RESET - 3.0 * spacing;
        if let Some(family) = super::settings::font_choice(ui, "panel_sub_font", &style.font, width)
        {
            style.font = family.map(|f| f.name.clone()).unwrap_or_default();
        }
        if reset(ui, style.font != d.font, tr!("Default", "По умолчанию")) {
            style.font.clear();
        }
    });
    slider_row(
        ui,
        tr!("Size", "Размер"),
        &mut style.scale,
        sub_style::SCALE_RANGE,
        (5.0, 0, " %"),
        d.scale,
    );
    labelled(ui, "", |ui| {
        ui.checkbox(&mut style.bold, tr!("Bold", "Жирные"));
    });

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
    slider_row(
        ui,
        thickness,
        &mut style.outline,
        sub_style::OUTLINE_RANGE,
        (0.05, 2, ""),
        d.outline,
    );
    slider_row(
        ui,
        tr!("Shadow", "Тень"),
        &mut style.shadow,
        sub_style::SHADOW_RANGE,
        (0.25, 2, ""),
        d.shadow,
    );
    labelled(ui, "", |ui| {
        ui.checkbox(
            &mut style.boxed,
            tr!("A box behind the text", "Подложка под текстом"),
        )
        .on_hover_text(tr!(
            "A dark, slightly see-through box behind each line in place of the outline, of the outline's colour.",
            "Тёмная полупрозрачная плашка под каждой строкой вместо обводки, цвета обводки."
        ));
    });

    heading(ui, tr!("Place", "Положение"));
    slider_row(
        ui,
        tr!("Height", "Высота"),
        &mut style.pos,
        sub_style::POS_RANGE,
        (1.0, 0, ""),
        d.pos,
    );

    ui.add_space(10.0);
    if reset_all(ui, *style != d) {
        *style = d;
    }
    ui.add_space(2.0);
    ui.label(
        RichText::new(tr!(
            "The size and the height apply to all subtitles; the font and the colours to text ones (SRT and the like), as ASS subtitles keep their own styles. Kept between runs.",
            "Размер и высота действуют на все субтитры; шрифт и цвета — на текстовые (SRT и подобные): у субтитров ASS свои стили. Запоминается."
        ))
        .size(12.0)
        .color(style::TEXT_WEAK),
    );
}

/// A slider row with its reset: `format` is the step, the decimals shown
/// and the suffix.
fn slider_row(
    ui: &mut Ui,
    label: &str,
    value: &mut f64,
    range: std::ops::RangeInclusive<f64>,
    format: (f64, usize, &str),
    default: f64,
) {
    let (step, decimals, suffix) = format;
    labelled(ui, label, |ui| {
        let response = ui
            .add(
                egui::Slider::new(value, range)
                    .step_by(step)
                    .fixed_decimals(decimals)
                    .suffix(suffix),
            )
            .on_hover_text(tr!(
                "A double click puts it back",
                "Двойной щелчок возвращает исходное"
            ));
        let changed = (*value - default).abs() > 1e-9;
        if response.double_clicked() || reset(ui, changed, tr!("Default", "По умолчанию"))
        {
            *value = default;
        }
    });
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

/// A group's title.
fn heading(ui: &mut Ui, text: &str) {
    heading_with(ui, text, "");
}

/// A group's title with `note` (a value) at the right end of its line.
fn heading_with(ui: &mut Ui, text: &str, note: &str) {
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        ui.label(RichText::new(text).strong().color(style::TEXT_BRIGHT));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(RichText::new(note).color(style::TEXT_WEAK));
        });
    });
    ui.add_space(2.0);
}

/// Two buttons side by side, each half the panel's width: `a` and `b` add
/// them at the size given.
fn pair(
    ui: &mut Ui,
    a: impl FnOnce(&mut Ui, Option<egui::Vec2>) -> egui::Response,
    b: impl FnOnce(&mut Ui, Option<egui::Vec2>) -> egui::Response,
) -> [egui::Response; 2] {
    ui.horizontal(|ui| {
        let gap = ui.spacing().item_spacing.x;
        let size = Some(vec2((WIDTH - gap) / 2.0, 26.0));
        let a = a(ui, size);
        [a, b(ui, size)]
    })
    .inner
}

/// A button's icon (`icons.rs`): the painter, the centre, the half width,
/// the colour.
type Icon = fn(&egui::Painter, egui::Pos2, f32, egui::Color32);

/// The side of a button's icon.
const ICON: f32 = 16.0;

/// A button with `icon` before its text, its content centred in `size`
/// when given; `setup` adds to the button (selected, a shortcut). The icon
/// takes the text's colour, faded with it when the button is disabled.
fn icon_button(
    ui: &mut Ui,
    icon: Icon,
    text: &str,
    size: Option<egui::Vec2>,
    setup: impl FnOnce(egui::Button<'_>) -> egui::Button<'_>,
) -> egui::Response {
    let id = ui.id().with(("icon", text));
    let button = setup(egui::Button::new((
        egui::Atom::custom(id, vec2(ICON, ICON)),
        text,
    )));
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
