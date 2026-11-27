//! Background images: one for the whole app, and optionally one per list.
//!
//! * Built-in backgrounds are drawn by RustDo itself (soft gradients with a gentle pattern),
//!   so there's nothing to license and they cost almost nothing in download size.
//! * Your own image is shrunk to fit 1920×1920 and kept as a JPEG copy (moving or deleting
//!   the original doesn't matter): in `Documents/RustDo/Backgrounds` on the desktop, the
//!   app's storage on Android, and browser storage in the web app.
//! * The choices sync with Sync my devices (they're settings), and so do your own images
//!   (account.rs sends them as `image` items), so a picture picked on the PC shows on the
//!   phone too.

use crate::i18n::tr;
use base64::Engine as _;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
#[cfg(not(target_arch = "wasm32"))]
use std::path::PathBuf;

/// What's behind the tasks.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(tag = "kind", content = "id")]
pub enum Background {
    /// The plain look.
    #[default]
    None,
    /// One of [`BUILTINS`], by id.
    Builtin(String),
    /// Your own image, by its id (the name of its stored copy).
    Custom(String),
}

/// A built-in background: colours from top-left to bottom-right, and a pattern.
pub struct Builtin {
    pub id: &'static str,
    pub stops: [[u8; 3]; 3],
    pub pattern: Pattern,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Pattern {
    None,
    Dots,
    Waves,
    Diagonal,
}

pub const BUILTINS: [Builtin; 8] = [
    Builtin { id: "dusk", stops: [[0x2b, 0x1e, 0x4a], [0x8a, 0x3f, 0x6b], [0xf0, 0x8a, 0x5d]], pattern: Pattern::None },
    Builtin { id: "ocean", stops: [[0x0b, 0x3d, 0x66], [0x1f, 0x7a, 0x9c], [0x7f, 0xd1, 0xd8]], pattern: Pattern::Waves },
    Builtin { id: "forest", stops: [[0x14, 0x3b, 0x2a], [0x2f, 0x6b, 0x45], [0x9b, 0xc4, 0x8a]], pattern: Pattern::Diagonal },
    Builtin { id: "sand", stops: [[0xf3, 0xe3, 0xc3], [0xe0, 0xc0, 0x8f], [0xc0, 0x8f, 0x5f]], pattern: Pattern::Dots },
    Builtin { id: "aurora", stops: [[0x0f, 0x24, 0x3d], [0x1f, 0x8f, 0x7a], [0x8f, 0x5f, 0xd0]], pattern: Pattern::Waves },
    Builtin { id: "rose", stops: [[0xfb, 0xe3, 0xe8], [0xf2, 0xa7, 0xbb], [0xc9, 0x6d, 0x8f]], pattern: Pattern::Dots },
    Builtin { id: "slate", stops: [[0x23, 0x2a, 0x34], [0x3d, 0x4a, 0x5c], [0x6f, 0x80, 0x96]], pattern: Pattern::Diagonal },
    Builtin { id: "meadow", stops: [[0xe6, 0xf4, 0xd4], [0xa8, 0xd8, 0x8f], [0x5e, 0xa8, 0x6b]], pattern: Pattern::None },
];

/// A built-in background's name in the current language.
pub fn builtin_name(id: &str) -> &'static str {
    match id {
        "dusk" => tr("Dusk"),
        "ocean" => tr("Ocean"),
        "forest" => tr("Forest"),
        "sand" => tr("Sand"),
        "aurora" => tr("Aurora"),
        "rose" => tr("Rose"),
        "slate" => tr("Slate"),
        _ => tr("Meadow"),
    }
}

pub fn builtin(id: &str) -> Option<&'static Builtin> {
    BUILTINS.iter().find(|b| b.id == id)
}

/// Draws a built-in background as `w`×`h` RGBA pixels.
pub fn render_builtin(b: &Builtin, w: usize, h: usize) -> Vec<u8> {
    let mut px = Vec::with_capacity(w * h * 4);
    let lerp = |a: u8, b: u8, t: f32| (a as f32 + (b as f32 - a as f32) * t).round().clamp(0.0, 255.0) as u8;
    for y in 0..h {
        for x in 0..w {
            let (fx, fy) = (x as f32 / (w - 1).max(1) as f32, y as f32 / (h - 1).max(1) as f32);
            // Diagonal position 0..1, top-left to bottom-right.
            let t = ((fx + fy) / 2.0).clamp(0.0, 1.0);
            let (c0, c1, tt) = if t < 0.5 { (b.stops[0], b.stops[1], t * 2.0) } else { (b.stops[1], b.stops[2], (t - 0.5) * 2.0) };
            let mut c = [lerp(c0[0], c1[0], tt), lerp(c0[1], c1[1], tt), lerp(c0[2], c1[2], tt)];
            // A gentle pattern: lighten a little where it is.
            let light = match b.pattern {
                Pattern::None => 0.0,
                Pattern::Dots => {
                    let (cx, cy) = ((x % 24) as f32 - 12.0, (y % 24) as f32 - 12.0);
                    if cx * cx + cy * cy < 5.0 { 0.10 } else { 0.0 }
                }
                Pattern::Waves => {
                    let wave = ((fx * 9.0 + (fy * 14.0).sin() * 0.6) * std::f32::consts::TAU).sin();
                    if wave > 0.92 { 0.07 } else { 0.0 }
                }
                Pattern::Diagonal => {
                    if (x + y) % 28 < 2 { 0.06 } else { 0.0 }
                }
            };
            if light > 0.0 {
                for v in &mut c {
                    *v = lerp(*v, 255, light);
                }
            }
            px.extend_from_slice(&[c[0], c[1], c[2], 255]);
        }
    }
    px
}

