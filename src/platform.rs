//! Plattformspezifische Helfer (Fensterebene, Mausposition).

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
    #[link(name = "CoreFoundation", kind = "framework")]
    unsafe extern "C" {
        fn CFRelease(cf: *const c_void);
    }
    unsafe {
        let ev = CGEventCreate(std::ptr::null());
        if ev.is_null() {
            return None;
        }
        let p = CGEventGetLocation(ev);
        CFRelease(ev);
        Some((p.x, p.y))
    }
}

#[cfg(not(target_os = "macos"))]
pub fn cursor_pos() -> Option<(f64, f64)> {
    None
}

#[cfg(target_os = "macos")]
#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGPreflightScreenCaptureAccess() -> bool;
    fn CGRequestScreenCaptureAccess() -> bool;
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
    let preflight = unsafe { CGPreflightScreenCaptureAccess() };
    let titles = list_windows().iter().any(|w| w.pid != own && !w.title.is_empty());
    (preflight, titles)
}

#[cfg(not(target_os = "macos"))]
pub fn screen_capture_diag() -> (bool, bool) {
    (true, true)
}

/// Zeigt den System-Dialog (nur beim ersten Mal) und trägt die App in die Liste ein.
#[cfg(target_os = "macos")]
pub fn request_screen_capture() {
    unsafe {
        CGRequestScreenCaptureAccess();
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
pub fn request_screen_capture() {}
#[cfg(not(target_os = "macos"))]
pub fn open_privacy_settings() {}

/// Sichtbares Fenster (globale logische Koordinaten), vorderstes zuerst.
pub struct OsWindow {
    pub pid: u32,
    pub app: String,
    pub title: String,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

/// Listet normale App-Fenster (Ebene 0) direkt über CGWindowListCopyWindowInfo.
#[cfg(target_os = "macos")]
pub fn list_windows() -> Vec<OsWindow> {
    use std::ffi::{c_char, c_void, CStr};
    type CFRef = *const c_void;
    #[repr(C)]
    #[derive(Default)]
    struct CGRect {
        x: f64,
        y: f64,
        w: f64,
        h: f64,
    }
    #[link(name = "CoreGraphics", kind = "framework")]
    unsafe extern "C" {
        fn CGWindowListCopyWindowInfo(option: u32, relative_to: u32) -> CFRef;
        fn CGRectMakeWithDictionaryRepresentation(dict: CFRef, rect: *mut CGRect) -> bool;
        static kCGWindowLayer: CFRef;
        static kCGWindowBounds: CFRef;
        static kCGWindowOwnerPID: CFRef;
        static kCGWindowOwnerName: CFRef;
        static kCGWindowName: CFRef;
        static kCGWindowAlpha: CFRef;
    }
    #[link(name = "CoreFoundation", kind = "framework")]
    unsafe extern "C" {
        fn CFArrayGetCount(arr: CFRef) -> isize;
        fn CFArrayGetValueAtIndex(arr: CFRef, idx: isize) -> CFRef;
        fn CFDictionaryGetValue(dict: CFRef, key: CFRef) -> CFRef;
        fn CFNumberGetValue(num: CFRef, kind: isize, out: *mut c_void) -> bool;
        fn CFStringGetCString(s: CFRef, buf: *mut c_char, len: isize, enc: u32) -> bool;
        fn CFRelease(cf: CFRef);
    }
    const ON_SCREEN_ONLY: u32 = 1 << 0;
    const EXCLUDE_DESKTOP: u32 = 1 << 4;
    const NUM_F64: isize = 13;

    unsafe fn num(d: CFRef, k: CFRef) -> Option<f64> {
        let v = unsafe { CFDictionaryGetValue(d, k) };
        let mut out = 0f64;
        (!v.is_null() && unsafe { CFNumberGetValue(v, NUM_F64, &mut out as *mut f64 as *mut c_void) }).then_some(out)
    }
    unsafe fn string(d: CFRef, k: CFRef) -> String {
        let v = unsafe { CFDictionaryGetValue(d, k) };
        let mut buf = [0 as c_char; 512];
        if !v.is_null() && unsafe { CFStringGetCString(v, buf.as_mut_ptr(), buf.len() as isize, 0x0800_0100) } {
            unsafe { CStr::from_ptr(buf.as_ptr()) }.to_string_lossy().into_owned()
        } else {
            String::new()
        }
    }

    let mut out = vec![];
    unsafe {
        let arr = CGWindowListCopyWindowInfo(ON_SCREEN_ONLY | EXCLUDE_DESKTOP, 0);
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
                pid: num(d, kCGWindowOwnerPID).unwrap_or(0.0) as u32,
                app: string(d, kCGWindowOwnerName),
                title: string(d, kCGWindowName),
                x: r.x as f32,
                y: r.y as f32,
                w: r.w as f32,
                h: r.h as f32,
            });
        }
        CFRelease(arr);
    }
    out
}

#[cfg(not(target_os = "macos"))]
pub fn list_windows() -> Vec<OsWindow> {
    xcap::Window::all()
        .unwrap_or_default()
        .into_iter()
        .filter(|w| !w.is_minimized().unwrap_or(false))
        .filter_map(|w| {
            Some(OsWindow {
                pid: w.pid().ok()?,
                app: w.app_name().unwrap_or_default(),
                title: w.title().unwrap_or_default(),
                x: w.x().ok()? as f32,
                y: w.y().ok()? as f32,
                w: w.width().ok()? as f32,
                h: w.height().ok()? as f32,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    #[test]
    fn lists_windows() {
        for w in super::list_windows() {
            println!("{:>6} {:<24} {:?} {}x{} @ {},{}", w.pid, w.app, w.title, w.w, w.h, w.x, w.y);
        }
    }
}

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
