//! The Video and sound panel (`F7`): a floating window over the player,
//! so that playback and the keys go on, with two tabs. Video: the
//! picture's adjustments (`adjust.rs`): the colour's sliders (a double
//! click puts one back to 0), the sharpness, the noise reduction and the
//! debanding, the aspect ratio, the rotation and the flips, saving the
//! frame, and Reset all. Sound: the ten-band equalizer with its presets
//! and the normalization (`audio.rs`).

use egui::{RichText, Ui, vec2};

use super::style;
use crate::adjust::{Adjust, Aspect, Color};
use crate::audio::{EQ_BANDS, EQ_RANGE, Equalizer, Preset, band_label};
use crate::input::Cmd;

/// The width of the window's content.
const WIDTH: f32 = 330.0;
/// The width of the labels' column.
const LABEL: f32 = 110.0;
/// The window's top left corner from the player's: under the title row.
const CORNER: egui::Vec2 = vec2(23.0, 46.0);

/// The panel's tab.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Tab {
    #[default]
    Video,
    Sound,
}

#[derive(Default)]
pub struct VideoPanel {
    pub open: bool,
    tab: Tab,
}

/// What the Sound tab edits: the settings' equalizer and normalization.
pub struct Sound<'a> {
    pub equalizer: &'a mut Equalizer,
    pub normalize: &'a mut bool,
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

    /// Draw the panel while it is open, editing `adjust` and `sound` in
    /// place; the commands of its buttons. `has_video`: a frame can be
    /// saved.
    pub fn show(
        &mut self,
        ctx: &egui::Context,
        adjust: &mut Adjust,
        sound: Sound<'_>,
        has_video: bool,
    ) -> Vec<Cmd> {
        let mut out = Vec::new();
        if !self.open {
            return out;
        }
        let screen = ctx.content_rect();
        let mut open = true;
        egui::Window::new(tr!("Video and sound", "Видео и звук"))
            .id(egui::Id::new("video_panel"))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_pos(screen.left_top() + CORNER)
            .min_width(WIDTH)
            .max_width(WIDTH)
            .constrain_to(screen)
            .frame(
                egui::Frame::window(&ctx.style_of(egui::Theme::Dark))
                    .fill(style::PANEL_BG)
                    .corner_radius(8)
                    .inner_margin(12),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut self.tab, Tab::Video, tr!("Video", "Видео"));
                    ui.selectable_value(&mut self.tab, Tab::Sound, tr!("Sound", "Звук"));
                });
                ui.separator();
                match self.tab {
                    Tab::Video => video_tab(ui, adjust, has_video, &mut out),
                    Tab::Sound => sound_tab(ui, sound),
                }
            });
        if !open {
            self.open = false;
        }
        out
    }
}

fn video_tab(ui: &mut Ui, adjust: &mut Adjust, has_video: bool, out: &mut Vec<Cmd>) {
    ui.spacing_mut().slider_width = WIDTH - LABEL - 60.0;
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
            if response.double_clicked() {
                value = 0;
            }
        });
        adjust.set(c, value);
    }

    heading(ui, tr!("Enhancement", "Улучшение"));
    labelled(ui, tr!("Sharpness", "Резкость"), |ui| {
        ui.add(egui::Slider::new(&mut adjust.sharpen, 0..=100).suffix("%"))
            .on_hover_text(tr!(
                "AMD's contrast adaptive sharpening, on the processor",
                "Адаптивная резкость AMD (CAS), на процессоре"
            ));
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

    heading(ui, tr!("Rotation and flips", "Поворот и отражение"));
    ui.horizontal(|ui| {
        if ui.button(tr!("Turn left", "Повернуть влево")).clicked() {
            adjust.turn(-1);
        }
        if ui.button(tr!("Turn right", "Повернуть вправо")).clicked() {
            adjust.turn(1);
        }
        ui.label(RichText::new(format!("{}°", adjust.rotate)).color(style::TEXT_WEAK));
    });
    ui.horizontal(|ui| {
        ui.toggle_value(
            &mut adjust.flip,
            tr!("Upside down", "Перевернуть вертикально"),
        );
        ui.toggle_value(&mut adjust.mirror, tr!("Mirror", "Отразить зеркально"));
    });

    heading(ui, tr!("Frame", "Кадр"));
    ui.horizontal(|ui| {
        if ui
            .add_enabled(
                has_video,
                egui::Button::new(tr!("Save the frame", "Сохранить кадр")).shortcut_text("Ctrl+E"),
            )
            .on_hover_text(tr!(
                "Into Pictures\\MetalMPV, as decoded, without subtitles",
                "В «Изображения\\MetalMPV», как декодирован, без субтитров"
            ))
            .clicked()
        {
            out.push(Cmd::SaveFrame);
        }
        if ui.button(tr!("Open the folder", "Открыть папку")).clicked() {
            out.push(Cmd::ShowFrames);
        }
    });

    ui.add_space(10.0);
    ui.horizontal(|ui| {
        if ui
            .add_enabled(
                !adjust.is_default(),
                egui::Button::new(tr!("Reset all", "Сбросить всё")),
            )
            .clicked()
        {
            *adjust = Adjust::default();
        }
        ui.label(
            RichText::new(tr!(
                "For every file, until the program closes.",
                "Для всех файлов, до закрытия программы."
            ))
            .size(12.0)
            .color(style::TEXT_WEAK),
        );
    });
}

fn sound_tab(ui: &mut Ui, sound: Sound<'_>) {
    let eq = sound.equalizer;
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
                    vec2(column, 190.0),
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
                        if response.double_clicked() {
                            eq.gains[i] = 0;
                        }
                        ui.label(RichText::new(band_label(*hz)).size(11.0));
                    },
                );
            }
        });
    });
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        if ui
            .add_enabled(
                eq.gains != [0; 10],
                egui::Button::new(tr!("Flat", "Сбросить")),
            )
            .clicked()
        {
            eq.gains = [0; 10];
        }
        ui.label(
            RichText::new(tr!("dB, kept between runs.", "дБ, запоминается."))
                .size(12.0)
                .color(style::TEXT_WEAK),
        );
    });

    heading(ui, tr!("Volume", "Громкость"));
    ui.checkbox(sound.normalize, tr!("Normalization", "Нормализация"))
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

/// A group's title.
fn heading(ui: &mut Ui, text: &str) {
    ui.add_space(6.0);
    ui.label(RichText::new(text).strong().color(style::TEXT_BRIGHT));
    ui.add_space(2.0);
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
