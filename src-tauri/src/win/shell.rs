//! Shell integration: app discovery (AppsFolder), icons, launching and wallpaper.

use crate::error::{AppError, AppResult};
use std::path::Path;
use windows::core::{Interface, HSTRING, PCWSTR, PWSTR};
use windows::Win32::Foundation::SIZE;
use windows::Win32::Graphics::Gdi::{
    DeleteObject, GetDC, GetDIBits, GetObjectW, ReleaseDC, BITMAP, BITMAPINFO, BITMAPINFOHEADER, BI_RGB,
    DIB_RGB_COLORS, HBITMAP, HGDIOBJ,
};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize, CLSCTX_ALL, COINIT_APARTMENTTHREADED,
    COINIT_DISABLE_OLE1DDE,
};
use windows::Win32::UI::Shell::{
    BHID_EnumItems, DesktopWallpaper, FOLDERID_AppsFolder, IDesktopWallpaper, IEnumShellItems, IShellItem,
    IShellItemImageFactory, SHCreateItemFromParsingName, SHGetKnownFolderItem, ShellExecuteExW, DESKTOP_WALLPAPER_POSITION,
    DWPOS_CENTER, DWPOS_FILL, DWPOS_FIT, DWPOS_SPAN, DWPOS_STRETCH, DWPOS_TILE, KF_FLAG_DEFAULT, SEE_MASK_FLAG_NO_UI,
    SEE_MASK_NOASYNC, SHELLEXECUTEINFOW, SIGDN_NORMALDISPLAY, SIGDN_PARENTRELATIVEPARSING, SIIGBF_BIGGERSIZEOK,
    SIIGBF_ICONONLY,
};
use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

/// RAII COM initialisation for the current thread (STA, as the shell expects).
pub struct ComGuard(bool);
impl ComGuard {
    pub fn sta() -> Self {
        let hr = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE) };
        ComGuard(hr.is_ok())
    }
}
impl Drop for ComGuard {
    fn drop(&mut self) {
        if self.0 {
            unsafe { CoUninitialize() };
        }
    }
}

#[derive(Debug, Clone)]
pub struct ShellApp {
    pub name: String,
    pub parsing_name: String,
    pub icon_png: Option<Vec<u8>>,
}

fn display_name(item: &IShellItem, kind: windows::Win32::UI::Shell::SIGDN) -> Option<String> {
    unsafe {
        let p: PWSTR = item.GetDisplayName(kind).ok()?;
        let s = p.to_string().ok();
        CoTaskMemFree(Some(p.0 as *const core::ffi::c_void));
        s
    }
}

/// Entries in the Start menu that are not useful launch targets.
fn is_noise(name: &str, parsing: &str) -> bool {
    let n = name.to_lowercase();
    let p = parsing.to_lowercase();
    const NOISE_WORDS: [&str; 6] = ["uninstall", "readme", "read me", "release notes", "license", "documentation"];
    const NOISE_EXT: [&str; 9] = [".txt", ".chm", ".pdf", ".htm", ".html", ".url", ".rtf", ".md", ".hlp"];
    NOISE_WORDS.iter().any(|w| n.contains(w)) || NOISE_EXT.iter().any(|e| p.ends_with(e)) || name.trim().is_empty()
}

/// Enumerates everything the Start menu lists (desktop and Store apps). Must run on an STA thread.
pub fn enumerate_apps(icon_size: i32) -> AppResult<Vec<ShellApp>> {
    let mut apps = Vec::new();
    unsafe {
        let folder: IShellItem = SHGetKnownFolderItem(&FOLDERID_AppsFolder, KF_FLAG_DEFAULT, None)?;
        let enumerator: IEnumShellItems = folder.BindToHandler(None, &BHID_EnumItems)?;
        loop {
            let mut items: [Option<IShellItem>; 1] = [None];
            let mut fetched = 0u32;
            if enumerator.Next(&mut items, Some(&mut fetched)).is_err() || fetched == 0 {
                break;
            }
            let Some(item) = items[0].take() else { break };
            let (Some(name), Some(parsing)) =
                (display_name(&item, SIGDN_NORMALDISPLAY), display_name(&item, SIGDN_PARENTRELATIVEPARSING))
            else {
                continue;
            };
            if is_noise(&name, &parsing) || parsing.len() > 1024 {
                continue;
            }
            let icon_png = item_icon_png(&item, icon_size);
            apps.push(ShellApp { name, parsing_name: parsing, icon_png });
        }
    }
    apps.sort_by_key(|a| a.name.to_lowercase());
    apps.dedup_by(|a, b| a.parsing_name == b.parsing_name);
    Ok(apps)
}

/// Icon for a file path (exe / lnk). Must run on an STA thread.
pub fn file_icon_png(path: &Path, icon_size: i32) -> Option<Vec<u8>> {
    unsafe {
        let item: IShellItem = SHCreateItemFromParsingName(&HSTRING::from(path.as_os_str()), None).ok()?;
        item_icon_png(&item, icon_size)
    }
}

