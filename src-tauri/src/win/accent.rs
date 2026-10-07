use windows::core::w;
use windows::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_DWORD};

const FALLBACK: &str = "#8b6cff";

/// The Windows accent colour (`HKCU\Software\Microsoft\Windows\DWM\AccentColor`, 0xAABBGGRR) as `#rrggbb`.
pub fn accent_hex() -> String {
    let mut data: u32 = 0;
    let mut size = std::mem::size_of::<u32>() as u32;
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            w!("Software\\Microsoft\\Windows\\DWM"),
            w!("AccentColor"),
            RRF_RT_REG_DWORD,
            None,
            Some(&mut data as *mut u32 as *mut _),
            Some(&mut size),
        )
    };
    if status.is_err() {
        return FALLBACK.into();
    }
    let (r, g, b) = (data & 0xff, (data >> 8) & 0xff, (data >> 16) & 0xff);
    format!("#{r:02x}{g:02x}{b:02x}")
}
