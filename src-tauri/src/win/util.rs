use windows::core::PWSTR;
use windows::Win32::System::Com::CoTaskMemFree;

/// Copies a CoTaskMem-allocated wide string and frees it.
pub unsafe fn take_pwstr(p: PWSTR) -> String {
    if p.is_null() {
        return String::new();
    }
    let s = p.to_string().unwrap_or_default();
    CoTaskMemFree(Some(p.0 as *const _));
    s
}

/// Decodes a wide-string buffer up to its first NUL, trimmed; `None` if that leaves nothing. Some
/// executables (e.g. Spotify) report version-resource lengths that run past the terminator, so a reported
/// length can't be trusted.
pub fn text_until_nul(raw: &[u16]) -> Option<String> {
    let end = raw.iter().position(|&c| c == 0).unwrap_or(raw.len());
    let s = String::from_utf16_lossy(&raw[..end]).trim().to_string();
    (!s.is_empty()).then_some(s)
}
