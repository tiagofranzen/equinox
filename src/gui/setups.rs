//! Side bar of saved setups: each one is a whole output sound (profile,
//! headphone correction, strength, preamp, balance). Pick one to switch to it;
//! a dot marks unsaved changes, saved with "Update". The toolbar uses the same
//! icons as the profile bar on the Equalizer page.

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
        ui.add_space(4.0);

        let active = self.st.active_setup.filter(|id| self.st.setups.iter().any(|x| x.id == *id));
        let active_name = active.and_then(|id| self.st.setups.iter().find(|x| x.id == id)).map(|x| x.name.clone());

        // Toolbar: + new, copy, rename, delete, then import / export.
        let mut act = None;
        ui.horizontal_wrapped(|ui| {
            ui.add_space(8.0);
            ui.spacing_mut().item_spacing.x = 6.0;
            let icon = |ui: &mut Ui, glyph: &str, tip: &str| flat(ui, RichText::new(glyph).size(15.0).color(DIM)).on_hover_text(tip).clicked();
            let text_btn = |ui: &mut Ui, label: &str, tip: &str| flat(ui, RichText::new(label).size(12.5).color(DIM)).on_hover_text(tip).clicked();
            if icon(ui, "+", "Save the current sound as a new setup") {
                act = Some("new");
            }
            if active.is_some() {
                if icon(ui, "⧉", "Duplicate this setup") {
                    act = Some("dup");
                }
                if icon(ui, "✎", "Rename") {
                    act = Some("rename");
                }
                if icon(ui, "🗑", "Delete setup (Ctrl+Z brings it back)") {
                    act = Some("delete");
                }
            }
            if text_btn(ui, "Import", "Import a setup file (.json) with its profile and headphone correction") {
                act = Some("import");
            }
            if active.is_some() && text_btn(ui, "Export", "Export this setup as a file you can share or back up") {
                act = Some("export");
            }
        });
        ui.add_space(6.0);

        let mut pick = None;
        let mut remove = None;
        let mut start_rename = None;
        egui::ScrollArea::vertical().auto_shrink([false, false]).max_height(ui.available_height() - 40.0).show(ui, |ui| {
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
                resp.on_hover_text("Click to switch. Right-click for more.");
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
                ui.label(
                    RichText::new("Save your current sound with +, for example one per pair of headphones or your speakers.")
                        .size(12.0)
                        .color(DIM),
                );
            }
        });

        // "Update" shows only while the active setup has unsaved changes.
        let dirty = active
            .and_then(|id| self.st.setups.iter().find(|x| x.id == id))
            .filter(|x| x.chain != self.st.global)
            .map(|x| (x.id, x.name.clone()));
        if let Some((id, name)) = dirty {
            ui.horizontal(|ui| {
                ui.add_space(8.0);
                let r = primary(ui, accent, &format!("Update \"{name}\""))
                    .on_hover_text("Overwrite this setup with the current sound. Clicking a setup instead discards unsaved changes.");
                if r.clicked()
                    && let Some(x) = self.st.setups.iter_mut().find(|x| x.id == id)
                {
                    x.chain = self.st.global.clone();
                }
            });
        }

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
        match act {
            Some("new") => {
                let n = self.st.setups.len() + 1;
                let id = self.st.add_setup(&format!("Setup {n}"));
                let name = self.st.setups.iter().find(|x| x.id == id).map(|x| x.name.clone()).unwrap_or_default();
                self.setup_rename = Some((id, name));
            }
            Some("dup") => {
                if let Some(id) = active
                    && let Some(nid) = self.st.duplicate_setup(id)
                {
                    let name = self.st.setups.iter().find(|x| x.id == nid).map(|x| x.name.clone()).unwrap_or_default();
                    self.setup_rename = Some((nid, name));
                    self.sel_band = None;
                }
            }
            Some("rename") => {
                if let (Some(id), Some(name)) = (active, active_name) {
                    self.setup_rename = Some((id, name));
                }
            }
            Some("delete") => {
                if let (Some(id), Some(name)) = (active, active_name) {
                    self.st.delete_setup(id);
                    self.notify(format!("Deleted {name}"), false);
                }
            }
            Some("import") => self.import_setup_file(),
            Some("export") => self.export_setup_file(),
            _ => {}
        }
    }

    fn import_setup_file(&mut self) {
        let Some(path) = rfd::FileDialog::new().add_filter("Equinox setup", &["json"]).add_filter("All files", &["*"]).pick_file() else {
            return;
        };
        match std::fs::read_to_string(&path) {
            Ok(text) => match self.st.import_setup(&text) {
                Ok(_) => {
                    self.sel_band = None;
                    self.notify("Setup imported", false);
                }
                Err(e) => self.notify(e, true),
            },
            Err(e) => self.notify(format!("couldn't read file: {e}"), true),
        }
    }

    fn export_setup_file(&mut self) {
        let Some(id) = self.st.active_setup else { return };
        let Some(name) = self.st.setups.iter().find(|x| x.id == id).map(|x| x.name.clone()) else {
            return;
        };
        let Some(json) = self.st.export_setup(id) else { return };
        let Some(path) = rfd::FileDialog::new()
            .set_file_name(format!("{name}.setup.json"))
            .add_filter("Equinox setup", &["json"])
            .save_file()
        else {
            return;
        };
        match std::fs::write(&path, json) {
            Ok(()) => self.notify(format!("Exported to {}", path.display()), false),
            Err(e) => self.notify(format!("couldn't write: {e}"), true),
        }
    }
}
