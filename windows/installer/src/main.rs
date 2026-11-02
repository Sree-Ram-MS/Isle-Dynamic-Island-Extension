// Isle for Windows installer. Per-user install, no admin rights needed:
//   %LOCALAPPDATA%\Programs\Isle\Isle.exe
//   Start menu shortcut "Isle"
//   Settings > Apps entry (uninstall runs "Isle.exe --uninstall")
//   optional: start Isle with Windows
// Running it again upgrades in place (closes the running Isle first).

#![windows_subsystem = "windows"]

use std::path::PathBuf;
use std::ptr::{null, null_mut};

use windows::core::{Interface, HSTRING};
use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, IPersistFile, CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED};
use windows::Win32::UI::Shell::{IShellLinkW, ShellLink};
use windows_sys::Win32::System::Registry::*;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

const ISLE: &[u8] = include_bytes!("../../target/x86_64-pc-windows-gnu/release/isle.exe");
const VERSION: &str = env!("CARGO_PKG_VERSION");
const UNINSTALL_KEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\Isle";
const RUN_KEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

unsafe fn msg(text: &str, flags: u32) -> i32 {
    MessageBoxW(null_mut(), wide(text).as_ptr(), wide(&format!("Isle {} Setup", VERSION)).as_ptr(), flags)
}

unsafe fn set_str(key: HKEY, name: &str, val: &str) {
    let v = wide(val);
    RegSetValueExW(key, wide(name).as_ptr(), 0, REG_SZ, v.as_ptr() as *const u8, (v.len() * 2) as u32);
}

unsafe fn set_dword(key: HKEY, name: &str, val: u32) {
    RegSetValueExW(key, wide(name).as_ptr(), 0, REG_DWORD, &val as *const u32 as *const u8, 4);
}

unsafe fn shortcut(target: &PathBuf, dir: &PathBuf, lnk: &PathBuf) -> windows::core::Result<()> {
    let link: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER)?;
    link.SetPath(&HSTRING::from(target.as_os_str()))?;
    link.SetWorkingDirectory(&HSTRING::from(dir.as_os_str()))?;
    link.SetDescription(&HSTRING::from("Isle - a living notch for Windows"))?;
    let file: IPersistFile = link.cast()?;
    file.Save(&HSTRING::from(lnk.as_os_str()), true)?;
    Ok(())
}

fn main() {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let ask = format!(
            "Install Isle {} for this account?\n\nIt goes to your user folder (no admin rights), gets a Start menu shortcut and appears in Settings > Apps, where you can uninstall it.",
            VERSION
        );
        if msg(&ask, MB_OKCANCEL | MB_ICONINFORMATION) != IDOK {
            return;
        }
        let autostart = msg("Start Isle automatically when you sign in to Windows?", MB_YESNO | MB_ICONQUESTION) == IDYES;

        let Some(local) = std::env::var_os("LOCALAPPDATA") else {
            msg("Could not find your user folder (LOCALAPPDATA).", MB_OK | MB_ICONERROR);
            return;
        };
        let dir = PathBuf::from(local).join("Programs").join("Isle");
        let exe = dir.join("Isle.exe");

        // close a running Isle so its file can be replaced
        let h = FindWindowW(wide("IsleNativeNotch").as_ptr(), null());
        if !h.is_null() {
            PostMessageW(h, WM_CLOSE, 0, 0);
            for _ in 0..40 {
                if IsWindow(h) == 0 {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
            std::thread::sleep(std::time::Duration::from_millis(300));
        }

        if std::fs::create_dir_all(&dir).is_err() || std::fs::write(&exe, ISLE).is_err() {
            msg(&format!("Could not write {}.\nIs Isle still running? Quit it from its tray icon and run Setup again.", exe.display()), MB_OK | MB_ICONERROR);
            return;
        }

        // Start menu shortcut
        if let Some(appdata) = std::env::var_os("APPDATA") {
            let lnk = PathBuf::from(appdata).join("Microsoft\\Windows\\Start Menu\\Programs\\Isle.lnk");
            let _ = shortcut(&exe, &dir, &lnk);
        }

        // Settings > Apps entry
        let mut key: HKEY = null_mut();
        if RegCreateKeyExW(HKEY_CURRENT_USER, wide(UNINSTALL_KEY).as_ptr(), 0, null(), 0, KEY_WRITE, null(), &mut key, null_mut()) == 0 {
            let q = format!("\"{}\"", exe.display());
            set_str(key, "DisplayName", "Isle");
            set_str(key, "DisplayVersion", VERSION);
            set_str(key, "Publisher", "Sree Ram M S");
            set_str(key, "DisplayIcon", &exe.display().to_string());
            set_str(key, "InstallLocation", &dir.display().to_string());
            set_str(key, "UninstallString", &format!("{} --uninstall", q));
            set_str(key, "URLInfoAbout", "https://github.com/Sree-Ram-MS/Isle-Dynamic-Island-Extension");
            set_dword(key, "NoModify", 1);
            set_dword(key, "NoRepair", 1);
            set_dword(key, "EstimatedSize", (ISLE.len() / 1024) as u32);
            RegCloseKey(key);
        }

        // start with Windows
        let mut run: HKEY = null_mut();
        if RegOpenKeyExW(HKEY_CURRENT_USER, wide(RUN_KEY).as_ptr(), 0, KEY_SET_VALUE, &mut run) == 0 {
            if autostart {
                set_str(run, "Isle", &format!("\"{}\"", exe.display()));
            } else {
                RegDeleteValueW(run, wide("Isle").as_ptr());
            }
            RegCloseKey(run);
        }

        let _ = std::process::Command::new(&exe).current_dir(&dir).spawn();
        msg("Isle is installed and running.\n\nHover the top centre of your screen to open it. Find it in the Start menu, or quit it from its tray icon.", MB_OK | MB_ICONINFORMATION);
    }
}
