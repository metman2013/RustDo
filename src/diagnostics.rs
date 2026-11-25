//! Logs, crash reports and start-up safety nets (Windows and Linux).
//!
//! A desktop RustDo has no console, so anything that goes wrong before (or instead of) the
//! window appearing used to vanish without a trace. This module:
//! * writes a small log: `%LOCALAPPDATA%\RustDo\logs\rustdo.log` on Windows
//!   (`~/.local/share/RustDo/logs/` on Linux), one file per process kind ("rustdo",
//!   "companion"), rotated at 1 MB;
//! * records a start-up line (version, OS, exe, data folder) and every warning or error
//!   that RustDo or its libraries report;
//! * on a crash (panic), logs the message and a backtrace and, on Windows, shows a message
//!   box that says where the log is;
//! * starts the window with OpenGL and, on Windows, falls back to Direct3D (wgpu) if
//!   OpenGL isn't available (virtual machines, Remote Desktop, missing graphics drivers),
//!   remembering the choice for next time.

use crate::i18n::{tr, trf};
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;

const MAX_LOG_BYTES: u64 = 1024 * 1024;

/// `…\RustDo\logs` in the local (not roaming, not synced) app data folder.
pub fn log_dir() -> Option<PathBuf> {
    #[cfg(not(any(target_os = "android", target_arch = "wasm32")))]
    {
        // data_local_dir() is …\RustDo\data on Windows (logs go beside it, in
        // …\RustDo\logs) and ~/.local/share/rustdo on Linux (logs go inside it).
        let dirs = directories::ProjectDirs::from("", "", "RustDo")?;
        let data = dirs.data_local_dir();
        let base = if data.file_name().is_some_and(|n| n == "data") { data.parent()? } else { data };
        Some(base.join("logs"))
    }
    #[cfg(any(target_os = "android", target_arch = "wasm32"))]
    None
}

pub fn log_path(role: &str) -> Option<PathBuf> {
    Some(log_dir()?.join(format!("{role}.log")))
}

struct FileLogger {
    file: Mutex<File>,
}

impl log::Log for FileLogger {
    fn enabled(&self, m: &log::Metadata) -> bool {
        // Our own messages from Info up; libraries only when something's wrong.
        let ours = m.target().starts_with("rustdo");
        m.level() <= if ours { log::Level::Info } else { log::Level::Warn }
    }

    fn log(&self, record: &log::Record) {
        if !self.enabled(record.metadata()) {
            return;
        }
        let line = format!(
            "{} {:<5} [{}] {}\n",
            chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
            record.level(),
            record.target(),
            record.args()
        );
        if let Ok(mut f) = self.file.lock() {
            let _ = f.write_all(line.as_bytes());
        }
    }

    fn flush(&self) {
        if let Ok(mut f) = self.file.lock() {
            let _ = f.flush();
        }
    }
}

/// Starts logging for this process and installs the crash handler. Call first thing in
/// `main`. Never fails: without a writable log folder, RustDo simply runs without a log.
pub fn init(role: &'static str) {
    let path = log_path(role);
    if let Some(path) = &path {
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        // Keep it small: one previous file.
        if std::fs::metadata(path).is_ok_and(|m| m.len() > MAX_LOG_BYTES) {
            let _ = std::fs::rename(path, path.with_extension("old.log"));
        }
        if let Ok(file) = OpenOptions::new().create(true).append(true).open(path) {
            if log::set_boxed_logger(Box::new(FileLogger { file: Mutex::new(file) })).is_ok() {
                log::set_max_level(log::LevelFilter::Info);
            }
        }
    }
    install_panic_hook(role, path.clone());
    log::info!(target: "rustdo", "---- RustDo {} ({role}) starting ----", crate::update::CURRENT_VERSION);
    log::info!(target: "rustdo", "{}", system_summary());
}

fn install_panic_hook(role: &'static str, path: Option<PathBuf>) {
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let backtrace = std::backtrace::Backtrace::force_capture();
        log::error!(target: "rustdo", "CRASH: {info}\n{backtrace}");
        log::logger().flush();
        default_hook(info);
        // The background companion has no window: don't pop up boxes from it.
        if role != "companion" {
            let where_ = path.as_ref().map_or(tr("RustDo's logs folder").to_string(), |p| p.display().to_string());
            show_error(
                tr("RustDo ran into a problem"),
                &trf(
                    "RustDo has to close because of an unexpected error:\n\n{info}\n\nYour tasks are saved. The details are in:\n{path}\n\nPlease send that file with your report.",
                    &[("info", &info), ("path", &where_)],
                ),
            );
        }
    }));
}

