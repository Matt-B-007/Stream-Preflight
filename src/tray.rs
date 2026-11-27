//! The system tray icon: shows what Preflight is doing (idle, watching,
//! testing, live, or needs you), opens the window, and shows notifications.
//!
//! Linux: a StatusNotifierItem over D-Bus (KDE Plasma, and GNOME with the
//! AppIndicator extension), notifications via org.freedesktop.Notifications.
//! Windows: Shell_NotifyIcon on its own thread, notifications as tray balloons.
//!
//! Nothing here reads anything; it only shows state the app already has.

use std::sync::mpsc;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum TrayState {
    /// Nothing going on.
    #[default]
    Idle,
    /// Waiting for OBS to start recording or streaming.
    Watching,
    /// A test is measuring.
    Testing,
    /// Streaming; post-flight is recording numbers.
    Live,
    /// Connected and not live, with fixes left from the last check.
    NeedsFixes,
}

impl TrayState {
    pub fn word(self) -> &'static str {
        match self {
            TrayState::Idle => "Idle",
            TrayState::Watching => "Watching OBS",
            TrayState::Testing => "Testing",
            TrayState::Live => "Live",
            TrayState::NeedsFixes => "Things to fix",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct TrayView {
    pub state: TrayState,
    /// Something needs the user (flag still on, a test finished...).
    pub attention: bool,
    /// One line for the menu and tooltip, e.g. "Live · 1:12:04".
    pub status: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrayCmd {
    Open,
    Quit,
}

/// RGBA pixels of the tray icon for a state, `size` × `size`.
pub fn icon_rgba(view: &TrayView, size: u32) -> Vec<u8> {
    let (r, g, b) = match view.state {
        TrayState::Idle => (0x9A, 0xA4, 0xB2),
        TrayState::Watching => (0xC6, 0xF3, 0x6B),
        TrayState::Testing => (0x5E, 0xF0, 0xB5),
        TrayState::Live => (0xF0, 0x5A, 0x5A),
        TrayState::NeedsFixes => (0xF5, 0xB9, 0x42),
    };
    let n = size as usize;
    let mut out = vec![0u8; n * n * 4];
    let s = size as f32 / 128.0;
    let plane = [(40.0, 62.0), (100.0, 32.0), (80.0, 78.0), (70.0, 64.0)];
    let check = [(30.0, 92.0), (54.0, 114.0), (104.0, 66.0)];
    const SS: usize = 4; // supersampling per axis
    for py in 0..n {
        for px in 0..n {
            let (mut bg, mut fg, mut dot, mut ring) = (0u32, 0u32, 0u32, 0u32);
            for sy in 0..SS {
                for sx in 0..SS {
                    let x = (px as f32 + (sx as f32 + 0.5) / SS as f32) / s;
                    let y = (py as f32 + (sy as f32 + 0.5) / SS as f32) / s;
                    if in_round_rect(x, y, 0.0, 0.0, 128.0, 128.0, 28.0) {
                        bg += 1;
                    }
                    if in_poly(x, y, &plane) || near_polyline(x, y, &check, 6.5) {
                        fg += 1;
                    }
                    if view.attention {
                        let d = ((x - 106.0).powi(2) + (y - 22.0).powi(2)).sqrt();
                        if d <= 20.0 {
                            dot += 1;
                        } else if d <= 27.0 {
                            ring += 1;
                        }
                    }
                }
            }
            let tot = (SS * SS) as f32;
            let i = (py * n + px) * 4;
            // Layer: background, then the logo, then the attention dot with a dark ring.
            let mut c = [0x1C as f32, 0x22 as f32, 0x2B as f32, bg as f32 / tot * 255.0];
            let blend = |c: &mut [f32; 4], col: (f32, f32, f32), a: f32| {
                if a <= 0.0 {
                    return;
                }
                c[0] = c[0] * (1.0 - a) + col.0 * a;
                c[1] = c[1] * (1.0 - a) + col.1 * a;
                c[2] = c[2] * (1.0 - a) + col.2 * a;
                c[3] = c[3].max(a * 255.0);
            };
            blend(&mut c, (r as f32, g as f32, b as f32), fg as f32 / tot);
            blend(&mut c, (0x0F as f32, 0x12 as f32, 0x17 as f32), ring as f32 / tot);
            blend(&mut c, (0xFF as f32, 0xFF as f32, 0xFF as f32), dot as f32 / tot);
            out[i] = c[0] as u8;
            out[i + 1] = c[1] as u8;
            out[i + 2] = c[2] as u8;
            out[i + 3] = c[3] as u8;
        }
    }
    out
}

fn in_round_rect(x: f32, y: f32, x0: f32, y0: f32, x1: f32, y1: f32, r: f32) -> bool {
    if x < x0 || y < y0 || x > x1 || y > y1 {
        return false;
    }
    let cx = x.clamp(x0 + r, x1 - r);
    let cy = y.clamp(y0 + r, y1 - r);
    (x - cx).powi(2) + (y - cy).powi(2) <= r * r
}

fn in_poly(x: f32, y: f32, p: &[(f32, f32)]) -> bool {
    let mut inside = false;
    let mut j = p.len() - 1;
    for i in 0..p.len() {
        let (xi, yi) = p[i];
        let (xj, yj) = p[j];
        if (yi > y) != (yj > y) && x < (xj - xi) * (y - yi) / (yj - yi) + xi {
            inside = !inside;
        }
        j = i;
    }
    inside
}

fn near_polyline(x: f32, y: f32, p: &[(f32, f32)], w: f32) -> bool {
    p.windows(2).any(|s| {
        let (ax, ay) = s[0];
        let (bx, by) = s[1];
        let (dx, dy) = (bx - ax, by - ay);
        let t = (((x - ax) * dx + (y - ay) * dy) / (dx * dx + dy * dy)).clamp(0.0, 1.0);
        let (qx, qy) = (ax + t * dx, ay + t * dy);
        (x - qx).powi(2) + (y - qy).powi(2) <= w * w
    })
}

/// The running tray icon. Dropping it removes the icon.
pub struct Tray {
    rx: mpsc::Receiver<TrayCmd>,
    last: TrayView,
    imp: imp::Imp,
}

impl Tray {
    /// Starts the tray icon. `None` when the desktop has no tray.
    pub fn start(ctx: eframe::egui::Context) -> Option<Tray> {
        let (tx, rx) = mpsc::channel();
        let view = TrayView { status: "Idle".into(), ..Default::default() };
        match imp::Imp::start(tx, ctx, view.clone()) {
            Ok(imp) => {
                log::info!("Tray icon started");
                Some(Tray { rx, last: view, imp })
            }
            Err(e) => {
                log::info!("No system tray: {e}");
                None
            }
        }
    }

    /// Updates the icon, tooltip and menu if anything changed.
    pub fn set(&mut self, view: TrayView) {
        if view != self.last {
            if view.state != self.last.state || view.attention != self.last.attention {
                log::info!("Tray: {}{}", view.state.word(), if view.attention { " · needs you" } else { "" });
            }
            self.imp.set(&view);
            self.last = view;
        }
    }

    pub fn last_view(&self) -> &TrayView {
        &self.last
    }

    /// A desktop notification. Text only, no names or keys.
    pub fn notify(&self, title: &str, body: &str) {
        log::info!("Notification: {title}");
        self.imp.notify(title, body);
    }

    pub fn poll(&self) -> Option<TrayCmd> {
        self.rx.try_recv().ok()
    }

    /// Show/hide the main window directly (Windows), where egui can't while hidden.
    #[allow(unused_variables)]
    pub fn set_window(&mut self, hwnd: isize) {
        #[cfg(windows)]
        self.imp.set_window(hwnd);
    }

    #[allow(unused_variables)]
    pub fn show_window(&self, show: bool) -> bool {
        #[cfg(windows)]
        {
            return self.imp.show_window(show);
        }
        #[allow(unreachable_code)]
        false
    }
}

// ---------------------------------------------------------------------------
// Linux: StatusNotifierItem

#[cfg(target_os = "linux")]
mod imp {
    use super::{TrayCmd, TrayView, icon_rgba};
    use ksni::blocking::TrayMethods;
    use std::sync::mpsc::Sender;

    struct Sni {
        view: TrayView,
        tx: Sender<TrayCmd>,
        ctx: eframe::egui::Context,
    }

    impl Sni {
        fn send(&self, c: TrayCmd) {
            let _ = self.tx.send(c);
            self.ctx.request_repaint();
        }
    }

    fn to_argb(rgba: &[u8]) -> Vec<u8> {
        rgba.chunks(4).flat_map(|p| [p[3], p[0], p[1], p[2]]).collect()
    }

    impl ksni::Tray for Sni {
        fn id(&self) -> String {
            "stream-preflight".into()
        }
        fn title(&self) -> String {
            "Stream Preflight".into()
        }
        fn category(&self) -> ksni::Category {
            ksni::Category::ApplicationStatus
        }
        fn status(&self) -> ksni::Status {
            if self.view.attention { ksni::Status::NeedsAttention } else { ksni::Status::Active }
        }
        fn icon_pixmap(&self) -> Vec<ksni::Icon> {
            [22u32, 32, 64].iter().map(|&s| ksni::Icon { width: s as i32, height: s as i32, data: to_argb(&icon_rgba(&self.view, s)) }).collect()
        }
        fn attention_icon_pixmap(&self) -> Vec<ksni::Icon> {
            self.icon_pixmap()
        }
        fn tool_tip(&self) -> ksni::ToolTip {
            ksni::ToolTip { title: "Stream Preflight".into(), description: self.view.status.clone(), ..Default::default() }
        }
        fn activate(&mut self, _x: i32, _y: i32) {
            self.send(TrayCmd::Open);
        }
        fn menu(&self) -> Vec<ksni::MenuItem<Self>> {
            use ksni::menu::StandardItem;
            vec![
                StandardItem { label: self.view.status.clone(), enabled: false, ..Default::default() }.into(),
                ksni::MenuItem::Separator,
                StandardItem { label: "Open Preflight".into(), activate: Box::new(|t: &mut Self| t.send(TrayCmd::Open)), ..Default::default() }.into(),
                StandardItem { label: "Quit".into(), activate: Box::new(|t: &mut Self| t.send(TrayCmd::Quit)), ..Default::default() }.into(),
            ]
        }
        fn watcher_offline(&self, reason: ksni::OfflineReason) -> bool {
            log::info!("Tray host went away: {reason:?}");
            true
        }
    }

    pub struct Imp {
        handle: ksni::blocking::Handle<Sni>,
    }

    impl Imp {
        pub fn start(tx: Sender<TrayCmd>, ctx: eframe::egui::Context, view: TrayView) -> Result<Imp, String> {
            let handle = Sni { view, tx, ctx }.spawn().map_err(|e| e.to_string())?;
            Ok(Imp { handle })
        }
        pub fn set(&self, v: &TrayView) {
            let v = v.clone();
            self.handle.update(move |t| t.view = v);
        }
        pub fn notify(&self, title: &str, body: &str) {
            let (title, body) = (title.to_string(), body.to_string());
            std::thread::spawn(move || {
                let r = (|| -> zbus::Result<()> {
                    let conn = zbus::blocking::Connection::session()?;
                    let hints: std::collections::HashMap<&str, zbus::zvariant::Value> = std::collections::HashMap::new();
                    conn.call_method(
                        Some("org.freedesktop.Notifications"),
                        "/org/freedesktop/Notifications",
                        Some("org.freedesktop.Notifications"),
                        "Notify",
                        &("Stream Preflight", 0u32, "", title.as_str(), body.as_str(), Vec::<&str>::new(), hints, 8000i32),
                    )?;
                    Ok(())
                })();
                if let Err(e) = r {
                    log::info!("Notification not shown: {e}");
                }
            });
        }
    }

    impl Drop for Imp {
        fn drop(&mut self) {
            let _ = self.handle.shutdown();
        }
    }
}

// ---------------------------------------------------------------------------
// Windows: Shell_NotifyIcon

#[cfg(windows)]
mod imp {
    use super::{TrayCmd, TrayView, icon_rgba};
    use std::sync::mpsc::Sender;
    use std::sync::{Mutex, OnceLock};
    use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM};
    use windows::Win32::Graphics::Gdi::{CreateBitmap, DeleteObject};
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::UI::Shell::{NIF_ICON, NIF_INFO, NIF_MESSAGE, NIF_TIP, NIIF_INFO, NIM_ADD, NIM_DELETE, NIM_MODIFY, NOTIFYICONDATAW, Shell_NotifyIconW};
    use windows::Win32::UI::WindowsAndMessaging::*;
    use windows::core::{PCWSTR, w};

    const WM_TRAY: u32 = WM_APP + 1;
    const WM_UPDATE: u32 = WM_APP + 2;
    const WM_NOTIFY_MSG: u32 = WM_APP + 3;
    const ID_OPEN: usize = 1;
    const ID_QUIT: usize = 2;

    struct Shared {
        tx: Sender<TrayCmd>,
        ctx: eframe::egui::Context,
        view: TrayView,
        note: Option<(String, String)>,
        hwnd: isize,
        main: isize,
    }

    static SHARED: OnceLock<Mutex<Shared>> = OnceLock::new();

    fn shared() -> std::sync::MutexGuard<'static, Shared> {
        SHARED.get().expect("tray").lock().unwrap_or_else(|e| e.into_inner())
    }

    fn wide(s: &str, out: &mut [u16]) {
        let v: Vec<u16> = s.encode_utf16().take(out.len() - 1).collect();
        out[..v.len()].copy_from_slice(&v);
        out[v.len()] = 0;
    }

    unsafe fn make_icon(view: &TrayView) -> HICON {
        let size = 32;
        let rgba = icon_rgba(view, size);
        let bgra: Vec<u8> = rgba.chunks(4).flat_map(|p| [p[2], p[1], p[0], p[3]]).collect();
        unsafe {
            let color = CreateBitmap(size as i32, size as i32, 1, 32, Some(bgra.as_ptr() as *const _));
            let mask_bits = vec![0u8; (size * size / 8) as usize];
            let mask = CreateBitmap(size as i32, size as i32, 1, 1, Some(mask_bits.as_ptr() as *const _));
            let info = ICONINFO { fIcon: true.into(), xHotspot: 0, yHotspot: 0, hbmMask: mask, hbmColor: color };
            let icon = CreateIconIndirect(&info).unwrap_or_default();
            let _ = DeleteObject(color.into());
            let _ = DeleteObject(mask.into());
            icon
        }
    }

    unsafe fn data(hwnd: HWND, view: &TrayView) -> NOTIFYICONDATAW {
        let mut d = NOTIFYICONDATAW { cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32, hWnd: hwnd, uID: 1, ..Default::default() };
        d.uFlags = NIF_ICON | NIF_MESSAGE | NIF_TIP;
        d.uCallbackMessage = WM_TRAY;
        d.hIcon = unsafe { make_icon(view) };
        wide(&format!("Stream Preflight · {}", view.status), &mut d.szTip);
        d
    }

    unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
        unsafe {
            match msg {
                WM_TRAY => {
                    let ev = (lp.0 & 0xFFFF) as u32;
                    if ev == WM_LBUTTONUP {
                        send(TrayCmd::Open);
                    } else if ev == WM_RBUTTONUP || ev == WM_CONTEXTMENU {
                        let status = shared().view.status.clone();
                        if let Ok(menu) = CreatePopupMenu() {
                            let mut st: Vec<u16> = status.encode_utf16().chain([0]).collect();
                            let _ = AppendMenuW(menu, MF_STRING | MF_GRAYED, 0, PCWSTR(st.as_mut_ptr()));
                            let _ = AppendMenuW(menu, MF_SEPARATOR, 0, PCWSTR::null());
                            let _ = AppendMenuW(menu, MF_STRING, ID_OPEN, w!("Open Preflight"));
                            let _ = AppendMenuW(menu, MF_STRING, ID_QUIT, w!("Quit"));
                            let mut pt = POINT::default();
                            let _ = GetCursorPos(&mut pt);
                            let _ = SetForegroundWindow(hwnd);
                            let cmd = TrackPopupMenu(menu, TPM_RETURNCMD | TPM_RIGHTBUTTON, pt.x, pt.y, Some(0), hwnd, None);
                            let _ = DestroyMenu(menu);
                            match cmd.0 as usize {
                                ID_OPEN => send(TrayCmd::Open),
                                ID_QUIT => send(TrayCmd::Quit),
                                _ => {}
                            }
                        }
                    }
                    LRESULT(0)
                }
                WM_UPDATE => {
                    let view = shared().view.clone();
                    let d = data(hwnd, &view);
                    let _ = Shell_NotifyIconW(NIM_MODIFY, &d);
                    let _ = DestroyIcon(d.hIcon);
                    LRESULT(0)
                }
                WM_NOTIFY_MSG => {
                    let (view, note) = {
                        let mut s = shared();
                        (s.view.clone(), s.note.take())
                    };
                    if let Some((t, b)) = note {
                        let mut d = data(hwnd, &view);
                        d.uFlags |= NIF_INFO;
                        d.dwInfoFlags = NIIF_INFO;
                        wide(&t, &mut d.szInfoTitle);
                        wide(&b, &mut d.szInfo);
                        let _ = Shell_NotifyIconW(NIM_MODIFY, &d);
                        let _ = DestroyIcon(d.hIcon);
                    }
                    LRESULT(0)
                }
                WM_DESTROY => {
                    let d = NOTIFYICONDATAW { cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32, hWnd: hwnd, uID: 1, ..Default::default() };
                    let _ = Shell_NotifyIconW(NIM_DELETE, &d);
                    PostQuitMessage(0);
                    LRESULT(0)
                }
                _ => DefWindowProcW(hwnd, msg, wp, lp),
            }
        }
    }

