//! Start with Windows, as a shortcut in the user's Startup folder (`shell:startup`).
//!
//! A Startup-folder shortcut is what Windows launches most reliably at sign-in; the registry Run entry
//! older versions used was sometimes skipped. Turning the feature on or refreshing it also removes that
//! old entry.

use std::{
    path::{Path, PathBuf},
    thread,
};

use windows::core::{w, Interface, HSTRING};
use windows::Win32::{
    Foundation::ERROR_SUCCESS,
    System::{
        Com::{CoCreateInstance, IPersistFile, CLSCTX_INPROC_SERVER},
        Registry::{RegDeleteKeyValueW, HKEY_CURRENT_USER},
    },
    UI::Shell::{FOLDERID_Startup, IShellLinkW, SHGetKnownFolderPath, ShellLink, KF_FLAG_DEFAULT},
};

use super::{com::ComGuard, util::take_pwstr};
use crate::error::{AudioError, Result};

const SHORTCUT_NAME: &str = "Resonance.lnk";
/// Passed by the shortcut so Resonance starts in the tray.
pub const MINIMIZED_ARG: &str = "--minimized";

/// Runs `f` on a fresh thread with COM initialised, so callers on any thread (including Tauri's UI
/// thread, which has its own apartment) can use the shell APIs.
fn with_com<T: Send + 'static>(f: impl FnOnce() -> Result<T> + Send + 'static) -> Result<T> {
    thread::spawn(move || {
        let _com = ComGuard::init_mta()?;
        f()
    })
    .join()
    .unwrap_or_else(|_| Err(AudioError::Os("The start-up helper stopped unexpectedly".into())))
}

fn startup_folder() -> Result<PathBuf> {
    unsafe { Ok(PathBuf::from(take_pwstr(SHGetKnownFolderPath(&FOLDERID_Startup, KF_FLAG_DEFAULT, None)?))) }
}

fn shortcut_path() -> Result<PathBuf> {
    Ok(startup_folder()?.join(SHORTCUT_NAME))
}

/// Writes a shortcut at `link` that runs `exe` with `args` (COM must be initialised on this thread).
fn write_shortcut(link: &Path, exe: &Path, args: &str) -> Result<()> {
    unsafe {
        let shell_link: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER)?;
        shell_link.SetPath(&HSTRING::from(exe.as_os_str()))?;
        shell_link.SetArguments(&HSTRING::from(args))?;
        if let Some(dir) = exe.parent() {
            shell_link.SetWorkingDirectory(&HSTRING::from(dir.as_os_str()))?;
        }
        shell_link.SetDescription(w!("Resonance"))?;
        let file: IPersistFile = shell_link.cast()?;
        file.Save(&HSTRING::from(link.as_os_str()), true)?;
    }
    Ok(())
}

/// Reads a shortcut's target and arguments (COM must be initialised on this thread).
#[cfg(test)]
fn read_shortcut(link: &Path) -> Result<(PathBuf, String)> {
    use windows::Win32::{Storage::FileSystem::WIN32_FIND_DATAW, System::Com::STGM_READ};
    unsafe {
        let shell_link: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER)?;
        let file: IPersistFile = shell_link.cast()?;
        file.Load(&HSTRING::from(link.as_os_str()), STGM_READ)?;
        let mut target = [0u16; 1024];
        shell_link.GetPath(&mut target, std::ptr::null_mut::<WIN32_FIND_DATAW>(), 0)?;
        let mut args = [0u16; 1024];
        shell_link.GetArguments(&mut args)?;
        let text = |buf: &[u16]| String::from_utf16_lossy(&buf[..buf.iter().position(|c| *c == 0).unwrap_or(buf.len())]);
        Ok((PathBuf::from(text(&target)), text(&args)))
    }
}

/// Removes the registry Run entry older versions used. Returns true if there was one.
fn remove_legacy_run_entry() -> bool {
    unsafe {
        RegDeleteKeyValueW(HKEY_CURRENT_USER, w!("Software\\Microsoft\\Windows\\CurrentVersion\\Run"), w!("Resonance"))
            == ERROR_SUCCESS
    }
}

/// Whether Resonance starts with Windows.
pub fn is_enabled() -> Result<bool> {
    Ok(shortcut_path()?.exists())
}

/// Makes Resonance start with Windows (in the tray), pointing at the running executable.
pub fn enable() -> Result<()> {
    let exe = std::env::current_exe().map_err(|e| AudioError::Os(format!("Couldn't find Resonance's own file: {e}")))?;
    let link = shortcut_path()?;
    with_com(move || write_shortcut(&link, &exe, MINIMIZED_ARG))?;
    remove_legacy_run_entry();
    Ok(())
}

/// Stops Resonance starting with Windows.
pub fn disable() -> Result<()> {
    remove_legacy_run_entry();
    match std::fs::remove_file(shortcut_path()?) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(AudioError::Os(format!("Couldn't remove the start-up shortcut: {e}"))),
    }
}

/// If Start with Windows is on (a shortcut, or the registry entry of an older version), points it at this
/// executable. Called by release builds on launch, so an update, a move or an entry made by another copy
/// never leaves Windows starting the wrong file.
pub fn refresh() -> Result<()> {
    let legacy = remove_legacy_run_entry();
    if legacy || is_enabled()? {
        enable()?;
    }
    Ok(())
}

/// A shortcut's target and arguments.
#[cfg(test)]
fn describe(link: &Path) -> Result<(PathBuf, String)> {
    let link = link.to_path_buf();
    with_com(move || read_shortcut(&link))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_a_shortcut_that_starts_the_exe_minimized() {
        let dir = tempfile::tempdir().unwrap();
        let link = dir.path().join(SHORTCUT_NAME);
        let exe = std::env::current_exe().unwrap();
        let (l, e) = (link.clone(), exe.clone());
        with_com(move || write_shortcut(&l, &e, MINIMIZED_ARG)).unwrap();
        assert!(link.exists());
        let (target, args) = describe(&link).unwrap();
        assert_eq!(target, exe);
        assert_eq!(args, "--minimized");
    }

    #[test]
    fn finds_the_startup_folder() {
        let folder = startup_folder().unwrap();
        assert!(folder.ends_with(r"Start Menu\Programs\Startup"), "{}", folder.display());
    }

    #[test]
    fn com_helper_reports_errors() {
        let missing = PathBuf::from(r"C:\definitely\not\here.lnk");
        assert!(describe(&missing).is_err());
    }
}