/// Largest side of a stored image.
const MAX_SIDE: u32 = 1920;
const JPEG_QUALITY: u8 = 82;

/// Reads a picture (JPG or PNG), shrinks it to fit [`MAX_SIDE`] and returns it as JPEG.
pub fn prepare(bytes: &[u8]) -> Result<Vec<u8>, String> {
    let img = image::load_from_memory(bytes).map_err(|_| tr("That isn't a JPG or PNG picture RustDo can read.").to_string())?;
    let img = if img.width() > MAX_SIDE || img.height() > MAX_SIDE { img.thumbnail(MAX_SIDE, MAX_SIDE) } else { img };
    let rgb = img.to_rgb8();
    let mut out = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, JPEG_QUALITY)
        .encode_image(&rgb)
        .map_err(|e| e.to_string())?;
    Ok(out)
}

/// A stored image's pixels, for drawing: (width, height, RGBA).
pub fn decode(jpeg: &[u8]) -> Option<(usize, usize, Vec<u8>)> {
    let img = image::load_from_memory(jpeg).ok()?.to_rgba8();
    Some((img.width() as usize, img.height() as usize, img.into_raw()))
}

/// A new image's id: its content's fingerprint, so the same picture is stored once.
pub fn image_id(jpeg: &[u8]) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for b in jpeg {
        hash ^= *b as u64;
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    format!("img-{hash:016x}")
}

