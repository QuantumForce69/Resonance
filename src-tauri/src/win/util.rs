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
