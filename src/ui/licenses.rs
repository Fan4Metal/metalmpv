//! The Licenses window (from About): the program's own licence, libmpv's
//! (the GPL v3, under which the program as distributed with it is), and
//! the notices of the Rust crates and fonts compiled into the exe. The
//! texts are embedded, so the exe carries them wherever it goes; the
//! installer and the portable archive ship them as files too.

use egui::{Color32, FontId, Key, Rect, RichText, Sense, pos2, vec2};

use super::style;

const METALMPV: &str = include_str!("../../LICENSE");
const THIRD_PARTY: &str = include_str!("../../THIRD_PARTY_LICENSES.md");
const GPL: &str = include_str!("../../licenses/GPL-3.0.txt");
/// Written by tools/notices.py.
pub const NOTICES: &str = include_str!("../../licenses/THIRD_PARTY_NOTICES.txt");

/// The window's content size in points.
const SIZE: egui::Vec2 = vec2(860.0, 540.0);
/// The width of the list on the left.
const LIST: f32 = 200.0;
const ROW: f32 = 15.0;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Part {
    #[default]
    Program,
    Libmpv,
    Crates,
}

impl Part {
    const ALL: [Part; 3] = [Part::Program, Part::Libmpv, Part::Crates];

    fn title(self) -> &'static str {
        match self {
            Part::Program => tr!("MetalMPV: MIT", "MetalMPV: MIT"),
            Part::Libmpv => tr!("libmpv: GNU GPL v3", "libmpv: GNU GPL v3"),
            Part::Crates => tr!("Libraries and fonts", "Библиотеки и шрифты"),
        }
    }

    fn texts(self) -> &'static [&'static str] {
        match self {
            Part::Program => &[METALMPV],
            Part::Libmpv => &[THIRD_PARTY, GPL],
            Part::Crates => &[NOTICES],
        }
    }
}

#[derive(Default)]
pub struct LicensesView {
    part: Part,
    /// The lines of the part shown, split once.
    lines: Vec<&'static str>,
}

impl LicensesView {
    /// Draw the window; whether it is to close (the cross, Esc, a click
    /// outside).
    pub fn show(&mut self, ctx: &egui::Context) -> bool {
        if self.lines.is_empty() {
            self.split();
        }
        let modal = egui::Modal::new(egui::Id::new("licenses")).show(ctx, |ui| {
            let (rect, _) = ui.allocate_exact_size(SIZE, Sense::hover());
            let list = Rect::from_min_size(rect.min, vec2(LIST, rect.height()));
            let mut list_ui = ui.new_child(egui::UiBuilder::new().max_rect(list));
            list_ui.label(
                RichText::new(tr!("Licenses", "Лицензии"))
                    .size(18.0)
                    .strong()
                    .color(style::TEXT_BRIGHT),
            );
            list_ui.add_space(8.0);
            for part in Part::ALL {
                if list_ui
                    .selectable_label(self.part == part, part.title())
                    .clicked()
                    && self.part != part
                {
                    self.part = part;
                    self.split();
                }
            }
            list_ui.add_space(12.0);
            list_ui.label(
                RichText::new(tr!(
                    "MetalMPV is distributed together with libmpv under the GNU GPL v3, without any warranty. Its own code is under the MIT License.",
                    "MetalMPV распространяется вместе с libmpv на условиях GNU GPL v3, без каких-либо гарантий. Его собственный код — под лицензией MIT."
                ))
                .size(12.0)
                .color(style::TEXT_WEAK),
            );

            let text = Rect::from_min_max(pos2(list.right() + 12.0, rect.top() + 28.0), rect.max);
            let cross = Rect::from_min_size(pos2(rect.right() - 24.0, rect.top()), vec2(24.0, 24.0));
            let closed = super::close_cross(&mut ui.new_child(egui::UiBuilder::new().max_rect(cross)));
            ui.painter().rect_filled(text, 4.0, Color32::from_black_alpha(60));
            let mut text_ui = ui.new_child(egui::UiBuilder::new().max_rect(text.shrink(6.0)));
            egui::ScrollArea::both()
                .id_salt(("licenses_text", self.part as u8))
                .auto_shrink([false, false])
                .show_rows(&mut text_ui, ROW, self.lines.len(), |ui, rows| {
                    for line in &self.lines[rows] {
                        ui.add(
                            egui::Label::new(
                                RichText::new(*line)
                                    .font(FontId::monospace(12.0))
                                    .color(style::TEXT),
                            )
                            .extend(),
                        );
                    }
                });
            closed
        });
        modal.inner || modal.should_close() || ctx.input(|i| i.key_pressed(Key::Escape))
    }

    fn split(&mut self) {
        self.lines.clear();
        for (i, text) in self.part.texts().iter().enumerate() {
            if i > 0 {
                self.lines.extend(["", ""]);
            }
            self.lines
                .extend(text.lines().map(|l| l.trim_end_matches('\r')));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// tools/notices.py's hash of Cargo.lock, without carriage returns.
    fn fnv1a64(data: &[u8]) -> String {
        let mut h: u64 = 0xCBF2_9CE4_8422_2325;
        for &b in data.iter().filter(|&&b| b != b'\r') {
            h ^= u64::from(b);
            h = h.wrapping_mul(0x0000_0100_0000_01B3);
        }
        format!("{h:016x}")
    }

    #[test]
    fn notices_follow_cargo_lock() {
        let lock = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.lock")).unwrap();
        let hash = fnv1a64(&lock);
        assert!(
            NOTICES.contains(&format!("Cargo.lock FNV-1a 64: {hash}")),
            "licenses/THIRD_PARTY_NOTICES.txt is older than Cargo.lock: run python tools/notices.py"
        );
    }

    #[test]
    fn texts_are_there() {
        assert!(METALMPV.contains("MIT License"));
        assert!(GPL.contains("GNU GENERAL PUBLIC LICENSE"));
        assert!(NOTICES.contains("egui") && NOTICES.contains("OFL"));
    }
}
