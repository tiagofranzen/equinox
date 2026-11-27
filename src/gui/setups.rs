//! Side bar of saved setups: each one is a whole output sound (profile,
//! headphone correction, strength, preamp, balance). Pick one to switch to it;
//! a dot marks unsaved changes, saved with "Update".

use eframe::egui::{self, RichText, Sense, Stroke, Ui, vec2};

use super::App;
use super::theme::*;

impl App {
    pub fn setups_panel(&mut self, ui: &mut Ui) {
        let accent = self.accent;
        ui.add_space(14.0);
        ui.horizontal(|ui| {
            ui.add_space(14.0);
            caption(ui, "Setups");
        });
        ui.add_space(8.0);

        let mut pick = None;
        let mut remove = None;
        let mut start_rename = None;
        let active = self.st.active_setup;
        egui::ScrollArea::vertical().auto_shrink([false, false]).max_height(ui.available_height() - 64.0).show(ui, |ui| {
            for x in &self.st.setups {
                let selected = active == Some(x.id);
                if self.setup_rename.as_ref().is_some_and(|(id, _)| *id == x.id) {
                    continue;
                }
                let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width() - 6.0, 34.0), Sense::click());
                let painter = ui.painter();
                if selected {
                    painter.rect(rect, 8.0, accent.gamma_multiply(0.14), Stroke::new(1.0, accent.gamma_multiply(0.8)), egui::StrokeKind::Inside);
                } else if resp.hovered() {
                    painter.rect(rect, 8.0, RAISED, Stroke::new(1.0, FAINT), egui::StrokeKind::Inside);
                }
                let modified = selected && x.chain != self.st.global;
                painter.text(
                    rect.left_center() + vec2(12.0, 0.0),
                    egui::Align2::LEFT_CENTER,
                    if modified { format!("{} \u{25CF}", x.name) } else { x.name.clone() },
                    egui::FontId::proportional(13.5),
                    if selected { WHITE } else { TEXT },
                );
                if resp.clicked() {
                    pick = Some(x.id);
                }
                resp.context_menu(|ui| {
                    if ui.button("Rename").clicked() {
                        start_rename = Some((x.id, x.name.clone()));
                        ui.close();
                    }
                    if ui.button("Remove").clicked() {
                        remove = Some(x.id);
                        ui.close();
                    }
                });
                resp.on_hover_text("Click to switch. Right-click to rename or remove.");
            }
            // The one being renamed shows a text box in place.
            if let Some((id, buf)) = &mut self.setup_rename {
                let id = *id;
                let r = ui.add(egui::TextEdit::singleline(buf).desired_width(ui.available_width() - 12.0));
                r.request_focus();
                let (enter, esc) = ui.input(|i| (i.key_pressed(egui::Key::Enter), i.key_pressed(egui::Key::Escape)));
                if enter || r.lost_focus() {
                    let new = buf.trim().to_string();
                    if !new.is_empty()
                        && !esc
                        && let Some(x) = self.st.setups.iter_mut().find(|x| x.id == id)
                    {
                        x.name = new;
                    }
                    self.setup_rename = None;
                } else if esc {
                    self.setup_rename = None;
                }
            }
            if self.st.setups.is_empty() {
                ui.label(RichText::new("Save your current sound as a setup, for example one per pair of headphones or your speakers.").size(12.0).color(DIM));
            }
        });

        ui.add_space(6.0);
        let dirty = active
            .and_then(|id| self.st.setups.iter().find(|x| x.id == id))
            .filter(|x| x.chain != self.st.global)
            .map(|x| (x.id, x.name.clone()));
        if let Some((id, name)) = dirty {
            ui.horizontal(|ui| {
                ui.add_space(8.0);
                let r = ghost(ui, &format!("Update \"{name}\""))
                    .on_hover_text("Overwrite this setup with the current sound. Clicking a setup instead discards unsaved changes.");
                if r.clicked()
                    && let Some(x) = self.st.setups.iter_mut().find(|x| x.id == id)
                {
                    x.chain = self.st.global.clone();
                }
            });
            ui.add_space(4.0);
        }
        ui.horizontal(|ui| {
            ui.add_space(8.0);
            if primary(ui, accent, "+ Save current as setup").clicked() {
                let n = self.st.setups.len() + 1;
                let id = self.st.add_setup(&format!("Setup {n}"));
                let name = self.st.setups.iter().find(|x| x.id == id).map(|x| x.name.clone()).unwrap_or_default();
                self.setup_rename = Some((id, name));
            }
        });

        if let Some(id) = pick {
            self.st.apply_setup(id);
            self.sel_band = None;
        }
        if let Some(r) = start_rename {
            self.setup_rename = Some(r);
        }
        if let Some(id) = remove {
            self.st.delete_setup(id);
        }
    }
}
