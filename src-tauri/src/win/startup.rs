//! Start with Windows, as a shortcut in the user's Startup folder (`shell:startup`).
//!
//! On the user's PC Windows silently skipped the registry Run entry older versions used; the shortcut is
//! what it launches reliably. Windows can still switch the shortcut off (Task Manager's Startup apps page),
//! so "on" means the shortcut exists and isn't switched off there, and turning it on clears that switch.

use std::{
    path::{Path, PathBuf},
    thread,
};

use windows::core::{w, Interface, HSTRING, PCWSTR};
use windows::Win32::{
    Foundation::ERROR_SUCCESS,
    Storage::FileSystem::WIN32_FIND_DATAW,
    System::{
        Com::{CoCreateInstance, IPersistFile, CLSCTX_INPROC_SERVER, STGM_READ},
        Registry::{RegDeleteKeyValueW, RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_BINARY},
    },
    UI::Shell::{FOLDERID_Startup, IShellLinkW, SHGetKnownFolderPath, ShellLink, KF_FLAG_DEFAULT},
};

use super::{
    com::ComGuard,
    util::{take_pwstr, text_until_nul},
};
use crate::error::{AudioError, Result};

const SHORTCUT_FILE: &str = "Resonance.lnk";
const SHORTCUT_NAME: PCWSTR = w!("Resonance.lnk");
/// Passed by the shortcut so Resonance starts in the tray.
pub const MINIMIZED_ARG: &str = "--minimized";

const RUN_KEY: PCWSTR = w!("Software\\Microsoft\\Windows\\CurrentVersion\\Run");
const APPROVED_RUN_KEY: PCWSTR = w!("Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\StartupApproved\\Run");
/// Where Windows records Startup-folder entries switched off in Task Manager (odd first byte = off).
const APPROVED_FOLDER_KEY: PCWSTR =
    w!("Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\StartupApproved\\StartupFolder");

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
    Ok(startup_folder()?.join(SHORTCUT_FILE))
}

fn current_exe() -> Result<PathBuf> {
    std::env::current_exe().map_err(|e| AudioError::Os(format!("Couldn't find Resonance's own file: {e}")))
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

/// A shortcut's target and arguments (COM must be initialised on this thread).
fn read_shortcut(link: &Path) -> Result<(PathBuf, String)> {
    unsafe {
        let shell_link: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER)?;
        let file: IPersistFile = shell_link.cast()?;
        file.Load(&HSTRING::from(link.as_os_str()), STGM_READ)?;
        let mut target = [0u16; 1024];
        shell_link.GetPath(&mut target, std::ptr::null_mut::<WIN32_FIND_DATAW>(), 0)?;
        let mut args = [0u16; 1024];
        shell_link.GetArguments(&mut args)?;
        Ok((PathBuf::from(text_until_nul(&target).unwrap_or_default()), text_until_nul(&args).unwrap_or_default()))
    }
}

fn delete_value(key: PCWSTR, name: PCWSTR) -> bool {
    unsafe { RegDeleteKeyValueW(HKEY_CURRENT_USER, key, name) == ERROR_SUCCESS }
}

/// Whether Windows has switched the Startup-folder shortcut off (Task Manager's Startup apps page).
fn switched_off_by_windows() -> bool {
    let mut data = [0u8; 12];
    let mut len = data.len() as u32;
    let found = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            APPROVED_FOLDER_KEY,
            SHORTCUT_NAME,
            RRF_RT_REG_BINARY,
            None,
            Some(data.as_mut_ptr().cast()),
            Some(&mut len),
        )
    } == ERROR_SUCCESS;
    found && len > 0 && data[0] & 1 == 1
}

/// Removes the registry Run entry (and its approval flag) older versions used. Returns true if there was one.
fn remove_legacy_run_entry() -> bool {
    delete_value(APPROVED_RUN_KEY, w!("Resonance"));
    delete_value(RUN_KEY, w!("Resonance"))
}

/// Whether Resonance starts with Windows.
pub fn is_enabled() -> Result<bool> {
    Ok(shortcut_path()?.exists() && !switched_off_by_windows())
}

/// Makes Resonance start with Windows (in the tray), pointing at the running executable, and clears a
/// "switched off" mark Windows may have kept for it.
pub fn enable() -> Result<()> {
    let (link, exe) = (shortcut_path()?, current_exe()?);
    with_com(move || write_shortcut(&link, &exe, MINIMIZED_ARG))?;
    delete_value(APPROVED_FOLDER_KEY, SHORTCUT_NAME);
    Ok(())
}

/// Stops Resonance starting with Windows. Also clears an older version's registry entry, since release
/// builds only migrate that on launch and this may be a development build.
pub fn disable() -> Result<()> {
    remove_legacy_run_entry();
    match std::fs::remove_file(shortcut_path()?) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(AudioError::Os(format!("Couldn't remove the start-up shortcut: {e}"))),
    }
}

/// Called by release builds on launch: moves an older version's registry entry to the shortcut, and
/// re-points an existing shortcut that doesn't start this executable (after an update or a move). Leaves a
/// correct shortcut untouched.
pub fn refresh() -> Result<()> {
    let (link, exe) = (shortcut_path()?, current_exe()?);
    let migrate = remove_legacy_run_entry();
    if !migrate && !link.exists() {
        return Ok(());
    }
    with_com(move || {
        let wanted = (exe, MINIMIZED_ARG.to_string());
        if migrate || read_shortcut(&link).ok().as_ref() != Some(&wanted) {
            write_shortcut(&link, &wanted.0, MINIMIZED_ARG)?;
        }
        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_a_shortcut_that_starts_the_exe_minimized() {
        let dir = tempfile::tempdir().unwrap();
        let link = dir.path().join(SHORTCUT_FILE);
        let exe = current_exe().unwrap();
        let (l, e) = (link.clone(), exe.clone());
        let read = with_com(move || {
            write_shortcut(&l, &e, MINIMIZED_ARG)?;
            read_shortcut(&l)
        })
        .unwrap();
        assert_eq!(read, (exe, MINIMIZED_ARG.to_string()));
        assert!(link.exists());
    }

    #[test]
    fn finds_the_startup_folder() {
        let folder = startup_folder().unwrap();
        assert!(folder.ends_with(r"Start Menu\Programs\Startup"), "{}", folder.display());
        assert_eq!(shortcut_path().unwrap(), folder.join(SHORTCUT_FILE));
    }
}
