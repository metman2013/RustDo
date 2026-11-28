//! Android: choosing one of your photos with Android's own photo picker.
//!
//! RustDo's activity (android/app/.../MainActivity.java) opens the system picker, which
//! needs no permission (Google Play doesn't allow photo permissions for this), shrinks the
//! chosen photo to a JPEG and keeps it for RustDo to collect.

use crate::app::PicFor;
use crate::i18n::tr;
use crate::platform::Picked;
use eframe::egui::{self, RichText};

pub struct PhotoPicker {
    /// What the chosen photo is for.
    pub target: PicFor,
    started: bool,
}

pub enum Outcome {
    Open,
    Cancelled,
    /// The chosen photo as a JPEG.
    Picked(Vec<u8>),
    Failed(String),
}

impl PhotoPicker {
    pub fn new(target: PicFor) -> Self {
        Self { target, started: false }
    }

    pub fn ui(&mut self, ui: &mut egui::Ui) -> Outcome {
        if !self.started {
            self.started = true;
            if !crate::platform::android_pick_photo() {
                return Outcome::Failed(tr("Couldn't open Android's photo picker.").to_string());
            }
        }
        match crate::platform::android_picked_photo() {
            Picked::Waiting => {}
            Picked::Cancelled => return Outcome::Cancelled,
            Picked::Photo(bytes) => return Outcome::Picked(bytes),
            Picked::Failed => return Outcome::Failed(tr("Couldn't read that photo.").to_string()),
        }
        ui.label(tr("Choose a picture in Android's photo picker."));
        ui.label(RichText::new(tr("Only the picture you choose is used.")).weak());
        ui.add_space(6.0);
        let cancel = ui.button(tr("Cancel")).clicked();
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(300));
        if cancel {
            Outcome::Cancelled
        } else {
            Outcome::Open
        }
    }
}
