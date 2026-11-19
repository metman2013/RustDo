//! Update checks against this project's GitHub Releases (desktop and Android).
//!
//! The only network request RustDo makes: an anonymous GET to GitHub's public API,
//! at most once a day, and only while "Check for updates" is on. No task data is sent.

use std::sync::mpsc::{channel, Receiver};

pub const REPO: &str = "metman2013/RustDo";
pub const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Clone, PartialEq)]
pub struct Release {
    pub version: String,
    pub notes_url: String,
    /// Windows installer (desktop) or APK (Android) for this release, if attached.
    pub download_url: Option<String>,
    pub download_name: Option<String>,
}

#[derive(Debug)]
pub enum CheckResult {
    UpToDate,
    Available(Release),
    Failed(String),
}

/// "v1.2.3" / "1.2.3" → (1, 2, 3). Missing parts count as 0.
pub fn parse_version(v: &str) -> Option<(u64, u64, u64)> {
    let v = v.trim().trim_start_matches(['v', 'V']);
    let core = v.split(['-', '+']).next()?;
    let mut parts = core.split('.').map(|p| p.parse::<u64>());
    let major = parts.next()?.ok()?;
    let minor = parts.next().unwrap_or(Ok(0)).ok()?;
    let patch = parts.next().unwrap_or(Ok(0)).ok()?;
    Some((major, minor, patch))
}

pub fn is_newer(latest: &str, current: &str) -> bool {
    match (parse_version(latest), parse_version(current)) {
        (Some(l), Some(c)) => l > c,
        _ => false,
    }
}

/// True when running from the Windows installer's copy (Inno Setup puts its uninstaller
/// next to the exe), as opposed to the portable exe, which should never update itself.
pub fn is_installed_copy() -> bool {
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|dir| dir.join("unins000.exe")))
        .is_some_and(|uninstaller| uninstaller.exists())
}

/// Which release asset this platform installs.
fn wanted_asset(name: &str) -> bool {
    let n = name.to_lowercase();
    if cfg!(target_os = "android") {
        n.ends_with(".apk")
    } else if cfg!(windows) {
        n.ends_with("-setup.exe")
    } else {
        false
    }
}

/// Parses the GitHub "latest release" API response.
pub fn parse_release(json: &str) -> Result<Release, String> {
    let v: serde_json::Value = serde_json::from_str(json).map_err(|e| e.to_string())?;
    let tag = v.get("tag_name").and_then(|t| t.as_str()).ok_or("no tag_name in release")?;
    let asset = v
        .get("assets")
        .and_then(|a| a.as_array())
        .and_then(|assets| {
            assets
                .iter()
                .find(|a| a.get("name").and_then(|n| n.as_str()).is_some_and(wanted_asset))
        });
    Ok(Release {
        version: tag.trim_start_matches(['v', 'V']).to_string(),
        notes_url: v
            .get("html_url")
            .and_then(|u| u.as_str())
            .map(str::to_string)
            .unwrap_or_else(|| format!("https://github.com/{REPO}/releases/latest")),
        download_url: asset
            .and_then(|a| a.get("browser_download_url"))
            .and_then(|u| u.as_str())
            .map(str::to_string),
        download_name: asset.and_then(|a| a.get("name")).and_then(|n| n.as_str()).map(str::to_string),
    })
}

#[cfg(all(not(target_arch = "wasm32"), feature = "updates"))]
fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout(std::time::Duration::from_secs(30))
        // A generic user agent: no machine or user details.
        .user_agent(&format!("RustDo/{CURRENT_VERSION}"))
        .build()
}

/// Starts a check on a background thread; poll the receiver each frame.
#[cfg(all(not(target_arch = "wasm32"), feature = "updates"))]
pub fn check_in_background(ctx: eframe::egui::Context) -> Receiver<CheckResult> {
    let (tx, rx) = channel();
    std::thread::spawn(move || {
        let url = format!("https://api.github.com/repos/{REPO}/releases/latest");
        let result = match agent().get(&url).set("Accept", "application/vnd.github+json").call() {
            Ok(resp) => match resp.into_string() {
                Ok(body) => match parse_release(&body) {
                    Ok(r) if is_newer(&r.version, CURRENT_VERSION) => CheckResult::Available(r),
                    Ok(_) => CheckResult::UpToDate,
                    Err(e) => CheckResult::Failed(e),
                },
                Err(e) => CheckResult::Failed(e.to_string()),
            },
            // 404 = no releases published yet.
            Err(ureq::Error::Status(404, _)) => CheckResult::UpToDate,
            Err(e) => CheckResult::Failed(e.to_string()),
        };
        let _ = tx.send(result);
        ctx.request_repaint();
    });
    rx
}

/// The web app updates itself through its service worker; builds without the
/// `updates` feature never contact the network.
#[cfg(any(target_arch = "wasm32", not(feature = "updates")))]
pub fn check_in_background(_ctx: eframe::egui::Context) -> Receiver<CheckResult> {
    let (tx, rx) = channel();
    let _ = tx.send(if cfg!(target_arch = "wasm32") {
        CheckResult::UpToDate
    } else {
        CheckResult::Failed("this build doesn't include update checks".into())
    });
    rx
}

/// Desktop: downloads the installer to the temp folder and returns its path.
#[cfg(all(windows, feature = "updates"))]
pub fn download_installer(url: &str, name: &str) -> Result<std::path::PathBuf, String> {
    // Only accept files from this project's GitHub releases.
    let allowed = format!("https://github.com/{REPO}/releases/download/");
    if !url.starts_with(&allowed) {
        return Err("unexpected download location".into());
    }
    let safe_name: String = name.chars().filter(|c| c.is_ascii_alphanumeric() || "-_.".contains(*c)).collect();
    let path = std::env::temp_dir().join(if safe_name.is_empty() { "RustDo-setup.exe".into() } else { safe_name });
    let resp = agent().get(url).call().map_err(|e| e.to_string())?;
    let mut file = std::fs::File::create(&path).map_err(|e| e.to_string())?;
    std::io::copy(&mut resp.into_reader(), &mut file).map_err(|e| e.to_string())?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions() {
        assert!(is_newer("v0.2.0", "0.1.9"));
        assert!(is_newer("1.0", "0.9.9"));
        assert!(!is_newer("v0.1.0", "0.1.0"));
        assert!(!is_newer("0.1.0-beta", "0.1.0"));
        assert!(!is_newer("garbage", "0.1.0"));
        assert_eq!(parse_version("V2.10.3+build"), Some((2, 10, 3)));
    }

    #[test]
    fn parses_github_release() {
        let json = r#"{"tag_name":"v0.3.0","html_url":"https://github.com/metman2013/RustDo/releases/tag/v0.3.0",
            "assets":[{"name":"RustDo.apk","browser_download_url":"https://github.com/metman2013/RustDo/releases/download/v0.3.0/RustDo.apk"},
                      {"name":"RustDo-0.3.0-setup.exe","browser_download_url":"https://github.com/metman2013/RustDo/releases/download/v0.3.0/RustDo-0.3.0-setup.exe"}]}"#;
        let r = parse_release(json).unwrap();
        assert_eq!(r.version, "0.3.0");
        if cfg!(windows) {
            assert_eq!(r.download_name.as_deref(), Some("RustDo-0.3.0-setup.exe"));
        }
    }
}