pub fn to_base64(bytes: &[u8]) -> String {
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

pub fn from_base64(text: &str) -> Option<Vec<u8>> {
    base64::engine::general_purpose::STANDARD.decode(text).ok()
}

/// Every custom image a setting refers to.
pub fn used_images(settings: &crate::model::Settings) -> Vec<String> {
    let mut ids: Vec<String> = std::iter::once(&settings.background)
        .chain(settings.list_backgrounds.values())
        .filter_map(|b| match b {
            Background::Custom(id) => Some(id.clone()),
            _ => None,
        })
        .collect();
    ids.sort();
    ids.dedup();
    ids
}

// ------------------------------------------------------------------ storage

/// Your own images on this device: id → JPEG bytes.
pub type Images = BTreeMap<String, Vec<u8>>;

/// Where images are kept on the desktop and Android (`<data folder>/Backgrounds`).
#[cfg(not(target_arch = "wasm32"))]
fn folder(data: &std::path::Path) -> PathBuf {
    data.join("Backgrounds")
}

/// Loads the stored images.
pub fn load(data: Option<&std::path::Path>) -> Images {
    let mut out = Images::new();
    #[cfg(target_arch = "wasm32")]
    {
        let _ = data;
        if let Some(ls) = web_sys::window().and_then(|w| w.local_storage().ok().flatten()) {
            let len = ls.length().unwrap_or(0);
            for i in 0..len {
                let Some(key) = ls.key(i).ok().flatten() else { continue };
                let Some(id) = key.strip_prefix(WEB_PREFIX) else { continue };
                if let Some(bytes) = ls.get_item(&key).ok().flatten().and_then(|t| from_base64(&t)) {
                    out.insert(id.to_string(), bytes);
                }
            }
        }
    }
    #[cfg(not(target_arch = "wasm32"))]
    if let Some(data) = data {
        if let Ok(entries) = std::fs::read_dir(folder(data)) {
            for e in entries.flatten() {
                let path = e.path();
                let Some(id) = path.file_stem().and_then(|s| s.to_str()).filter(|s| s.starts_with("img-")) else { continue };
                if path.extension().is_some_and(|x| x == "jpg") {
                    if let Ok(bytes) = std::fs::read(&path) {
                        out.insert(id.to_string(), bytes);
                    }
                }
            }
        }
    }
    out
}

#[cfg(target_arch = "wasm32")]
const WEB_PREFIX: &str = "rustdo.bg.";

/// The ids of the stored images (cheap: no pictures are read).
pub fn list_ids(data: Option<&std::path::Path>) -> Vec<String> {
    let mut ids = Vec::new();
    #[cfg(target_arch = "wasm32")]
    {
        let _ = data;
        if let Some(ls) = web_sys::window().and_then(|w| w.local_storage().ok().flatten()) {
            for i in 0..ls.length().unwrap_or(0) {
                if let Some(id) = ls.key(i).ok().flatten().and_then(|k| k.strip_prefix(WEB_PREFIX).map(str::to_string)) {
                    ids.push(id);
                }
            }
        }
    }
    #[cfg(not(target_arch = "wasm32"))]
    if let Some(data) = data {
        if let Ok(entries) = std::fs::read_dir(folder(data)) {
            for e in entries.flatten() {
                let path = e.path();
                if path.extension().is_some_and(|x| x == "jpg") {
                    if let Some(id) = path.file_stem().and_then(|s| s.to_str()).filter(|s| s.starts_with("img-")) {
                        ids.push(id.to_string());
                    }
                }
            }
        }
    }
    ids.sort();
    ids
}

/// One stored image.
pub fn load_one(data: Option<&std::path::Path>, id: &str) -> Option<Vec<u8>> {
    #[cfg(target_arch = "wasm32")]
    {
        let _ = data;
        let ls = web_sys::window()?.local_storage().ok()??;
        from_base64(&ls.get_item(&format!("{WEB_PREFIX}{id}")).ok()??)
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        std::fs::read(folder(data?).join(format!("{id}.jpg"))).ok()
    }
}

/// Stores an image.
pub fn save(data: Option<&std::path::Path>, id: &str, jpeg: &[u8]) -> Result<(), String> {
    #[cfg(target_arch = "wasm32")]
    {
        let _ = data;
        let ls = web_sys::window().and_then(|w| w.local_storage().ok().flatten()).ok_or("browser storage is unavailable")?;
        ls.set_item(&format!("{WEB_PREFIX}{id}"), &to_base64(jpeg))
            .map_err(|_| tr("The browser's storage is full: remove a background picture and try again.").to_string())
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let Some(data) = data else { return Ok(()) };
        let dir = folder(data);
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        std::fs::write(dir.join(format!("{id}.jpg")), jpeg).map_err(|e| e.to_string())
    }
}

/// Deletes a stored image (no longer used by any setting).
pub fn delete(data: Option<&std::path::Path>, id: &str) {
    #[cfg(target_arch = "wasm32")]
    {
        let _ = data;
        if let Some(ls) = web_sys::window().and_then(|w| w.local_storage().ok().flatten()) {
            let _ = ls.remove_item(&format!("{WEB_PREFIX}{id}"));
        }
    }
    #[cfg(not(target_arch = "wasm32"))]
    if let Some(data) = data {
        let _ = std::fs::remove_file(folder(data).join(format!("{id}.jpg")));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtins_render() {
        for b in &BUILTINS {
            let px = render_builtin(b, 32, 20);
            assert_eq!(px.len(), 32 * 20 * 4);
            // Top-left is the first colour, bottom-right the last (a pattern may lighten
            // a pixel a little).
            let near = |p: &[u8], c: &[u8; 3]| p.iter().zip(c).all(|(a, b)| (*a as i32 - *b as i32).abs() <= 30);
            assert!(near(&px[0..3], &b.stops[0]), "{} top-left", b.id);
            assert!(near(&px[px.len() - 4..px.len() - 1], &b.stops[2]), "{} bottom-right", b.id);
        }
    }

    #[test]
    fn pictures_are_shrunk_and_stored_as_jpeg() {
        let big = image::RgbImage::from_fn(3000, 1500, |x, y| image::Rgb([(x % 256) as u8, (y % 256) as u8, 120]));
        let mut png = Vec::new();
        image::DynamicImage::ImageRgb8(big).write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png).unwrap();
        let jpeg = prepare(&png).unwrap();
        assert_eq!(&jpeg[0..2], &[0xFF, 0xD8], "JPEG");
        let (w, h, px) = decode(&jpeg).unwrap();
        assert_eq!((w, h), (1920, 960));
        assert_eq!(px.len(), w * h * 4);
        assert!(prepare(b"not a picture").is_err());
        assert_eq!(image_id(&jpeg), image_id(&jpeg.clone()));
        assert_eq!(from_base64(&to_base64(&jpeg)).unwrap(), jpeg);
    }

    #[test]
    fn stored_and_loaded() {
        let dir = std::env::temp_dir().join(format!("rustdo-bg-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        save(Some(&dir), "img-0000000000000001", b"jpegbytes").unwrap();
        let all = load(Some(&dir));
        assert_eq!(all.get("img-0000000000000001").map(Vec::as_slice), Some(&b"jpegbytes"[..]));
        delete(Some(&dir), "img-0000000000000001");
        assert!(load(Some(&dir)).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
