//! Android: choosing one of your photos inside RustDo. NativeActivity can't receive the
//! system photo picker's answer, so RustDo shows the phone's newest photos itself.

use crate::app::PicFor;
use crate::i18n::tr;
use eframe::egui::{self, Color32, CornerRadius, RichText, Sense, Stroke, Vec2};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// How many of the newest photos are offered.
const MAX_PHOTOS: usize = 120;
/// Thumbnail size (pixels) and the size a chosen photo is read at.
const THUMB: i32 = 256;
const FULL: i32 = 1920;

/// What the background thread has found so far.
#[derive(Default)]
struct Found {
    /// None until the list of photos is in.
    ids: Option<Vec<i64>>,
    /// Thumbnails as they arrive (None: that one couldn't be read).
    thumbs: Vec<(i64, Option<Vec<u8>>)>,
}

pub struct PhotoPicker {
    /// What the chosen photo is for.
    pub target: PicFor,
    asked: bool,
    last_check: f64,
    found: Option<Arc<Mutex<Found>>>,
    textures: HashMap<i64, Option<egui::TextureHandle>>,
}

pub enum Outcome {
    Open,
    Cancelled,
    /// The chosen photo's bytes (JPEG, or the file itself on older phones).
    Picked(Vec<u8>),
    Failed(String),
}

impl PhotoPicker {
    pub fn new(target: PicFor) -> Self {
        Self { target, asked: false, last_check: f64::NEG_INFINITY, found: None, textures: HashMap::new() }
    }

    pub fn ui(&mut self, ui: &mut egui::Ui) -> Outcome {
        let ctx = ui.ctx().clone();
        let mut outcome = Outcome::Open;

        if self.found.is_none() {
            // Not loading yet: first make sure RustDo may see the photos.
            let now = ctx.input(|i| i.time);
            if now - self.last_check > 0.5 {
                self.last_check = now;
                if crate::platform::android_photo_access() {
                    self.found = Some(start(&ctx));
                }
            }
            ctx.request_repaint_after(std::time::Duration::from_millis(500));
            if self.found.is_none() {
                ui.label(tr("RustDo needs your permission to show your photos here. Only the picture you choose is used; nothing is uploaded unless you sync with your own Hub."));
                ui.add_space(6.0);
                if ui.button(tr("Allow access to photos")).clicked() {
                    crate::platform::android_ask_photo_access();
                    self.asked = true;
                }
                if self.asked {
                    ui.add_space(4.0);
                    ui.label(RichText::new(tr("Nothing happened? Allow Photos for RustDo in Android Settings → Apps → RustDo → Permissions.")).weak());
                }
                ui.add_space(6.0);
                if ui.button(tr("Cancel")).clicked() {
                    outcome = Outcome::Cancelled;
                }
                return outcome;
            }
        }

        let (ids, thumbs) = {
            let found = self.found.as_ref().expect("loading");
            let mut f = found.lock().unwrap_or_else(|e| e.into_inner());
            (f.ids.clone(), std::mem::take(&mut f.thumbs))
        };
        for (id, jpeg) in thumbs {
            let texture = jpeg.as_deref().and_then(crate::backgrounds::decode).map(|(w, h, px)| {
                ctx.load_texture(format!("photo:{id}"), egui::ColorImage::from_rgba_unmultiplied([w, h], &px), egui::TextureOptions::LINEAR)
            });
            self.textures.insert(id, texture);
        }

        match ids {
            None => {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(tr("Looking for your photos…"));
                });
            }
            Some(ids) if ids.is_empty() => {
                ui.label(tr("No photos found on this phone."));
            }
            Some(ids) => {
                let columns = 3.0;
                let gap = 6.0;
                let side = ((ui.available_width() - gap * (columns - 1.0)) / columns).floor().max(60.0);
                let height = (ctx.screen_rect().height() * 0.6).max(200.0);
                let mut chosen = None;
                egui::ScrollArea::vertical().max_height(height).auto_shrink([false, true]).show(ui, |ui| {
                    ui.spacing_mut().item_spacing = Vec2::splat(gap);
                    ui.horizontal_wrapped(|ui| {
                        for id in &ids {
                            let (rect, r) = ui.allocate_exact_size(Vec2::splat(side), Sense::click());
                            if !ui.is_rect_visible(rect) {
                                continue;
                            }
                            let corner = CornerRadius::same(6);
                            match self.textures.get(id) {
                                Some(Some(t)) => {
                                    let uv = crate::app::cover_uv(t.size_vec2(), rect.size());
                                    ui.painter().add(egui::epaint::RectShape::filled(rect, corner, Color32::WHITE).with_texture(t.id(), uv));
                                }
                                _ => {
                                    ui.painter().rect_filled(rect, corner, ui.visuals().extreme_bg_color);
                                }
                            }
                            ui.painter().rect_stroke(rect, corner, Stroke::new(1.0_f32, ui.visuals().weak_text_color()), egui::StrokeKind::Inside);
                            if r.clicked() {
                                chosen = Some(*id);
                            }
                        }
                    });
                });
                if let Some(id) = chosen {
                    outcome = match crate::platform::android_photo(id, FULL) {
                        Some(bytes) => Outcome::Picked(bytes),
                        None => Outcome::Failed(tr("Couldn't read that photo.").to_string()),
                    };
                }
            }
        }
        ui.add_space(6.0);
        if ui.button(tr("Cancel")).clicked() {
            outcome = Outcome::Cancelled;
        }
        outcome
    }
}

/// Lists the newest photos and reads their thumbnails on a background thread. It stops
/// when the picker closes.
fn start(ctx: &egui::Context) -> Arc<Mutex<Found>> {
    let found = Arc::new(Mutex::new(Found::default()));
    let shared = found.clone();
    let ctx = ctx.clone();
    std::thread::spawn(move || {
        let ids = crate::platform::android_recent_photos(MAX_PHOTOS);
        shared.lock().unwrap_or_else(|e| e.into_inner()).ids = Some(ids.clone());
        ctx.request_repaint();
        for id in ids {
            if Arc::strong_count(&shared) == 1 {
                break;
            }
            let thumb = crate::platform::android_photo(id, THUMB);
            shared.lock().unwrap_or_else(|e| e.into_inner()).thumbs.push((id, thumb));
            ctx.request_repaint();
        }
    });
    found
}
