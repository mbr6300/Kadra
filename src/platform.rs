//! Plattformspezifische Helfer (Fensterebene, Mausposition, Fensterliste).

use image::RgbaImage;

/// Mausposition in globalen logischen Koordinaten (Ursprung oben links).
#[cfg(target_os = "macos")]
pub fn cursor_pos() -> Option<(f64, f64)> {
    use std::ffi::c_void;
    #[repr(C)]
    struct CGPoint {
        x: f64,
        y: f64,
    }
    #[link(name = "CoreGraphics", kind = "framework")]
    unsafe extern "C" {
        fn CGEventCreate(source: *const c_void) -> *mut c_void;
        fn CGEventGetLocation(event: *mut c_void) -> CGPoint;
    }
    unsafe {
        let ev = CGEventCreate(std::ptr::null());
        if ev.is_null() {
            return None;
        }
        let p = CGEventGetLocation(ev);
        cg::CFRelease(ev);
        Some((p.x, p.y))
    }
}

#[cfg(not(target_os = "macos"))]
pub fn cursor_pos() -> Option<(f64, f64)> {
    None
}

/// Hat die App die Berechtigung „Bildschirmaufnahme“?
/// CGPreflight… cached das Ergebnis teils bis zum Neustart, daher zusätzlich prüfen,
/// ob Fenstertitel fremder Apps lesbar sind (geht nur mit Berechtigung).
#[cfg(target_os = "macos")]
pub fn screen_capture_allowed() -> bool {
    let (preflight, titles) = screen_capture_diag();
    preflight || titles
}

/// (CGPreflightScreenCaptureAccess, Fenstertitel fremder Apps sichtbar)
#[cfg(target_os = "macos")]
pub fn screen_capture_diag() -> (bool, bool) {
    let own = std::process::id();
    let preflight = unsafe { cg::CGPreflightScreenCaptureAccess() };
    let titles = list_windows(false).iter().any(|w| w.pid != own && !w.title.is_empty());
    (preflight, titles)
}

/// Zeigt den System-Dialog (nur beim ersten Mal) und trägt die App in die Liste ein.
#[cfg(target_os = "macos")]
pub fn request_screen_capture() {
    unsafe {
        cg::CGRequestScreenCaptureAccess();
    }
}

#[cfg(target_os = "macos")]
pub fn open_privacy_settings() {
    let _ = std::process::Command::new("open")
        .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture")
        .spawn();
}

#[cfg(not(target_os = "macos"))]
pub fn screen_capture_allowed() -> bool {
    true
}
#[cfg(not(target_os = "macos"))]
pub fn screen_capture_diag() -> (bool, bool) {
    (true, true)
}
#[cfg(not(target_os = "macos"))]
pub fn request_screen_capture() {}
#[cfg(not(target_os = "macos"))]
pub fn open_privacy_settings() {}

/// App-Fenster (globale logische Koordinaten).
pub struct OsWindow {
    pub id: u32,
    pub pid: u32,
    pub app: String,
    pub title: String,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    /// false = auf anderem Schreibtisch, minimiert oder ausgeblendet.
    pub on_screen: bool,
}

/// CoreGraphics/CoreFoundation-Anbindung.
#[cfg(target_os = "macos")]
mod cg {
    use std::ffi::{c_char, c_void, CStr};
    pub type CFRef = *const c_void;

    #[repr(C)]
    #[derive(Default, Clone, Copy)]
    pub struct CGRect {
        pub x: f64,
        pub y: f64,
        pub w: f64,
        pub h: f64,
    }

    #[link(name = "CoreGraphics", kind = "framework")]
    unsafe extern "C" {
        pub fn CGPreflightScreenCaptureAccess() -> bool;
        pub fn CGRequestScreenCaptureAccess() -> bool;
        pub fn CGWindowListCopyWindowInfo(option: u32, relative_to: u32) -> CFRef;
        pub fn CGRectMakeWithDictionaryRepresentation(dict: CFRef, rect: *mut CGRect) -> bool;
        pub fn CGWindowListCreateImage(bounds: CGRect, option: u32, window: u32, image_option: u32) -> CFRef;
        pub fn CGImageGetWidth(img: CFRef) -> usize;
        pub fn CGImageGetHeight(img: CFRef) -> usize;
        pub fn CGColorSpaceCreateDeviceRGB() -> CFRef;
        pub fn CGBitmapContextCreate(
            data: *mut c_void,
            w: usize,
            h: usize,
            bits: usize,
            row_bytes: usize,
            space: CFRef,
            info: u32,
        ) -> CFRef;
        pub fn CGContextDrawImage(ctx: CFRef, rect: CGRect, img: CFRef);
        pub static CGRectNull: CGRect;
        pub static kCGWindowNumber: CFRef;
        pub static kCGWindowLayer: CFRef;
        pub static kCGWindowBounds: CFRef;
        pub static kCGWindowOwnerPID: CFRef;
        pub static kCGWindowOwnerName: CFRef;
        pub static kCGWindowName: CFRef;
        pub static kCGWindowAlpha: CFRef;
        pub static kCGWindowIsOnscreen: CFRef;
    }