/// Display name the shell uses for a file (e.g. a shortcut's title).
pub fn file_display_name(path: &Path) -> Option<String> {
    unsafe {
        let item: IShellItem = SHCreateItemFromParsingName(&HSTRING::from(path.as_os_str()), None).ok()?;
        display_name(&item, SIGDN_NORMALDISPLAY)
    }
}

fn item_icon_png(item: &IShellItem, size: i32) -> Option<Vec<u8>> {
    unsafe {
        let factory: IShellItemImageFactory = item.cast().ok()?;
        let hbmp = factory.GetImage(SIZE { cx: size, cy: size }, SIIGBF_ICONONLY | SIIGBF_BIGGERSIZEOK).ok()?;
        let png = hbitmap_to_png(hbmp);
        let _ = DeleteObject(HGDIOBJ(hbmp.0));
        png
    }
}

unsafe fn hbitmap_to_png(hbmp: HBITMAP) -> Option<Vec<u8>> {
    let mut bm = BITMAP::default();
    if GetObjectW(HGDIOBJ(hbmp.0), std::mem::size_of::<BITMAP>() as i32, Some(&mut bm as *mut _ as *mut _)) == 0 {
        return None;
    }
    let (w, h) = (bm.bmWidth, bm.bmHeight.abs());
    if w <= 0 || h <= 0 || w > 512 || h > 512 {
        return None;
    }
    let mut info = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: w,
            biHeight: -h, // top-down
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut buf = vec![0u8; (w * h * 4) as usize];
    let hdc = GetDC(None);
    let lines = GetDIBits(hdc, hbmp, 0, h as u32, Some(buf.as_mut_ptr() as *mut _), &mut info, DIB_RGB_COLORS);
    ReleaseDC(None, hdc);
    if lines == 0 {
        return None;
    }
    // BGRA (premultiplied) -> RGBA (straight alpha).
    let has_alpha = buf.chunks_exact(4).any(|px| px[3] != 0);
    for px in buf.chunks_exact_mut(4) {
        let (b, g, r, a) = (px[0], px[1], px[2], px[3]);
        if !has_alpha {
            px.copy_from_slice(&[r, g, b, 255]);
        } else if a == 0 {
            px.copy_from_slice(&[0, 0, 0, 0]);
        } else {
            let un = |c: u8| ((c as u32 * 255 + a as u32 / 2) / a as u32).min(255) as u8;
            px.copy_from_slice(&[un(r), un(g), un(b), a]);
        }
    }
    let img = image::RgbaImage::from_raw(w as u32, h as u32, buf)?;
    let mut out = std::io::Cursor::new(Vec::new());
    img.write_to(&mut out, image::ImageFormat::Png).ok()?;
    Some(out.into_inner())
}

/// Opens a shell target (file path or `shell:AppsFolder\<id>`) with the default verb.
/// The target is passed as a file, never as a command line, so no arguments can be injected.
pub fn shell_open(target: &str, working_dir: Option<&Path>) -> AppResult<()> {
    let file = HSTRING::from(target);
    let dir = working_dir.map(|d| HSTRING::from(d.as_os_str()));
    let mut info = SHELLEXECUTEINFOW {
        cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: SEE_MASK_NOASYNC | SEE_MASK_FLAG_NO_UI,
        lpVerb: PCWSTR::null(),
        lpFile: PCWSTR(file.as_ptr()),
        lpDirectory: dir.as_ref().map(|d| PCWSTR(d.as_ptr())).unwrap_or(PCWSTR::null()),
        nShow: SW_SHOWNORMAL.0,
        ..Default::default()
    };
    unsafe { ShellExecuteExW(&mut info) }
        .map_err(|e| AppError::new("launch_failed", "The app could not be opened.").with_detail(e.message()))
}

pub fn apply_wallpaper(path: &Path, fit: crate::settings::WallpaperFit) -> AppResult<()> {
    use crate::settings::WallpaperFit as F;
    let position: DESKTOP_WALLPAPER_POSITION = match fit {
        F::Fill => DWPOS_FILL,
        F::Fit => DWPOS_FIT,
        F::Stretch => DWPOS_STRETCH,
        F::Center => DWPOS_CENTER,
        F::Tile => DWPOS_TILE,
        F::Span => DWPOS_SPAN,
    };
    let _com = ComGuard::sta();
    unsafe {
        let wallpaper: IDesktopWallpaper = CoCreateInstance(&DesktopWallpaper, None, CLSCTX_ALL)
            .map_err(|e| AppError::native("Windows did not allow changing the wallpaper.", e.message()))?;
        wallpaper.SetPosition(position)?;
        wallpaper
            .SetWallpaper(PCWSTR::null(), &HSTRING::from(path.as_os_str()))
            .map_err(|e| AppError::native("Windows did not allow changing the wallpaper.", e.message()))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn noise_filter() {
        assert!(is_noise("Uninstall Foo", "foo\\uninstall.exe"));
        assert!(is_noise("Foo Readme", "x"));
        assert!(is_noise("Foo Help", "C:\\foo\\help.chm"));
        assert!(!is_noise("Visual Studio Code", "Microsoft.VisualStudioCode"));
    }
}
