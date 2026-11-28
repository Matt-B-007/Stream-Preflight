// SPDX-License-Identifier: GPL-3.0-or-later
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

/// What the icon looks like, as in the other apps.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Look {
    /// OBS not connected: greyed out.
    Disconnected,
    /// Ready: plain.
    Idle,
    /// Testing, watching for a test, or live: lit in the accent colour.
    Active,
}

impl TrayState {
    pub fn look(self) -> Look {
        match self {
            TrayState::Idle => Look::Disconnected,
            TrayState::Watching | TrayState::NeedsFixes => Look::Idle,
            TrayState::Testing | TrayState::Live => Look::Active,
        }
    }

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
    /// Something needs the user (a test finished, fixes to make...): amber dot.
    pub attention: bool,
    /// Something is wrong (a check failed, the test flag is on while live): red dot.
    pub problem: bool,
    /// One line for the menu and tooltip, e.g. "Live · 1:12:04".
    pub status: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrayCmd {
    Open,
    Quit,
}

/// RGBA pixels of the tray icon for a state, `size` × `size`. Transparent
/// background; the symbol is grey (disconnected), plain (idle) or lime
/// (active), with a thin dark edge so it shows on light and dark panels. An
/// amber dot means "needs attention", a red dot "problem", bottom right.
pub fn icon_rgba(view: &TrayView, size: u32) -> Vec<u8> {
    let sym = match view.state.look() {
        Look::Disconnected => (0x8A, 0x94, 0xA3),
        Look::Idle => (0xD5, 0xDB, 0xE4),
        Look::Active => (0xC6, 0xF3, 0x6B),
    };
    let dot_col = if view.problem {
        Some((0xF8, 0x71, 0x71))
    } else if view.attention || view.state == TrayState::NeedsFixes {
        Some((0xFB, 0xBF, 0x24))
    } else {
        None
    };
    let n = size as usize;
    let mut out = vec![0u8; n * n * 4];
    let s = size as f32 / 128.0;
    let plane = [(34.0, 58.0), (106.0, 22.0), (84.0, 76.0), (72.0, 62.0)];
    let check = [(22.0, 92.0), (48.0, 116.0), (102.0, 64.0)];
    const SS: usize = 4; // supersampling per axis
    let (dx, dy) = (100.0, 100.0);
    for py in 0..n {
        for px in 0..n {
            let (mut edge, mut fg, mut dot, mut ring) = (0u32, 0u32, 0u32, 0u32);
            for sy in 0..SS {
                for sx in 0..SS {
                    let x = (px as f32 + (sx as f32 + 0.5) / SS as f32) / s;
                    let y = (py as f32 + (sy as f32 + 0.5) / SS as f32) / s;
                    if in_poly(x, y, &plane) || near_polyline(x, y, &check, 8.0) {
                        fg += 1;
                    } else if near_poly_edge(x, y, &plane, 6.0) || near_polyline(x, y, &check, 13.0)
                    {
                        edge += 1;
                    }
                    if dot_col.is_some() {
                        let d = ((x - dx).powi(2) + (y - dy).powi(2)).sqrt();
                        if d <= 22.0 {
                            dot += 1;
                        } else if d <= 29.0 {
                            ring += 1;
                        }
                    }
                }
            }
            let tot = (SS * SS) as f32;
            let i = (py * n + px) * 4;
            let mut c = [0.0f32; 4];
            let blend = |c: &mut [f32; 4], col: (u8, u8, u8), a: f32| {
                if a <= 0.0 {
                    return;
                }
                let na = a + c[3] / 255.0 * (1.0 - a);
                if na <= 0.0 {
                    return;
                }
                for (k, v) in [col.0, col.1, col.2].into_iter().enumerate() {
                    c[k] = (v as f32 * a + c[k] * c[3] / 255.0 * (1.0 - a)) / na;
                }
                c[3] = na * 255.0;
            };
            blend(&mut c, (0x0C, 0x10, 0x16), edge as f32 / tot * 0.7);
            blend(&mut c, sym, fg as f32 / tot);
            if let Some(dc) = dot_col {
                // Cut the symbol away under the dot's ring, then draw the dot.
                let cut = ring as f32 / tot;
                if cut > 0.0 {
                    blend(&mut c, (0x0C, 0x10, 0x16), cut);
                }
                blend(&mut c, dc, dot as f32 / tot);
            }
            out[i] = c[0] as u8;
            out[i + 1] = c[1] as u8;
            out[i + 2] = c[2] as u8;
            out[i + 3] = c[3] as u8;
        }
    }
    out
}

/// Near any edge of a polygon (for the dark outline).
fn near_poly_edge(x: f32, y: f32, pts: &[(f32, f32)], w: f32) -> bool {
    let mut closed = pts.to_vec();
    closed.push(pts[0]);
    near_polyline(x, y, &closed, w)
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
/// A new tray picture shows only after holding this long.
pub const HOLD_MS: u64 = 400;

pub struct Tray {
    rx: mpsc::Receiver<TrayCmd>,
    last: TrayView,
    /// A different picture waiting to hold for 400 ms, and since when.
    pending: Option<(TrayView, std::time::Instant)>,
    imp: imp::Imp,
}

impl Tray {
    /// Starts the tray icon. `None` when the desktop has no tray.
    pub fn start(ctx: eframe::egui::Context) -> Option<Tray> {
        let (tx, rx) = mpsc::channel();
        let view = TrayView {
            status: "Idle".into(),
            ..Default::default()
        };
        match imp::Imp::start(tx, ctx, view.clone()) {
            Ok(imp) => {
                log::info!("Tray icon started");
                Some(Tray {
                    rx,
                    last: view,
                    pending: None,
                    imp,
                })
            }
            Err(e) => {
                log::info!("No system tray: {e}");
                None
            }
        }
    }

    /// Updates the icon, tooltip and menu if anything changed.
    /// Shows a new state. A change of picture waits until it has held for
    /// 400 ms, so the icon doesn't flicker; only changes are redrawn.
    /// Returns true while a change is waiting: call again in about 400 ms.
    pub fn set(&mut self, view: TrayView) -> bool {
        if view == self.last {
            self.pending = None;
            return false;
        }
        let picture = |v: &TrayView| {
            (
                v.state.look(),
                v.state == TrayState::NeedsFixes,
                v.attention,
                v.problem,
            )
        };
        if picture(&view) != picture(&self.last) {
            let now = std::time::Instant::now();
            match &self.pending {
                Some((p, since)) if picture(p) == picture(&view) => {
                    if now.duration_since(*since) < std::time::Duration::from_millis(HOLD_MS) {
                        return true;
                    }
                }
                _ => {
                    self.pending = Some((view, now));
                    return true;
                }
            }
            log::info!(
                "Tray: {}{}",
                view.state.word(),
                if view.problem {
                    " · problem"
                } else if view.attention {
                    " · needs you"
                } else {
                    ""
                }
            );
        }
        self.pending = None;
        self.imp.set(&view);
        self.last = view;
        false
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
        rgba.chunks(4)
            .flat_map(|p| [p[3], p[0], p[1], p[2]])
            .collect()
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
            if self.view.attention {
                ksni::Status::NeedsAttention
            } else {
                ksni::Status::Active
            }
        }
        fn icon_pixmap(&self) -> Vec<ksni::Icon> {
            [22u32, 32, 64]
                .iter()
                .map(|&s| ksni::Icon {
                    width: s as i32,
                    height: s as i32,
                    data: to_argb(&icon_rgba(&self.view, s)),
                })
                .collect()
        }
        fn attention_icon_pixmap(&self) -> Vec<ksni::Icon> {
            self.icon_pixmap()
        }
        fn tool_tip(&self) -> ksni::ToolTip {
            ksni::ToolTip {
                title: "Stream Preflight".into(),
                description: self.view.status.clone(),
                ..Default::default()
            }
        }
        fn activate(&mut self, _x: i32, _y: i32) {
            self.send(TrayCmd::Open);
        }
        fn menu(&self) -> Vec<ksni::MenuItem<Self>> {
            use ksni::menu::StandardItem;
            vec![
                StandardItem {
                    label: self.view.status.clone(),
                    enabled: false,
                    ..Default::default()
                }
                .into(),
                ksni::MenuItem::Separator,
                StandardItem {
                    label: "Open Preflight".into(),
                    activate: Box::new(|t: &mut Self| t.send(TrayCmd::Open)),
                    ..Default::default()
                }
                .into(),
                StandardItem {
                    label: "Quit".into(),
                    activate: Box::new(|t: &mut Self| t.send(TrayCmd::Quit)),
                    ..Default::default()
                }
                .into(),
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
        pub fn start(
            tx: Sender<TrayCmd>,
            ctx: eframe::egui::Context,
            view: TrayView,
        ) -> Result<Imp, String> {
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
                    let hints: std::collections::HashMap<&str, zbus::zvariant::Value> =
                        std::collections::HashMap::new();
                    conn.call_method(
                        Some("org.freedesktop.Notifications"),
                        "/org/freedesktop/Notifications",
                        Some("org.freedesktop.Notifications"),
                        "Notify",
                        &(
                            "Stream Preflight",
                            0u32,
                            "",
                            title.as_str(),
                            body.as_str(),
                            Vec::<&str>::new(),
                            hints,
                            8000i32,
                        ),
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
    use windows::Win32::UI::Shell::{
        NIF_ICON, NIF_INFO, NIF_MESSAGE, NIF_TIP, NIIF_INFO, NIM_ADD, NIM_DELETE, NIM_MODIFY,
        NOTIFYICONDATAW, Shell_NotifyIconW,
    };
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
        SHARED
            .get()
            .expect("tray")
            .lock()
            .unwrap_or_else(|e| e.into_inner())
    }

    fn wide(s: &str, out: &mut [u16]) {
        let v: Vec<u16> = s.encode_utf16().take(out.len() - 1).collect();
        out[..v.len()].copy_from_slice(&v);
        out[v.len()] = 0;
    }

    unsafe fn make_icon(view: &TrayView) -> HICON {
        let size = 32;
        let rgba = icon_rgba(view, size);
        let bgra: Vec<u8> = rgba
            .chunks(4)
            .flat_map(|p| [p[2], p[1], p[0], p[3]])
            .collect();
        unsafe {
            let color = CreateBitmap(
                size as i32,
                size as i32,
                1,
                32,
                Some(bgra.as_ptr() as *const _),
            );
            let mask_bits = vec![0u8; (size * size / 8) as usize];
            let mask = CreateBitmap(
                size as i32,
                size as i32,
                1,
                1,
                Some(mask_bits.as_ptr() as *const _),
            );
            let info = ICONINFO {
                fIcon: true.into(),
                xHotspot: 0,
                yHotspot: 0,
                hbmMask: mask,
                hbmColor: color,
            };
            let icon = CreateIconIndirect(&info).unwrap_or_default();
            let _ = DeleteObject(color.into());
            let _ = DeleteObject(mask.into());
            icon
        }
    }

    unsafe fn data(hwnd: HWND, view: &TrayView) -> NOTIFYICONDATAW {
        let mut d = NOTIFYICONDATAW {
            cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
            hWnd: hwnd,
            uID: 1,
            ..Default::default()
        };
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
                            let _ = AppendMenuW(
                                menu,
                                MF_STRING | MF_GRAYED,
                                0,
                                PCWSTR(st.as_mut_ptr()),
                            );
                            let _ = AppendMenuW(menu, MF_SEPARATOR, 0, PCWSTR::null());
                            let _ = AppendMenuW(menu, MF_STRING, ID_OPEN, w!("Open Preflight"));
                            let _ = AppendMenuW(menu, MF_STRING, ID_QUIT, w!("Quit"));
                            let mut pt = POINT::default();
                            let _ = GetCursorPos(&mut pt);
                            let _ = SetForegroundWindow(hwnd);
                            let cmd = TrackPopupMenu(
                                menu,
                                TPM_RETURNCMD | TPM_RIGHTBUTTON,
                                pt.x,
                                pt.y,
                                Some(0),
                                hwnd,
                                None,
                            );
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
                    let d = NOTIFYICONDATAW {
                        cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
                        hWnd: hwnd,
                        uID: 1,
                        ..Default::default()
                    };
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
        pub fn start(
            tx: Sender<TrayCmd>,
            ctx: eframe::egui::Context,
            view: TrayView,
        ) -> Result<Imp, String> {
            if SHARED
                .set(Mutex::new(Shared {
                    tx,
                    ctx,
                    view,
                    note: None,
                    hwnd: 0,
                    main: 0,
                }))
                .is_err()
            {
                return Err("tray already started".into());
            }
            let (ok_tx, ok_rx) = std::sync::mpsc::channel();
            std::thread::Builder::new()
                .name("tray".into())
                .spawn(move || unsafe {
                    let inst = GetModuleHandleW(None).unwrap_or_default();
                    let class = w!("PreFlightTray");
                    let wc = WNDCLASSW {
                        lpfnWndProc: Some(wndproc),
                        hInstance: inst.into(),
                        lpszClassName: class,
                        ..Default::default()
                    };
                    RegisterClassW(&wc);
                    let hwnd = match CreateWindowExW(
                        WINDOW_EX_STYLE(0),
                        class,
                        w!("Preflight tray"),
                        WS_OVERLAPPED,
                        0,
                        0,
                        0,
                        0,
                        None,
                        None,
                        Some(inst.into()),
                        None,
                    ) {
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
                    let _ = ok_tx.send(if added {
                        Ok(())
                    } else {
                        Err("Shell_NotifyIcon failed".into())
                    });
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
        pub fn start(
            _: std::sync::mpsc::Sender<TrayCmd>,
            _: eframe::egui::Context,
            _: TrayView,
        ) -> Result<Imp, String> {
            Err("no tray on this system".into())
        }
        pub fn set(&self, _: &TrayView) {}
        pub fn notify(&self, _: &str, _: &str) {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn icon(state: TrayState, attention: bool, problem: bool) -> Vec<u8> {
        icon_rgba(
            &TrayView {
                state,
                attention,
                problem,
                ..Default::default()
            },
            64,
        )
    }

    fn px(img: &[u8], x: usize, y: usize) -> [u8; 4] {
        let i = (y * 64 + x) * 4;
        [img[i], img[i + 1], img[i + 2], img[i + 3]]
    }

    #[test]
    fn icons_follow_the_house_states() {
        let disconnected = icon(TrayState::Idle, false, false);
        let idle = icon(TrayState::Watching, false, false);
        let active = icon(TrayState::Live, false, false);
        let attention = icon(TrayState::Watching, true, false);
        let fixes = icon(TrayState::NeedsFixes, false, false);
        let problem = icon(TrayState::Live, false, true);
        assert_eq!(idle.len(), 64 * 64 * 4);
        // Transparent background.
        for img in [&disconnected, &idle, &active, &problem] {
            assert_eq!(px(img, 0, 0)[3], 0);
            assert_eq!(px(img, 63, 0)[3], 0);
            assert_eq!(px(img, 0, 63)[3], 0);
        }
        // The symbol (inside the plane) is solid: grey, plain or lime.
        let sym = |img: &[u8]| px(img, 33, 22);
        assert_eq!(sym(&disconnected), [0x8A, 0x94, 0xA3, 255]);
        assert_eq!(sym(&idle), [0xD5, 0xDB, 0xE4, 255]);
        assert_eq!(sym(&active), [0xC6, 0xF3, 0x6B, 255]);
        // Bottom-right dot: none, amber (needs attention or fixes), red (problem).
        let dot = |img: &[u8]| px(img, 50, 50);
        assert_ne!(&dot(&idle)[..3], &[0xFB, 0xBF, 0x24]);
        assert_eq!(dot(&attention), [0xFB, 0xBF, 0x24, 255]);
        assert_eq!(dot(&fixes), [0xFB, 0xBF, 0x24, 255]);
        assert_eq!(dot(&problem), [0xF8, 0x71, 0x71, 255]);
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
        for (n, st, att) in [
            ("idle", TrayState::Idle, false),
            ("watching", TrayState::Watching, false),
            ("testing", TrayState::Testing, false),
            ("live", TrayState::Live, false),
            ("live_att", TrayState::Live, true),
            ("watch_att", TrayState::Watching, true),
        ] {
            let px = icon_rgba(
                &TrayView {
                    state: st,
                    attention: att,
                    problem: false,
                    status: String::new(),
                },
                64,
            );
            image::save_buffer(
                format!("/tmp/tray_{n}.png"),
                &px,
                64,
                64,
                image::ExtendedColorType::Rgba8,
            )
            .unwrap();
        }
    }
}
