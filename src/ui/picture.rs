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

use egui::{Color32, FontId, Rect, RichText, Sense, Ui, Vec2, vec2};

use super::{icons, style};
use crate::adjust::{Adjust, Aspect, Color};
use crate::audio::{DELAY_STEP, EQ_BANDS, EQ_RANGE, Equalizer, Preset, band_label};
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
    /// The name being typed for the subtitles' look to save.
    naming: Option<String>,
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
    /// The subtitles' looks the user saved.
    pub sub_presets: &'a mut Vec<sub_style::Own>,
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

/// The delays' sliders reach this far either way; the keys go further.
const DELAY_RANGE: f64 = 5.0;

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
        self.tab = tab;
    }

    /// Draw the panel while it is open, editing `adjust` and `kept` in
    /// place; the commands of its buttons. It stands over `button` (the
    /// control bar's, where it was drawn this frame), its right edge on the
    /// button's, else at the window's bottom right corner; shown whole,
    /// kept on the window when the room above is short. A press outside it and its button closes
    /// it, unless a popup of its own is open (a list, the colour picker):
    /// the press is that one's. A tab with something changed from its
    /// default has a dot.
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
                    let changed = [
                        !adjust.is_default(),
                        (kept.equalizer.on && kept.equalizer.gains != [0; 10])
                            || *kept.normalize
                            || now.audio_delay != 0.0,
                        *kept.subs != SubStyle::default() || now.sub_delay != 0.0,
                    ];
                    ui.horizontal(|ui| {
                        let tabs = [
                            (Tab::Video, tr!("Video", "Видео")),
                            (Tab::Sound, tr!("Sound", "Звук")),
                            (Tab::Subtitles, tr!("Subtitles", "Субтитры")),
                        ];
                        for ((tab, name), changed) in tabs.into_iter().zip(changed) {
                            tab_button(ui, &mut self.tab, tab, name, changed);
                        }
                    });
                    ui.separator();
                    match self.tab {
                        Tab::Video => video_tab(ui, adjust, now.has_video, &mut out),
                        Tab::Sound => sound_tab(ui, kept, now, &mut out),
                        Tab::Subtitles => subtitles_tab(
                            ui,
                            kept.subs,
                            kept.sub_presets,
                            &mut self.naming,
                            now,
                            &mut out,
                        ),
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

fn sound_tab(ui: &mut Ui, kept: Kept<'_>, now: Now, out: &mut Vec<Cmd>) {
    let (eq, normalize) = (kept.equalizer, kept.normalize);
    let spacing = ui.spacing().item_spacing.x;
    ui.spacing_mut().slider_width = WIDTH - LABEL - VALUE - RESET - 3.0 * spacing;
    ui.spacing_mut().interact_size.x = VALUE;
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

    if let Some(delay) = delay_row(
        ui,
        tr!("Delay", "Задержка"),
        now.audio_delay,
        tr!(
            "Later when positive. Also Ctrl+= and Ctrl+-; not kept between runs.",
            "Положительная — звук позже. Также Ctrl+= и Ctrl+-; не запоминается."
        ),
    ) {
        out.push(Cmd::SetAudioDelay(delay));
    }

    heading(ui, tr!("Equalizer", "Эквалайзер"));
    ui.horizontal(|ui| {
        ui.checkbox(&mut eq.on, tr!("On", "Включён"));
        ui.add_space(12.0);
        let preset = Preset::of(&eq.gains);
        let name = preset.map_or(tr!("Custom", "Другой"), Preset::label);
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

fn subtitles_tab(
    ui: &mut Ui,
    style: &mut SubStyle,
    own: &mut Vec<sub_style::Own>,
    naming: &mut Option<String>,
    now: Now,
    out: &mut Vec<Cmd>,
) {
    let d = SubStyle::default();
    let spacing = ui.spacing().item_spacing.x;
    ui.spacing_mut().slider_width = WIDTH - LABEL - VALUE - RESET - 3.0 * spacing;
    ui.spacing_mut().interact_size.x = VALUE;
    heading(ui, tr!("Style", "Стиль"));
    looks(ui, style, own, naming);
    ui.add_space(4.0);
    preview(ui, style);

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

    heading(ui, tr!("Place and timing", "Положение и время"));
    slider_row(
        ui,
        tr!("Height", "Высота"),
        &mut style.pos,
        sub_style::POS_RANGE,
        (1.0, 0, ""),
        d.pos,
    );
    if let Some(delay) = delay_row(
        ui,
        tr!("Delay", "Задержка"),
        now.sub_delay,
        tr!(
            "Later when positive. Also Shift+. and Shift+,; not kept between runs.",
            "Положительная — субтитры позже. Также Shift+. и Shift+,; не запоминается."
        ),
    ) {
        out.push(Cmd::SetSubDelay(delay));
    }

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

/// The ready-made looks and the user's own (the whole style, by name) in
/// one list; beside it, saving the look as one's own and, for one's own,
/// deleting it. Saving asks for a name in a row under the list (Enter
/// saves, Esc gives up; the player's keys wait meanwhile).
fn looks(
    ui: &mut Ui,
    style: &mut SubStyle,
    own: &mut Vec<sub_style::Own>,
    naming: &mut Option<String>,
) {
    let spacing = ui.spacing().item_spacing.x;
    let mine = own.iter().position(|o| o.style == *style);
    let preset = sub_style::Preset::of(style);
    labelled(ui, tr!("Ready-made", "Заготовка"), |ui| {
        let name = match (mine, preset) {
            (Some(i), _) => own[i].name.clone(),
            (None, Some(p)) => p.label().to_owned(),
            (None, None) => tr!("Custom", "Другой").to_owned(),
        };
        egui::ComboBox::from_id_salt("sub_preset")
            .selected_text(name)
            .width(WIDTH - LABEL - 2.0 * RESET - 4.0 * spacing)
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
        let save = super::tool(
            ui,
            tr!("Save as my own…", "Сохранить как свою…"),
            naming.is_none(),
            |p, o, c| icons::plus(p, o, 6.0, c),
        );
        if save.clicked() {
            *naming = Some(mine.map(|i| own[i].name.clone()).unwrap_or_default());
        }
        let delete = super::tool(
            ui,
            tr!("Delete this look of mine", "Удалить эту свою заготовку"),
            mine.is_some(),
            |p, o, c| icons::trash(p, o, 6.0, c),
        );
        if delete.clicked()
            && let Some(i) = mine
        {
            own.remove(i);
        }
    });
    let Some(name) = naming else {
        return;
    };
    let mut done = false;
    labelled(ui, tr!("Name", "Название"), |ui| {
        let field = ui.add(
            egui::TextEdit::singleline(name)
                .hint_text(tr!("My look", "Моя заготовка"))
                .desired_width(WIDTH - LABEL - 2.0 * RESET - 4.0 * spacing),
        );
        if ui.memory(|m| m.focused().is_none()) {
            field.request_focus();
        }
        let enter = field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        let ok = super::tool(
            ui,
            tr!("Save", "Сохранить"),
            !name.trim().is_empty(),
            |p, o, c| icons::check(p, o, 6.0, c),
        );
        if (enter || ok.clicked()) && !name.trim().is_empty() {
            sub_style::Own::save(own, name, style);
            done = true;
        }
        let cancel = super::tool(ui, tr!("Cancel", "Отмена"), true, |p, o, c| {
            icons::close(p, o, 5.0, c)
        });
        if cancel.clicked() || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            done = true;
        }
    });
    if own.len() >= sub_style::MAX_OWN && !own.iter().any(|o| o.name == name.trim()) {
        ui.label(
            RichText::new(tr!(
                "The list is full: the oldest look of mine gives way.",
                "Список полон: самая старая своя заготовка уступит место."
            ))
            .size(12.0)
            .color(style::TEXT_WEAK),
        );
    }
    if done {
        *naming = None;
    }
}

/// A tab's name, a dot in the accent colour after it while `changed`.
fn tab_button(ui: &mut Ui, current: &mut Tab, tab: Tab, name: &str, changed: bool) {
    let response = ui.selectable_label(*current == tab, name);
    if response.clicked() {
        *current = tab;
    }
    let (dot, _) = ui.allocate_exact_size(vec2(8.0, response.rect.height()), Sense::hover());
    if changed {
        ui.painter().circle_filled(
            egui::pos2(dot.left() + 2.0, dot.top() + 6.0),
            3.0,
            style::accent(),
        );
    }
}

/// A delay's row: a slider of seconds, a reset; the delay chosen, if
/// another. The slider does not clamp: a delay set further by the keys
/// shows as it is.
fn delay_row(ui: &mut Ui, label: &str, now: f64, tip: &str) -> Option<f64> {
    let mut value = now;
    let mut chosen = None;
    let unit = tr!(" s", " с");
    labelled(ui, label, |ui| {
        let response = ui
            .add(
                egui::Slider::new(&mut value, -DELAY_RANGE..=DELAY_RANGE)
                    .step_by(DELAY_STEP / 2.0)
                    .fixed_decimals(2)
                    .suffix(unit)
                    .clamping(egui::SliderClamping::Never),
            )
            .on_hover_text(tip);
        if response.changed() && (value - now).abs() > 1e-9 {
            chosen = Some(value);
        }
        if response.double_clicked() || reset(ui, now != 0.0, tr!("Back to 0", "Вернуть 0"))
        {
            chosen = Some(0.0);
        }
    });
    chosen
}

/// A line of sample text drawn as the subtitles look, on a ground from dark
/// to light as a picture under them. An approximation: egui's font, the
/// outline drawn round the letters, thicker than mpv's to be seen at this
/// size, the box of the outline's colour a little see-through.
fn preview(ui: &mut Ui, style: &SubStyle) {
    let (rect, response) = ui.allocate_exact_size(vec2(WIDTH, 60.0), Sense::hover());
    response.on_hover_text(tr!(
        "Roughly as on the picture: the size follows, the font is the interface's.",
        "Примерно как на изображении: размер учитывается, шрифт — интерфейса."
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
    let size = (18.0 * style.scale as f32 / 100.0).clamp(10.0, 32.0);
    let text = tr!("Subtitles look like this", "Так выглядят субтитры");
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