    fn send(c: TrayCmd) {
        let s = shared();
        let _ = s.tx.send(c);
        s.ctx.request_repaint();
    }

    pub struct Imp;

    impl Imp {
        pub fn start(tx: Sender<TrayCmd>, ctx: eframe::egui::Context, view: TrayView) -> Result<Imp, String> {
            if SHARED.set(Mutex::new(Shared { tx, ctx, view, note: None, hwnd: 0, main: 0 })).is_err() {
                return Err("tray already started".into());
            }
            let (ok_tx, ok_rx) = std::sync::mpsc::channel();
            std::thread::Builder::new()
                .name("tray".into())
                .spawn(move || unsafe {
                    let inst = GetModuleHandleW(None).unwrap_or_default();
                    let class = w!("PreFlightTray");
                    let wc = WNDCLASSW { lpfnWndProc: Some(wndproc), hInstance: inst.into(), lpszClassName: class, ..Default::default() };
                    RegisterClassW(&wc);
                    let hwnd = match CreateWindowExW(WINDOW_EX_STYLE(0), class, w!("Preflight tray"), WS_OVERLAPPED, 0, 0, 0, 0, None, None, Some(inst.into()), None) {
                        Ok(h) => h,
                        Err(e) => {
                            let _ = ok_tx.send(Err(e.to_string()));
                            return;
                        }
                    };
                    shared().hwnd = hwnd.0 as isize;
                    let view = shared().view.clone();
                    let d = data(hwnd, &view);
                    let added = Shell_NotifyIconW(NIM_ADD, &d).as_bool();
                    let _ = DestroyIcon(d.hIcon);
                    let _ = ok_tx.send(if added { Ok(()) } else { Err("Shell_NotifyIcon failed".into()) });
                    let mut msg = MSG::default();
                    while GetMessageW(&mut msg, None, 0, 0).as_bool() {
                        let _ = TranslateMessage(&msg);
                        DispatchMessageW(&msg);
                    }
                })
                .map_err(|e| e.to_string())?;
            ok_rx.recv().map_err(|e| e.to_string())??;
            Ok(Imp)
        }
        fn post(&self, m: u32) {
            let h = shared().hwnd;
            unsafe {
                let _ = PostMessageW(Some(HWND(h as *mut _)), m, WPARAM(0), LPARAM(0));
            }
        }
        pub fn set(&self, v: &TrayView) {
            shared().view = v.clone();
            self.post(WM_UPDATE);
        }
        pub fn notify(&self, title: &str, body: &str) {
            shared().note = Some((title.to_string(), body.to_string()));
            self.post(WM_NOTIFY_MSG);
        }
        pub fn set_window(&self, hwnd: isize) {
            shared().main = hwnd;
        }
        pub fn show_window(&self, show: bool) -> bool {
            let main = shared().main;
            if main == 0 {
                return false;
            }
            unsafe {
                let h = HWND(main as *mut _);
                let _ = ShowWindow(h, if show { SW_SHOW } else { SW_HIDE });
                if show {
                    let _ = ShowWindow(h, SW_RESTORE);
                    let _ = SetForegroundWindow(h);
                }
            }
            true
        }
    }

