//! Running as a Microsoft Store app (an MSIX package, Windows).
//!
//! The Store installs and updates RustDo itself and signs the package, so the packaged copy:
//! * doesn't check for, download or install updates (the Store does that, and its
//!   policies don't allow apps to update themselves);
//! * starts the background companion at sign-in through the package's startup task
//!   ("RustDoCompanion" in AppxManifest.xml) instead of the registry's Run key, which a
//!   packaged app can't use. Windows starts the app without arguments for it, so
//!   [`launched_at_sign_in`] tells the two apart;
//! * shows notifications under the package's own identity.
//!
//! Elsewhere (the installer or portable exe, Linux, Android, web) everything here reports
//! "not packaged" and does nothing.

/// The startup task's id in AppxManifest.xml.
#[cfg(windows)]
const STARTUP_TASK_ID: &str = "RustDoCompanion";

/// True when RustDo runs from a Store (MSIX) package.
pub fn is_packaged() -> bool {
    #[cfg(windows)]
    {
        use std::sync::OnceLock;
        static PACKAGED: OnceLock<bool> = OnceLock::new();
        *PACKAGED.get_or_init(|| {
            use windows::core::PWSTR;
            use windows::Win32::Foundation::APPMODEL_ERROR_NO_PACKAGE;
            use windows::Win32::Storage::Packaging::Appx::GetCurrentPackageFullName;
            let mut len = 0u32;
            // Asking for the length only: "no package" is the one answer that means unpackaged.
            let result = unsafe { GetCurrentPackageFullName(&mut len, PWSTR::null()) };
            result != APPMODEL_ERROR_NO_PACKAGE
        })
    }
    #[cfg(not(windows))]
    false
}

/// True when Windows started the packaged app through its startup task (at sign-in).
pub fn launched_at_sign_in() -> bool {
    #[cfg(windows)]
    {
        if !is_packaged() {
            return false;
        }
        use windows::ApplicationModel::Activation::ActivationKind;
        use windows::ApplicationModel::AppInstance;
        AppInstance::GetActivatedEventArgs()
            .and_then(|args| args.Kind())
            .is_ok_and(|kind| kind == ActivationKind::StartupTask)
    }
    #[cfg(not(windows))]
    false
}

/// Turns the packaged app's start-at-sign-in on or off (Settings › Background). Returns
/// false if Windows refused (e.g. the user turned it off in Task Manager › Startup apps,
/// which only they can undo there).
pub fn set_startup_task(on: bool) -> bool {
    #[cfg(windows)]
    {
        use windows::ApplicationModel::{StartupTask, StartupTaskState};
        use windows::core::HSTRING;
        let Ok(task) = StartupTask::GetAsync(&HSTRING::from(STARTUP_TASK_ID)).and_then(|op| op.get()) else {
            log::warn!(target: "rustdo", "the package has no startup task {STARTUP_TASK_ID}");
            return false;
        };
        let state = task.State().unwrap_or(StartupTaskState::Disabled);
        if on {
            if state == StartupTaskState::Enabled {
                return true;
            }
            match task.RequestEnableAsync().and_then(|op| op.get()) {
                Ok(StartupTaskState::Enabled) => true,
                Ok(other) => {
                    log::warn!(target: "rustdo", "Windows kept start-at-sign-in off ({other:?})");
                    false
                }
                Err(e) => {
                    log::warn!(target: "rustdo", "couldn't turn on start-at-sign-in: {e}");
                    false
                }
            }
        } else {
            if state == StartupTaskState::Enabled {
                let _ = task.Disable();
            }
            true
        }
    }
    #[cfg(not(windows))]
    {
        let _ = on;
        false
    }
}

/// Store package: if the Store has an update for RustDo waiting, installs it now, quietly.
/// Windows closes RustDo to do it, and starts the background companion again afterwards
/// (it asks to be restarted first). Called by the companion only while the main window is
/// closed, so nobody is interrupted. Returns whether an update was found.
///
/// Without this, a running companion holds the update back until it's quit or the PC
/// restarts. Errors (offline, auto-updates turned off in the Store) just leave things as
/// they are.
pub fn install_waiting_update() -> Result<bool, String> {
    #[cfg(windows)]
    {
        use windows::core::HSTRING;
        use windows::Services::Store::StoreContext;
        use windows::Win32::System::Recovery::{RegisterApplicationRestart, RESTART_NO_CRASH, RESTART_NO_HANG, RESTART_NO_REBOOT};
        if !is_packaged() {
            return Ok(false);
        }
        let e = |e: windows::core::Error| e.message().to_string();
        let store = StoreContext::GetDefault().map_err(e)?;
        let updates = store.GetAppAndOptionalStorePackageUpdatesAsync().and_then(|op| op.get()).map_err(e)?;
        if updates.Size().map_err(e)? == 0 {
            return Ok(false);
        }
        if !store.CanSilentlyDownloadStorePackageUpdates().unwrap_or(false) {
            log::info!(target: "rustdo", "a Store update is waiting, but Windows doesn't allow installing it quietly now");
            return Ok(true);
        }
        // Come back after the update (not after a crash, hang or reboot: sign-in does that).
        let restart = unsafe { RegisterApplicationRestart(&HSTRING::from("--companion"), RESTART_NO_CRASH | RESTART_NO_HANG | RESTART_NO_REBOOT) };
        if let Err(err) = restart {
            log::warn!(target: "rustdo", "couldn't ask to be restarted after the update: {err}");
        }
        log::info!(target: "rustdo", "installing a waiting Store update");
        let result = store.TrySilentDownloadAndInstallStorePackageUpdatesAsync(&updates).and_then(|op| op.get()).map_err(e)?;
        log::info!(target: "rustdo", "Store update finished: {:?}", result.OverallState());
        Ok(true)
    }
    #[cfg(not(windows))]
    Ok(false)
}