    #[link(name = "CoreFoundation", kind = "framework")]
    unsafe extern "C" {
        pub fn CFArrayGetCount(arr: CFRef) -> isize;
        pub fn CFArrayGetValueAtIndex(arr: CFRef, idx: isize) -> CFRef;
        pub fn CFDictionaryGetValue(dict: CFRef, key: CFRef) -> CFRef;
        pub fn CFNumberGetValue(num: CFRef, kind: isize, out: *mut c_void) -> bool;
        pub fn CFBooleanGetValue(b: CFRef) -> bool;
        pub fn CFStringGetCString(s: CFRef, buf: *mut c_char, len: isize, enc: u32) -> bool;
        pub fn CFRelease(cf: CFRef);
    }

    pub unsafe fn num(d: CFRef, k: CFRef) -> Option<f64> {
        let v = unsafe { CFDictionaryGetValue(d, k) };
        let mut out = 0f64;
        // 13 = kCFNumberDoubleType
        (!v.is_null() && unsafe { CFNumberGetValue(v, 13, &mut out as *mut f64 as *mut c_void) }).then_some(out)
    }

    pub unsafe fn boolean(d: CFRef, k: CFRef) -> bool {
        let v = unsafe { CFDictionaryGetValue(d, k) };
        !v.is_null() && unsafe { CFBooleanGetValue(v) }
    }

    pub unsafe fn string(d: CFRef, k: CFRef) -> String {
        let v = unsafe { CFDictionaryGetValue(d, k) };
        let mut buf = [0 as c_char; 512];
        // 0x0800_0100 = kCFStringEncodingUTF8
        if !v.is_null() && unsafe { CFStringGetCString(v, buf.as_mut_ptr(), buf.len() as isize, 0x0800_0100) } {
            unsafe { CStr::from_ptr(buf.as_ptr()) }.to_string_lossy().into_owned()
        } else {
            String::new()
        }
    }
}

/// Listet normale App-Fenster (Ebene 0), vorderstes zuerst.
/// `include_hidden` schließt Fenster anderer Schreibtische und minimierte ein.
#[cfg(target_os = "macos")]
pub fn list_windows(include_hidden: bool) -> Vec<OsWindow> {
    use cg::*;
    const ON_SCREEN_ONLY: u32 = 1 << 0;
    const EXCLUDE_DESKTOP: u32 = 1 << 4;
    let option = if include_hidden { EXCLUDE_DESKTOP } else { ON_SCREEN_ONLY | EXCLUDE_DESKTOP };

    let mut out = vec![];
    unsafe {
        let arr = CGWindowListCopyWindowInfo(option, 0);
        if arr.is_null() {
            return out;
        }
        for i in 0..CFArrayGetCount(arr) {
            let d = CFArrayGetValueAtIndex(arr, i);
            if num(d, kCGWindowLayer) != Some(0.0) || num(d, kCGWindowAlpha).is_some_and(|a| a <= 0.01) {
                continue;
            }
            let bounds = CFDictionaryGetValue(d, kCGWindowBounds);
            let mut r = CGRect::default();
            if bounds.is_null() || !CGRectMakeWithDictionaryRepresentation(bounds, &mut r) {
                continue;
            }
            out.push(OsWindow {
                id: num(d, kCGWindowNumber).unwrap_or(0.0) as u32,
                pid: num(d, kCGWindowOwnerPID).unwrap_or(0.0) as u32,
                app: string(d, kCGWindowOwnerName),
                title: string(d, kCGWindowName),
                x: r.x as f32,
                y: r.y as f32,
                w: r.w as f32,
                h: r.h as f32,
                on_screen: boolean(d, kCGWindowIsOnscreen),
            });
        }
        CFRelease(arr);
    }
    out
}

