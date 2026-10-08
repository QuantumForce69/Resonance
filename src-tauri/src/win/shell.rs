use windows::core::{w, HSTRING, PCWSTR};
use windows::Win32::UI::{Shell::ShellExecuteW, WindowsAndMessaging::SW_SHOWNORMAL};

/// Opens a web page in the default browser.
pub fn open_url(url: &str) {
    unsafe {
        let _ = ShellExecuteW(None, w!("open"), &HSTRING::from(url), PCWSTR::null(), PCWSTR::null(), SW_SHOWNORMAL);
    }
}
