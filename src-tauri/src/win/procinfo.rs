use std::ffi::c_void;

use windows::core::{w, PCWSTR, PWSTR};
use windows::Win32::{
    Foundation::{CloseHandle, ERROR_INVALID_PARAMETER},
    Storage::FileSystem::{GetFileVersionInfoSizeW, GetFileVersionInfoW, VerQueryValueW},
    System::Threading::{OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION},
};

use super::util::to_wide;

#[derive(Debug, Clone)]
pub struct ProcInfo {
    pub exe_path: String,
    pub description: Option<String>,
}

#[derive(Debug, Clone)]
pub enum ProcLookup {
    /// The process has exited (stale session).
    Dead,
    /// Alive but unreadable (e.g. protected process).
    Unknown,
    Found(ProcInfo),
}

pub fn lookup(pid: u32) -> ProcLookup {
    unsafe {
        let handle = match OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) {
            Ok(h) => h,
            Err(e) if e.code() == ERROR_INVALID_PARAMETER.to_hresult() => return ProcLookup::Dead,
            Err(_) => return ProcLookup::Unknown,
        };
        let mut buf = [0u16; 1024];
        let mut len = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(handle, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut len);
        let _ = CloseHandle(handle);
        if ok.is_err() {
            return ProcLookup::Unknown;
        }
        let exe_path = String::from_utf16_lossy(&buf[..len as usize]);
        let description = file_description(&exe_path);
        ProcLookup::Found(ProcInfo { exe_path, description })
    }
}

/// Reads the FileDescription version resource (e.g. "Google Chrome").
pub fn file_description(path: &str) -> Option<String> {
    unsafe {
        let wide = to_wide(path);
        let size = GetFileVersionInfoSizeW(PCWSTR(wide.as_ptr()), None);
        if size == 0 {
            return None;
        }
        let mut data = vec![0u8; size as usize];
        GetFileVersionInfoW(PCWSTR(wide.as_ptr()), None, size, data.as_mut_ptr() as *mut c_void).ok()?;
        let mut ptr: *mut c_void = std::ptr::null_mut();
        let mut len = 0u32;
        if !VerQueryValueW(data.as_ptr() as *const c_void, w!("\\VarFileInfo\\Translation"), &mut ptr, &mut len).as_bool()
            || len < 4
        {
            return None;
        }
        let lang = *(ptr as *const u16);
        let codepage = *(ptr as *const u16).add(1);
        let key = to_wide(&format!("\\StringFileInfo\\{lang:04x}{codepage:04x}\\FileDescription"));
        if !VerQueryValueW(data.as_ptr() as *const c_void, PCWSTR(key.as_ptr()), &mut ptr, &mut len).as_bool() || len == 0 {
            return None;
        }
        let text = std::slice::from_raw_parts(ptr as *const u16, len as usize);
        let s = String::from_utf16_lossy(text).trim_end_matches('\0').trim().to_string();
        (!s.is_empty()).then_some(s)
    }
}