/// Nimmt ein einzelnes Fenster direkt auf – auch wenn es verdeckt ist
/// oder auf einem anderen Schreibtisch liegt. Minimierte Fenster liefern meist nichts.
#[cfg(target_os = "macos")]
pub fn capture_window(id: u32) -> Option<RgbaImage> {
    use cg::*;
    const INCLUDING_WINDOW: u32 = 1 << 3;
    const IGNORE_FRAMING: u32 = 1 << 0;
    const BEST_RESOLUTION: u32 = 1 << 3;
    unsafe {
        let img = CGWindowListCreateImage(CGRectNull, INCLUDING_WINDOW, id, IGNORE_FRAMING | BEST_RESOLUTION);
        if img.is_null() {
            return None;
        }
        let (w, h) = (CGImageGetWidth(img), CGImageGetHeight(img));
        if w < 2 || h < 2 {
            CFRelease(img);
            return None;
        }
        let mut buf = vec![0u8; w * h * 4];
        let space = CGColorSpaceCreateDeviceRGB();
        // kCGImageAlphaPremultipliedLast | kCGBitmapByteOrder32Big = RGBA
        let ctx = CGBitmapContextCreate(buf.as_mut_ptr().cast(), w, h, 8, w * 4, space, 1 | (4 << 12));
        if !ctx.is_null() {
            CGContextDrawImage(ctx, CGRect { x: 0.0, y: 0.0, w: w as f64, h: h as f64 }, img);
            CFRelease(ctx);
        }
        CFRelease(space);
        CFRelease(img);
        if ctx.is_null() || buf.chunks_exact(4).all(|p| p[3] == 0) {
            return None;
        }
        for px in buf.chunks_exact_mut(4) {
            let a = px[3] as u32;
            if a > 0 && a < 255 {
                for c in &mut px[..3] {
                    *c = ((*c as u32 * 255) / a).min(255) as u8;
                }
            }
        }
        RgbaImage::from_raw(w as u32, h as u32, buf)
    }
}

#[cfg(not(target_os = "macos"))]
pub fn list_windows(_include_hidden: bool) -> Vec<OsWindow> {
    xcap::Window::all()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|w| {
            Some(OsWindow {
                id: w.id().ok()?,
                pid: w.pid().ok()?,
                app: w.app_name().unwrap_or_default(),
                title: w.title().unwrap_or_default(),
                x: w.x().ok()? as f32,
                y: w.y().ok()? as f32,
                w: w.width().ok()? as f32,
                h: w.height().ok()? as f32,
                on_screen: !w.is_minimized().unwrap_or(false),
            })
        })
        .collect()
}

#[cfg(not(target_os = "macos"))]
pub fn capture_window(id: u32) -> Option<RgbaImage> {
    xcap::Window::all().ok()?.into_iter().find(|w| w.id().ok() == Some(id))?.capture_image().ok()
}

#[cfg(test)]
mod tests {
    #[test]
    fn lists_and_captures_windows() {
        for w in super::list_windows(true) {
            println!("{:>6} {:>6} {:<24} {:?} {}x{} @ {},{} on={}", w.id, w.pid, w.app, w.title, w.w, w.h, w.x, w.y, w.on_screen);
        }
        if let Some(w) = super::list_windows(false).first() {
            let img = super::capture_window(w.id);
            println!("capture {} -> {:?}", w.id, img.map(|i| i.dimensions()));
        }
    }
}

/// Dock-Symbol zeigen (normale App) oder ausblenden (läuft nur in der Menüleiste).
/// Beim Zeigen wird die App zusätzlich in den Vordergrund geholt.
#[cfg(target_os = "macos")]
pub fn set_dock_visible(visible: bool) {
    use objc2::runtime::AnyObject;
    use objc2::{class, msg_send};
    // NSApplicationActivationPolicyRegular = 0, Accessory = 1
    let policy: isize = if visible { 0 } else { 1 };
    unsafe {
        let app: *mut AnyObject = msg_send![class!(NSApplication), sharedApplication];
        if app.is_null() {
            return;
        }
        let _: bool = msg_send![app, setActivationPolicy: policy];
        if visible {
            let _: () = msg_send![app, activateIgnoringOtherApps: true];
        }
    }
}

/// App in den Vordergrund holen (z. B. für das Auswahl-Overlay aus dem Hintergrund).
#[cfg(target_os = "macos")]
pub fn activate_app() {
    use objc2::runtime::AnyObject;
    use objc2::{class, msg_send};
    unsafe {
        let app: *mut AnyObject = msg_send![class!(NSApplication), sharedApplication];
        if !app.is_null() {
            let _: () = msg_send![app, activateIgnoringOtherApps: true];
        }
    }
}

#[cfg(not(target_os = "macos"))]
pub fn set_dock_visible(_visible: bool) {}
#[cfg(not(target_os = "macos"))]
pub fn activate_app() {}

/// Hebt das Fenster über Menüleiste und Dock (für das Auswahl-Overlay).
#[cfg(target_os = "macos")]
pub fn set_overlay_level(frame: &eframe::Frame, overlay: bool) {
    use objc2::msg_send;
    use objc2::runtime::AnyObject;
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};

    let Ok(handle) = frame.window_handle() else { return };
    let RawWindowHandle::AppKit(h) = handle.as_raw() else { return };
    let view = h.ns_view.as_ptr() as *mut AnyObject;
    // NSScreenSaverWindowLevel = 1000, NSNormalWindowLevel = 0
    let level: isize = if overlay { 1000 } else { 0 };
    unsafe {
        let win: *mut AnyObject = msg_send![view, window];
        if !win.is_null() {
            let _: () = msg_send![win, setLevel: level];
            let _: () = msg_send![win, setHasShadow: !overlay];
        }
    }
}

#[cfg(not(target_os = "macos"))]
pub fn set_overlay_level(_frame: &eframe::Frame, _overlay: bool) {}