/// Version, operating system and paths: the first thing to know about a problem report.
pub fn system_summary() -> String {
    let exe = std::env::current_exe().map(|p| p.display().to_string()).unwrap_or_default();
    format!(
        "version {} · {} · {} · exe {} · data {}",
        crate::update::CURRENT_VERSION,
        os_description(),
        std::env::consts::ARCH,
        exe,
        crate::storage::data_path().display()
    )
}

/// "Windows 10.0.19045" / the Linux distribution name.
pub fn os_description() -> String {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        if let Ok(out) = std::process::Command::new("cmd").args(["/c", "ver"]).creation_flags(CREATE_NO_WINDOW).output() {
            let text = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if !text.is_empty() {
                return text;
            }
        }
        "Windows (version unknown)".to_string()
    }
    #[cfg(target_os = "linux")]
    {
        std::fs::read_to_string("/etc/os-release")
            .ok()
            .and_then(|t| {
                t.lines()
                    .find_map(|l| l.strip_prefix("PRETTY_NAME=").map(|v| v.trim_matches('"').to_string()))
            })
            .unwrap_or_else(|| "Linux".to_string())
    }
    #[cfg(not(any(windows, target_os = "linux")))]
    {
        std::env::consts::OS.to_string()
    }
}

/// A message box (Windows); elsewhere the message goes to the log and stderr.
pub fn show_error(title: &str, text: &str) {
    log::error!(target: "rustdo", "{title}: {text}");
    #[cfg(windows)]
    {
        use windows::core::HSTRING;
        use windows::Win32::UI::WindowsAndMessaging::{MessageBoxW, MB_ICONERROR, MB_OK};
        unsafe {
            MessageBoxW(None, &HSTRING::from(text), &HSTRING::from(title), MB_OK | MB_ICONERROR);
        }
    }
    #[cfg(not(windows))]
    eprintln!("{title}: {text}");
}

// ---------------------------------------------------------------- starting the window

/// Remembers that Direct3D had to be used (Windows).
#[cfg(windows)]
fn renderer_file() -> Option<PathBuf> {
    Some(log_dir()?.parent()?.join("renderer.txt"))
}

/// Opens the window with eframe. Windows: OpenGL first (or Direct3D if that worked last
/// time); if the window can't be created, Direct3D (wgpu), which also works without a
/// graphics driver. Every failure is logged.
#[cfg(not(any(target_os = "android", target_arch = "wasm32")))]
pub fn run_native(
    app_name: &str,
    options: impl Fn() -> eframe::NativeOptions,
    mut create: impl FnMut(&eframe::CreationContext<'_>) -> Box<dyn eframe::App>,
) -> eframe::Result<()> {
    #[cfg(windows)]
    let use_wgpu = renderer_file().and_then(|f| std::fs::read_to_string(f).ok()).is_some_and(|s| s.trim() == "wgpu")
        || std::env::var("RUSTDO_RENDERER").is_ok_and(|v| v.eq_ignore_ascii_case("wgpu"));
    #[cfg(not(windows))]
    let use_wgpu = false;

    #[cfg_attr(not(windows), allow(unused_mut))]
    let mut opts = options();
    #[cfg(windows)]
    if use_wgpu {
        opts.renderer = eframe::Renderer::Wgpu;
    }
    log::info!(target: "rustdo", "opening the window with {}", if use_wgpu { "Direct3D (wgpu)" } else { "OpenGL (glow)" });
    let first = eframe::run_native(app_name, opts, Box::new(|cc| Ok(create(cc))));
    let Err(err) = first else { return Ok(()) };
    log::error!(target: "rustdo", "couldn't open the window: {err}");

    #[cfg(windows)]
    if !use_wgpu {
        log::warn!(target: "rustdo", "trying again with Direct3D (wgpu)");
        let mut opts = options();
        opts.renderer = eframe::Renderer::Wgpu;
        let second = eframe::run_native(app_name, opts, Box::new(|cc| Ok(create(cc))));
        match &second {
            Ok(()) => {
                if let Some(f) = renderer_file() {
                    let _ = std::fs::write(f, "wgpu");
                }
                log::info!(target: "rustdo", "Direct3D worked; it will be used from now on");
            }
            Err(e) => log::error!(target: "rustdo", "Direct3D didn't work either: {e}"),
        }
        return second;
    }
    Err(err)
}
