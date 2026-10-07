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
        text_until_nul(std::slice::from_raw_parts(ptr as *const u16, len as usize))
    }
}

/// Decodes a version-resource string up to its first NUL. Some executables (e.g. Spotify) report a
/// length that runs past the terminator into the next resource entry, so the length can't be trusted.
fn text_until_nul(raw: &[u16]) -> Option<String> {
    let end = raw.iter().position(|&c| c == 0).unwrap_or(raw.len());
    let s = String::from_utf16_lossy(&raw[..end]).trim().to_string();
    (!s.is_empty()).then_some(s)
}


#[cfg(test)]
mod tests {
    use super::*;

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().collect()
    }

    #[test]
    fn stops_at_the_first_nul_even_when_the_reported_length_overruns() {
        // Spotify.exe reports a length that runs into the next version-resource entry.
        let mut raw = wide("Spotify");
        raw.extend([0, 0x34, 0x14, 0x01]);
        raw.extend(wide("FileV"));
        assert_eq!(text_until_nul(&raw).as_deref(), Some("Spotify"));
    }

    /// Reads the FileDescription of every .exe under the usual install folders and checks none
    /// contains control characters (the symptom of reading past the string's terminator).
    #[test]
    #[ignore]
    fn installed_program_names_are_clean() {
        fn walk(dir: &std::path::Path, depth: u32, out: &mut Vec<std::path::PathBuf>) {
            let Ok(entries) = std::fs::read_dir(dir) else { return };
            for e in entries.flatten() {
                let p = e.path();
                if p.is_dir() && depth > 0 {
                    walk(&p, depth - 1, out);
                } else if p.extension().is_some_and(|x| x.eq_ignore_ascii_case("exe")) {
                    out.push(p);
                }
            }
        }
        let mut exes = Vec::new();
        for var in ["ProgramFiles", "ProgramFiles(x86)", "APPDATA", "LOCALAPPDATA"] {
            if let Some(root) = std::env::var_os(var) {
                walk(std::path::Path::new(&root), 4, &mut exes);
            }
        }
        let mut named = 0;
        let mut bad = Vec::new();
        for exe in &exes {
            if let Some(name) = file_description(&exe.to_string_lossy()) {
                named += 1;
                if name.chars().any(char::is_control) {
                    bad.push(format!("{} => {name:?}", exe.display()));
                }
            }
        }
        println!("checked {} exes, {named} with names", exes.len());
        assert!(bad.is_empty(), "names with control characters:\n{}", bad.join("\n"));
    }

    #[test]
    fn trims_whitespace_and_rejects_empty_text() {
        assert_eq!(text_until_nul(&wide(" Discord ")).as_deref(), Some("Discord"));
        assert_eq!(text_until_nul(&[0, 0x41]), None);
        assert_eq!(text_until_nul(&[]), None);
    }
}