    impl Drop for Imp {
        fn drop(&mut self) {
            self.post(WM_CLOSE);
        }
    }
}

#[cfg(not(any(windows, target_os = "linux")))]
mod imp {
    use super::{TrayCmd, TrayView};
    pub struct Imp;
    impl Imp {
        pub fn start(_: std::sync::mpsc::Sender<TrayCmd>, _: eframe::egui::Context, _: TrayView) -> Result<Imp, String> {
            Err("no tray on this system".into())
        }
        pub fn set(&self, _: &TrayView) {}
        pub fn notify(&self, _: &str, _: &str) {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn icons_differ_by_state_and_attention() {
        let a = icon_rgba(&TrayView { state: TrayState::Idle, ..Default::default() }, 32);
        let b = icon_rgba(&TrayView { state: TrayState::Live, ..Default::default() }, 32);
        let c = icon_rgba(&TrayView { state: TrayState::Live, attention: true, ..Default::default() }, 32);
        assert_eq!(a.len(), 32 * 32 * 4);
        assert_ne!(a, b);
        assert_ne!(b, c);
        // Corners are transparent (rounded), the middle is opaque.
        assert_eq!(a[3], 0);
        assert_eq!(a[(16 * 32 + 16) * 4 + 3], 255);
    }
}

#[cfg(test)]
mod preview_icons {
    /// Writes the icons to /tmp when PF_ICON_DUMP is set (for looking at them).
    #[test]
    fn dump_icons() {
        if std::env::var("PF_ICON_DUMP").is_err() {
            return;
        }
        use super::*;
        for (n, st, att) in [("idle", TrayState::Idle, false), ("watching", TrayState::Watching, false), ("testing", TrayState::Testing, false), ("live", TrayState::Live, false), ("live_att", TrayState::Live, true), ("watch_att", TrayState::Watching, true)] {
            let px = icon_rgba(&TrayView { state: st, attention: att, status: String::new() }, 64);
            image::save_buffer(format!("/tmp/tray_{n}.png"), &px, 64, 64, image::ExtendedColorType::Rgba8).unwrap();
        }
    }
}
