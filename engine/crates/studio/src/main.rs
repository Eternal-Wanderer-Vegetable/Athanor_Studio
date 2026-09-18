// This file is part of Athanor, the Azodoc document engine.
// Copyright (C) 2026 The Athanor Studio Developers
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License as published
// by the Free Software Foundation, version 3 of the License only.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

/// WebView2 运行时预检（E6）：缺失时给可读指引而不是深层 panic。
/// 检测规则与 Tauri/Wry 一致：注册表 32/64 位视图的
/// `Microsoft.EdgeWebView2Runtime\Current\ pv` 任意非空版本即视为可用。
#[cfg(windows)]
fn webview2_present() -> bool {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    use std::ptr;
    const HKEY_LOCAL_MACHINE: isize = 0x80000002u32 as isize;
    const KEY_QUERY_VALUE: u32 = 0x0001;
    const KEY_WOW64_32KEY: u32 = 0x0200;
    const KEY_WOW64_64KEY: u32 = 0x0100;
    #[allow(clippy::upper_case_acronyms)] // 与 Win32 头文件命名一致
    type HKEY = *mut core::ffi::c_void;
    unsafe extern "system" {
        fn RegOpenKeyExW(
            hkey: isize,
            subkey: *const u16,
            options: u32,
            sam: u32,
            result: *mut HKEY,
        ) -> i32;
        fn RegQueryValueExW(
            hkey: HKEY,
            value: *const u16,
            reserved: *mut u32,
            ty: *mut u32,
            data: *mut u8,
            len: *mut u32,
        ) -> i32;
        fn RegCloseKey(hkey: HKEY) -> i32;
    }
    unsafe {
        for sam in [KEY_WOW64_64KEY, KEY_WOW64_32KEY, 0] {
            let sub: Vec<u16> = OsStr::new(
                "SOFTWARE\\Microsoft\\EdgeUpdate\\Clients\\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}",
            )
            .encode_wide()
            .chain([0])
            .collect();
            let mut key: HKEY = ptr::null_mut();
            if RegOpenKeyExW(
                HKEY_LOCAL_MACHINE,
                sub.as_ptr(),
                0,
                KEY_QUERY_VALUE | sam,
                &mut key,
            ) != 0
            {
                continue;
            }
            let mut len: u32 = 0;
            let val: Vec<u16> = OsStr::new("pv").encode_wide().chain([0]).collect();
            let rc = RegQueryValueExW(
                key,
                val.as_ptr(),
                ptr::null_mut(),
                ptr::null_mut(),
                ptr::null_mut(),
                &mut len,
            );
            RegCloseKey(key);
            if rc == 0 && len > 2 {
                return true; // 非空版本号存在
            }
        }
    }
    false
}

fn main() {
    #[cfg(windows)]
    if !webview2_present() {
        eprintln!(
            "错误：未检测到 WebView2 运行时，Athanor Studio 无法启动图形界面。\n\
             建议：安装 Microsoft Edge WebView2 Runtime（常青版）后重试：\n\
             https://developer.microsoft.com/microsoft-edge/webview2/\n\
             企业离线环境可部署固定版本 WebView2。"
        );
        std::process::exit(2);
    }
    studio::run();
}
