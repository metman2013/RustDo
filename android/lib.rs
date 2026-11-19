//! The APK's native library. All of RustDo lives in the `rustdo_app` crate; linking it
//! here brings in its `android_main` entry point, which Android's NativeActivity calls.
pub use rustdo_app::*;
