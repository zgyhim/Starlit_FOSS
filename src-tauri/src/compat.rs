//! 平台兼容性检查：Windows 7 / 8 / 8.1 无法运行 WebView2，启动时直接提示并退出。

#[cfg(windows)]
fn windows_supported() -> bool {
    use winreg::enums::HKEY_LOCAL_MACHINE;
    use winreg::RegKey;

    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    let Ok(key) = hklm.open_subkey(r"SOFTWARE\Microsoft\Windows NT\CurrentVersion") else {
        return true;
    };
    // Windows 10/11 提供 CurrentMajorVersionNumber。
    if let Ok(major) = key.get_value::<u32, _>("CurrentMajorVersionNumber") {
        return major >= 10;
    }
    // 旧系统只有 CurrentVersion：6.1 / 6.2 / 6.3。
    if let Ok(version) = key.get_value::<String, _>("CurrentVersion") {
        let major = version
            .split('.')
            .next()
            .and_then(|value| value.parse::<u32>().ok())
            .unwrap_or(10);
        return major >= 10;
    }
    true
}

#[cfg(windows)]
fn show_warning() {
    use std::os::windows::ffi::OsStrExt;
    use winapi::um::winuser::{MessageBoxW, MB_ICONERROR, MB_OK};

    fn wide(value: &str) -> Vec<u16> {
        std::ffi::OsStr::new(value)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect()
    }

    let text = wide(
        "星伴 FOSS 需要 Windows 10 或更高版本。\n\n\
         当前系统版本过低，无法运行本应用。\n\n\
         请升级 Windows 后重试。",
    );
    let title = wide("星伴 FOSS · 兼容性警告");
    unsafe {
        MessageBoxW(
            std::ptr::null_mut(),
            text.as_ptr(),
            title.as_ptr(),
            MB_OK | MB_ICONERROR,
        );
    }
}

pub fn ensure_supported() {
    #[cfg(windows)]
    {
        if !windows_supported() {
            show_warning();
            std::process::exit(2);
        }
    }
}
