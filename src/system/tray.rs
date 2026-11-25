use crate::system::proxy::SystemProxyTakeoverState;
use std::sync::atomic::{AtomicIsize, Ordering};
use std::sync::{Arc, Mutex, mpsc};

#[derive(Debug, Clone)]
pub enum TrayCommand {
    ShowWindow,
    EnableSystemProxy,
    DisableSystemProxy,
    ReapplySystemProxy,
    ToggleTun,
    SetMode(String),
    RestartService,
    Quit,
}

#[derive(Debug, Clone)]
pub struct TrayState {
    pub visible: bool,
    pub proxy_state: SystemProxyTakeoverState,
    pub proxy_reason: Option<String>,
    pub proxy_server: String,
    pub tun_enabled: bool,
    pub mode: String,
    pub service_running: bool,
    pub main_hwnd: isize,
}

impl Default for TrayState {
    fn default() -> Self {
        Self {
            visible: false,
            proxy_state: SystemProxyTakeoverState::Unmanaged,
            proxy_reason: None,
            proxy_server: "127.0.0.1:7890".to_string(),
            tun_enabled: false,
            mode: "rule".to_string(),
            service_running: false,
            main_hwnd: 0,
        }
    }
}

#[cfg(windows)]
mod win {
    use super::*;
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    use std::sync::OnceLock;
    use windows_sys::Win32::Foundation::{
        CloseHandle, ERROR_ALREADY_EXISTS, GetLastError, HANDLE, HWND, LPARAM, LRESULT, POINT,
        WPARAM,
    };
    use windows_sys::Win32::Graphics::Gdi::{
        BI_RGB, BITMAPINFO, BITMAPINFOHEADER, CreateBitmap, CreateCompatibleDC, CreateDIBSection,
        DIB_RGB_COLORS, DeleteDC, DeleteObject, HBITMAP,
    };
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows_sys::Win32::System::Threading::{CreateMutexW, GetCurrentProcessId};
    use windows_sys::Win32::UI::Shell::{
        NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NIM_MODIFY, NOTIFYICONDATAW,
        Shell_NotifyIconW,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        AppendMenuW, CreateIconIndirect, CreatePopupMenu, CreateWindowExW, DefWindowProcW,
        DestroyIcon, DestroyMenu, DestroyWindow, DispatchMessageW, EnumWindows, FindWindowW,
        GW_OWNER, GWL_EXSTYLE, GetClassNameW, GetCursorPos, GetMessageW, GetSystemMetrics,
        GetWindow, GetWindowLongW, GetWindowTextW, GetWindowThreadProcessId, HICON, ICON_BIG,
        ICON_SMALL, ICONINFO, IMAGE_ICON, InsertMenuItemW, IsIconic, IsWindow, LR_DEFAULTCOLOR,
        LR_SHARED, LoadImageW, MENUITEMINFOW, MF_CHECKED, MF_SEPARATOR, MF_STRING, MFS_CHECKED,
        MIIM_CHECKMARKS, MIIM_ID, MIIM_STATE, MIIM_STRING, MSG, PostMessageW, PostQuitMessage,
        RegisterClassW, RegisterWindowMessageW, SM_CXICON, SM_CXMENUCHECK, SM_CXSMICON, SM_CYICON,
        SM_CYMENUCHECK, SM_CYSMICON, SW_HIDE, SW_RESTORE, SW_SHOW, SendMessageW,
        SetForegroundWindow, SetMenuDefaultItem, ShowWindow, TPM_BOTTOMALIGN, TPM_LEFTALIGN,
        TPM_NONOTIFY, TPM_RETURNCMD, TPM_RIGHTBUTTON, TrackPopupMenu, TranslateMessage, WM_APP,
        WM_CONTEXTMENU, WM_LBUTTONDBLCLK, WM_LBUTTONUP, WM_NULL, WM_RBUTTONUP, WM_SETICON,
        WNDCLASSW, WS_EX_APPWINDOW, WS_EX_TOOLWINDOW,
    };

    const APP_ICON_RESOURCE_ID: usize = 1;

    const TRAY_CLASS_NAME: &str = "MihomoDesktopTrayWindow";
    const SINGLE_INSTANCE_MUTEX: &str = "Local\\MihomoDesktopSingleInstanceMutex";
    const TRAY_ICON_ID: u32 = 1001;

    const WM_TRAY_CALLBACK: u32 = WM_APP + 1;
    const WM_TRAY_SYNC: u32 = WM_APP + 2;
    const WM_TRAY_WAKE_MAIN: u32 = WM_APP + 3;
    const WM_TRAY_SHUTDOWN: u32 = WM_APP + 4;

    const ID_SHOW_WINDOW: usize = 2001;
    const ID_PROXY_ENABLE: usize = 2002;
    const ID_PROXY_DISABLE: usize = 2003;
    const ID_PROXY_REAPPLY: usize = 2004;
    const ID_TOGGLE_TUN: usize = 2005;
    const ID_MODE_RULE: usize = 2006;
    const ID_MODE_GLOBAL: usize = 2007;
    const ID_MODE_DIRECT: usize = 2008;
    const ID_RESTART_SERVICE: usize = 2009;
    const ID_QUIT: usize = 2010;

    fn to_wide(s: &str) -> Vec<u16> {
        OsStr::new(s).encode_wide().chain(std::iter::once(0)).collect()
    }

    pub struct SingleInstanceGuard(HANDLE);

    impl Drop for SingleInstanceGuard {
        fn drop(&mut self) {
            unsafe {
                if !self.0.is_null() {
                    CloseHandle(self.0);
                }
            }
        }
    }

    /// Acquire a per-session single-instance mutex.
    /// If another GUI instance is already running, wakes its window and returns `None`.
    pub fn acquire_single_instance() -> Option<SingleInstanceGuard> {
        unsafe {
            let name = to_wide(SINGLE_INSTANCE_MUTEX);
            let handle = CreateMutexW(std::ptr::null(), 0, name.as_ptr());
            if handle.is_null() {
                return Some(SingleInstanceGuard(std::ptr::null_mut()));
            }
            if GetLastError() == ERROR_ALREADY_EXISTS {
                CloseHandle(handle);
                // Try to wake the existing instance's tray controller
                let cls = to_wide(TRAY_CLASS_NAME);
                let tray_hwnd = FindWindowW(cls.as_ptr(), std::ptr::null());
                if !tray_hwnd.is_null() {
                    PostMessageW(tray_hwnd, WM_TRAY_WAKE_MAIN, 0, 0);
                } else {
                    // Fallback: wake visible window titled "Mihomo Desktop"
                    let title = to_wide("Mihomo Desktop");
                    let main_hwnd = FindWindowW(std::ptr::null(), title.as_ptr());
                    if !main_hwnd.is_null() {
                        show_and_focus_main_window(main_hwnd as isize);
                    }
                }
                return None;
            }
            Some(SingleInstanceGuard(handle))
        }
    }

    /// Locate our process's primary GPUI top-level HWND (excluding hidden helper/tray windows).
    pub fn find_current_process_main_hwnd() -> Option<isize> {
        struct SearchCtx {
            pid: u32,
            exact: isize,
            fallback: isize,
        }

        unsafe extern "system" fn enum_proc(hwnd: HWND, lparam: LPARAM) -> i32 {
            let ctx = unsafe { &mut *(lparam as *mut SearchCtx) };
            let mut win_pid = 0u32;
            unsafe { GetWindowThreadProcessId(hwnd, &mut win_pid) };
            if win_pid != ctx.pid {
                return 1;
            }
            // Skip owned helper windows
            if unsafe { !GetWindow(hwnd, GW_OWNER).is_null() } {
                return 1;
            }
            // Skip tool/message helper windows (e.g. GPUI's hidden platform window)
            let ex_style = unsafe { GetWindowLongW(hwnd, GWL_EXSTYLE) as u32 };
            if (ex_style & WS_EX_TOOLWINDOW) != 0 {
                return 1;
            }
            // Skip our own tray message window
            let mut cls_buf = [0u16; 128];
            let cls_len =
                unsafe { GetClassNameW(hwnd, cls_buf.as_mut_ptr(), cls_buf.len() as i32) };
            if cls_len > 0 {
                let cls = String::from_utf16_lossy(&cls_buf[..cls_len as usize]);
                if cls == TRAY_CLASS_NAME {
                    return 1;
                }
            }
            let mut title_buf = [0u16; 128];
            let title_len =
                unsafe { GetWindowTextW(hwnd, title_buf.as_mut_ptr(), title_buf.len() as i32) };
            if title_len > 0 {
                let title = String::from_utf16_lossy(&title_buf[..title_len as usize]);
                if title == "Mihomo Desktop" {
                    ctx.exact = hwnd as isize;
                    return 0; // exact match found
                }
            }
            if ctx.fallback == 0 && (ex_style & WS_EX_APPWINDOW) != 0 {
                ctx.fallback = hwnd as isize;
            }
            1
        }

        let mut ctx = SearchCtx {
            pid: unsafe { GetCurrentProcessId() },
            exact: 0,
            fallback: 0,
        };
        unsafe {
            EnumWindows(Some(enum_proc), &mut ctx as *mut _ as LPARAM);
        }
        let found = if ctx.exact != 0 {
            ctx.exact
        } else {
            ctx.fallback
        };
        (found != 0).then_some(found)
    }

    pub fn hide_main_window(hwnd: isize) {
        if hwnd == 0 {
            return;
        }
        unsafe {
            let h = hwnd as HWND;
            if IsWindow(h) != 0 {
                ShowWindow(h, SW_HIDE);
            }
        }
    }

    pub fn show_and_focus_main_window(hwnd: isize) {
        if hwnd == 0 {
            return;
        }
        unsafe {
            let h = hwnd as HWND;
            if IsWindow(h) != 0 {
                ShowWindow(h, SW_SHOW);
                if IsIconic(h) != 0 {
                    ShowWindow(h, SW_RESTORE);
                }
                SetForegroundWindow(h);
            }
        }
    }

    /// Apply the embedded EXE icon resource (`RT_GROUP_ICON` ID 1) to the main GPUI window
    /// via `WM_SETICON` (`ICON_BIG` and `ICON_SMALL`) so the Windows taskbar, Alt+Tab switcher,
    /// and window caption always display the crisp DPI-matched app logo.
    pub fn apply_window_icon(hwnd: isize) {
        if hwnd == 0 {
            return;
        }
        unsafe {
            let h = hwnd as HWND;
            if IsWindow(h) == 0 {
                return;
            }
            let hinstance = GetModuleHandleW(std::ptr::null());
            let res_id = APP_ICON_RESOURCE_ID as *const u16;

            let big_w = GetSystemMetrics(SM_CXICON).max(32);
            let big_h = GetSystemMetrics(SM_CYICON).max(32);
            let big_icon = LoadImageW(
                hinstance,
                res_id,
                IMAGE_ICON,
                big_w,
                big_h,
                LR_DEFAULTCOLOR | LR_SHARED,
            );
            if !big_icon.is_null() {
                SendMessageW(h, WM_SETICON, ICON_BIG as WPARAM, big_icon as LPARAM);
            }

            let small_w = GetSystemMetrics(SM_CXSMICON).max(16);
            let small_h = GetSystemMetrics(SM_CYSMICON).max(16);
            let small_icon = LoadImageW(
                hinstance,
                res_id,
                IMAGE_ICON,
                small_w,
                small_h,
                LR_DEFAULTCOLOR | LR_SHARED,
            );
            if !small_icon.is_null() {
                SendMessageW(h, WM_SETICON, ICON_SMALL as WPARAM, small_icon as LPARAM);
            }
        }
    }

    struct TrayRuntime {
        state: Arc<Mutex<TrayState>>,
        cmd_tx: mpsc::Sender<TrayCommand>,
        taskbar_created_msg: u32,
        icon_added: Mutex<bool>,
        current_icon: Mutex<isize>,
    }

    static TRAY_RUNTIME: OnceLock<Arc<TrayRuntime>> = OnceLock::new();

    #[derive(Clone)]
    pub struct TrayController {
        state: Arc<Mutex<TrayState>>,
        tray_hwnd: Arc<AtomicIsize>,
    }

    impl TrayController {
        pub fn start(initial: TrayState, cmd_tx: mpsc::Sender<TrayCommand>) -> Self {
            let state = Arc::new(Mutex::new(initial));
            let tray_hwnd = Arc::new(AtomicIsize::new(0));
            let taskbar_created_msg = unsafe {
                let msg_name = to_wide("TaskbarCreated");
                RegisterWindowMessageW(msg_name.as_ptr())
            };

            let runtime = Arc::new(TrayRuntime {
                state: Arc::clone(&state),
                cmd_tx,
                taskbar_created_msg,
                icon_added: Mutex::new(false),
                current_icon: Mutex::new(0),
            });
            let _ = TRAY_RUNTIME.set(Arc::clone(&runtime));

            let hwnd_slot = Arc::clone(&tray_hwnd);
            let _ = std::thread::Builder::new()
                .name("mihomo-tray".to_string())
                .spawn(move || unsafe {
                    let hinstance = GetModuleHandleW(std::ptr::null());
                    let cls_name = to_wide(TRAY_CLASS_NAME);
                    let wc = WNDCLASSW {
                        style: 0,
                        lpfnWndProc: Some(tray_wnd_proc),
                        cbClsExtra: 0,
                        cbWndExtra: 0,
                        hInstance: hinstance,
                        hIcon: std::ptr::null_mut(),
                        hCursor: std::ptr::null_mut(),
                        hbrBackground: std::ptr::null_mut(),
                        lpszMenuName: std::ptr::null(),
                        lpszClassName: cls_name.as_ptr(),
                    };
                    RegisterClassW(&wc);

                    let hwnd = CreateWindowExW(
                        0,
                        cls_name.as_ptr(),
                        cls_name.as_ptr(),
                        0,
                        0,
                        0,
                        0,
                        0,
                        std::ptr::null_mut(),
                        std::ptr::null_mut(),
                        hinstance,
                        std::ptr::null(),
                    );
                    if hwnd.is_null() {
                        return;
                    }
                    hwnd_slot.store(hwnd as isize, Ordering::SeqCst);

                    // Apply initial state immediately
                    PostMessageW(hwnd, WM_TRAY_SYNC, 0, 0);

                    let mut msg: MSG = std::mem::zeroed();
                    while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {
                        TranslateMessage(&msg);
                        DispatchMessageW(&msg);
                    }
                });

            Self { state, tray_hwnd }
        }

        pub fn update(&self, edit: impl FnOnce(&mut TrayState)) {
            {
                let mut g = self.state.lock().unwrap();
                edit(&mut g);
            }
            let h = self.tray_hwnd.load(Ordering::SeqCst);
            if h != 0 {
                unsafe {
                    PostMessageW(h as HWND, WM_TRAY_SYNC, 0, 0);
                }
            }
        }

        pub fn shutdown(&self) {
            {
                let mut g = self.state.lock().unwrap();
                g.visible = false;
            }
            let h = self.tray_hwnd.load(Ordering::SeqCst);
            if h != 0 {
                unsafe {
                    PostMessageW(h as HWND, WM_TRAY_SHUTDOWN, 0, 0);
                }
            }
        }
    }

    unsafe extern "system" fn tray_wnd_proc(
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        let Some(rt) = TRAY_RUNTIME.get() else {
            return unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) };
        };

        if msg == rt.taskbar_created_msg && rt.taskbar_created_msg != 0 {
            // Explorer restarted; mark icon as not added and re-sync
            *rt.icon_added.lock().unwrap() = false;
            sync_tray_icon(hwnd, rt);
            return 0;
        }

        match msg {
            WM_TRAY_SYNC => {
                sync_tray_icon(hwnd, rt);
                0
            }
            WM_TRAY_WAKE_MAIN => {
                let main_hwnd = rt.state.lock().unwrap().main_hwnd;
                show_and_focus_main_window(main_hwnd);
                let _ = rt.cmd_tx.send(TrayCommand::ShowWindow);
                0
            }
            WM_TRAY_SHUTDOWN => {
                remove_tray_icon(hwnd, rt);
                unsafe {
                    DestroyWindow(hwnd);
                    PostQuitMessage(0);
                }
                0
            }
            WM_TRAY_CALLBACK => {
                let event = (lparam as u32) & 0xFFFF;
                match event {
                    WM_LBUTTONUP | WM_LBUTTONDBLCLK => {
                        let main_hwnd = rt.state.lock().unwrap().main_hwnd;
                        show_and_focus_main_window(main_hwnd);
                        let _ = rt.cmd_tx.send(TrayCommand::ShowWindow);
                    }
                    WM_RBUTTONUP | WM_CONTEXTMENU => {
                        show_context_menu(hwnd, rt);
                    }
                    _ => {}
                }
                0
            }
            _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
        }
    }

    fn sync_tray_icon(hwnd: HWND, rt: &TrayRuntime) {
        let snapshot = rt.state.lock().unwrap().clone();
        if !snapshot.visible {
            remove_tray_icon(hwnd, rt);
            return;
        }

        let new_icon = create_status_icon(snapshot.proxy_state);
        let mut nid: NOTIFYICONDATAW = unsafe { std::mem::zeroed() };
        nid.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
        nid.hWnd = hwnd;
        nid.uID = TRAY_ICON_ID;
        nid.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP;
        nid.uCallbackMessage = WM_TRAY_CALLBACK;
        nid.hIcon = new_icon;

        let tip = build_tooltip(&snapshot);
        let tip_w: Vec<u16> = OsStr::new(&tip).encode_wide().collect();
        let copy_len = tip_w.len().min(nid.szTip.len() - 1);
        nid.szTip[..copy_len].copy_from_slice(&tip_w[..copy_len]);
        nid.szTip[copy_len] = 0;

        let mut added = rt.icon_added.lock().unwrap();
        let ok = unsafe {
            if *added {
                Shell_NotifyIconW(NIM_MODIFY, &nid)
            } else {
                Shell_NotifyIconW(NIM_ADD, &nid)
            }
        };
        if ok != 0 {
            *added = true;
        }

        let mut prev_icon = rt.current_icon.lock().unwrap();
        if *prev_icon != 0 {
            unsafe {
                DestroyIcon(*prev_icon as HICON);
            }
        }
        *prev_icon = new_icon as isize;
    }

    fn remove_tray_icon(hwnd: HWND, rt: &TrayRuntime) {
        let mut added = rt.icon_added.lock().unwrap();
        if *added {
            let mut nid: NOTIFYICONDATAW = unsafe { std::mem::zeroed() };
            nid.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
            nid.hWnd = hwnd;
            nid.uID = TRAY_ICON_ID;
            unsafe {
                Shell_NotifyIconW(NIM_DELETE, &nid);
            }
            *added = false;
        }
        let mut prev_icon = rt.current_icon.lock().unwrap();
        if *prev_icon != 0 {
            unsafe {
                DestroyIcon(*prev_icon as HICON);
            }
            *prev_icon = 0;
        }
    }

    fn build_tooltip(s: &TrayState) -> String {
        let tun_str = if s.tun_enabled { "开" } else { "关" };
        let svc_str = if s.service_running { "运行中" } else { "已停止" };
        let proxy_line = match (&s.proxy_state, &s.proxy_reason) {
            (SystemProxyTakeoverState::ManagedIneffective, Some(reason)) => {
                format!("{} ({reason})", s.proxy_state.label())
            }
            _ => s.proxy_state.label().to_string(),
        };
        format!(
            "Mihomo Desktop\n系统代理: {}\nTUN: {} | 服务: {}",
            proxy_line, tun_str, svc_str
        )
    }

    fn show_context_menu(hwnd: HWND, rt: &TrayRuntime) {
        let s = rt.state.lock().unwrap().clone();
        unsafe {
            let hmenu = CreatePopupMenu();
            if hmenu.is_null() {
                return;
            }

            let add_item = |flags: u32, id: usize, text: &str| {
                let w = to_wide(text);
                AppendMenuW(hmenu, flags, id, w.as_ptr());
            };

            add_item(MF_STRING, ID_SHOW_WINDOW, "打开 Mihomo Desktop");
            SetMenuDefaultItem(hmenu, ID_SHOW_WINDOW as u32, 0);
            AppendMenuW(hmenu, MF_SEPARATOR, 0, std::ptr::null());

            // Single Surge-style 3-state System Proxy row:
            // - Unmanaged:          (blank) 设置为系统代理
            // - ManagedEffective:     ✓     设置为系统代理
            // - ManagedIneffective:   -     设置为系统代理
            let mut dash_bmp: HBITMAP = std::ptr::null_mut();
            match s.proxy_state {
                SystemProxyTakeoverState::Unmanaged => {
                    add_item(MF_STRING, ID_PROXY_ENABLE, "设置为系统代理");
                }
                SystemProxyTakeoverState::ManagedEffective => {
                    add_item(MF_STRING | MF_CHECKED, ID_PROXY_DISABLE, "设置为系统代理");
                }
                SystemProxyTakeoverState::ManagedIneffective => {
                    dash_bmp = create_mixed_dash_bitmap();
                    let mut label_w = to_wide("设置为系统代理");
                    let mut mii: MENUITEMINFOW = std::mem::zeroed();
                    mii.cbSize = std::mem::size_of::<MENUITEMINFOW>() as u32;
                    mii.fMask = MIIM_ID | MIIM_STATE | MIIM_STRING | MIIM_CHECKMARKS;
                    mii.wID = ID_PROXY_REAPPLY as u32;
                    mii.fState = MFS_CHECKED;
                    mii.hbmpChecked = dash_bmp;
                    mii.hbmpUnchecked = std::ptr::null_mut();
                    mii.dwTypeData = label_w.as_mut_ptr();
                    InsertMenuItemW(hmenu, u32::MAX, 1, &mii);
                }
            }

            let tun_flags = MF_STRING | if s.tun_enabled { MF_CHECKED } else { 0 };
            add_item(tun_flags, ID_TOGGLE_TUN, "TUN 虚拟网卡模式");

            AppendMenuW(hmenu, MF_SEPARATOR, 0, std::ptr::null());

            let mode_lower = s.mode.to_ascii_lowercase();
            add_item(
                MF_STRING | if mode_lower == "rule" { MF_CHECKED } else { 0 },
                ID_MODE_RULE,
                "代理模式：规则 (Rule)",
            );
            add_item(
                MF_STRING | if mode_lower == "global" { MF_CHECKED } else { 0 },
                ID_MODE_GLOBAL,
                "代理模式：全局 (Global)",
            );
            add_item(
                MF_STRING | if mode_lower == "direct" { MF_CHECKED } else { 0 },
                ID_MODE_DIRECT,
                "代理模式：直连 (Direct)",
            );

            AppendMenuW(hmenu, MF_SEPARATOR, 0, std::ptr::null());
            add_item(MF_STRING, ID_RESTART_SERVICE, "重启 Mihomo 服务");
            add_item(MF_STRING, ID_QUIT, "退出 Mihomo Desktop");

            let mut pt = POINT { x: 0, y: 0 };
            GetCursorPos(&mut pt);
            SetForegroundWindow(hwnd);
            let cmd = TrackPopupMenu(
                hmenu,
                TPM_RETURNCMD | TPM_RIGHTBUTTON | TPM_NONOTIFY | TPM_LEFTALIGN | TPM_BOTTOMALIGN,
                pt.x,
                pt.y,
                0,
                hwnd,
                std::ptr::null(),
            ) as usize;
            PostMessageW(hwnd, WM_NULL, 0, 0);
            DestroyMenu(hmenu);
            if !dash_bmp.is_null() {
                DeleteObject(dash_bmp as _);
            }

            match cmd {
                ID_SHOW_WINDOW => {
                    show_and_focus_main_window(s.main_hwnd);
                    let _ = rt.cmd_tx.send(TrayCommand::ShowWindow);
                }
                ID_PROXY_ENABLE => {
                    let _ = rt.cmd_tx.send(TrayCommand::EnableSystemProxy);
                }
                ID_PROXY_DISABLE => {
                    let _ = rt.cmd_tx.send(TrayCommand::DisableSystemProxy);
                }
                ID_PROXY_REAPPLY => {
                    let _ = rt.cmd_tx.send(TrayCommand::ReapplySystemProxy);
                }
                ID_TOGGLE_TUN => {
                    let _ = rt.cmd_tx.send(TrayCommand::ToggleTun);
                }
                ID_MODE_RULE => {
                    let _ = rt.cmd_tx.send(TrayCommand::SetMode("rule".to_string()));
                }
                ID_MODE_GLOBAL => {
                    let _ = rt.cmd_tx.send(TrayCommand::SetMode("global".to_string()));
                }
                ID_MODE_DIRECT => {
                    let _ = rt.cmd_tx.send(TrayCommand::SetMode("direct".to_string()));
                }
                ID_RESTART_SERVICE => {
                    let _ = rt.cmd_tx.send(TrayCommand::RestartService);
                }
                ID_QUIT => {
                    let _ = rt.cmd_tx.send(TrayCommand::Quit);
                }
                _ => {}
            }
        }
    }

    /// Create a crisp monochrome '-' (indeterminate/mixed) bitmap sized to Windows' menu checkmark
    /// metrics (`SM_CXMENUCHECK x SM_CYMENUCHECK`). In Win32 monochrome checkmarks, bit `1` is
    /// transparent background and bit `0` is rendered in the active menu text color.
    fn create_mixed_dash_bitmap() -> HBITMAP {
        unsafe {
            let w = GetSystemMetrics(SM_CXMENUCHECK).clamp(11, 32);
            let h = GetSystemMetrics(SM_CYMENUCHECK).clamp(11, 32);
            // Each scanline of a 1-bpp bitmap must be WORD (16-bit) aligned.
            let stride_bytes = (((w as usize) + 15) / 16) * 2;
            let mut bits = vec![0xFFu8; stride_bytes * (h as usize)];

            let x0 = (w / 4).max(2);
            let x1 = (w - x0 - 1).max(x0 + 3);
            let mid = h / 2;
            let thickness = if h >= 18 { 2 } else { 1 };
            let y0 = mid - thickness / 2;
            let y1 = y0 + thickness; // inclusive -> 2px or 3px crisp dash

            for y in y0..=y1.min(h - 1) {
                for x in x0..=x1.min(w - 1) {
                    let byte_idx = (y as usize) * stride_bytes + (x as usize) / 8;
                    let bit_mask = 0x80u8 >> ((x as usize) % 8);
                    bits[byte_idx] &= !bit_mask; // 0 = foreground dash pixel
                }
            }

            CreateBitmap(w, h, 1, 1, bits.as_ptr() as *const _)
        }
    }

    /// Generate a crisp 32x32 RGBA tray icon color-coded by System Proxy takeover state:
    /// - ManagedEffective: Emerald Green (#10B981)
    /// - ManagedIneffective: Amber Warning (#F59E0B)
    /// - Unmanaged: Cool Blue (#3B82F6)
    fn create_status_icon(state: SystemProxyTakeoverState) -> HICON {
        const SIZE: i32 = 32;
        let (r, g, b): (u8, u8, u8) = match state {
            SystemProxyTakeoverState::ManagedEffective => (0x10, 0xB9, 0x81),
            SystemProxyTakeoverState::ManagedIneffective => (0xF5, 0x9E, 0x0B),
            SystemProxyTakeoverState::Unmanaged => (0x3B, 0x82, 0xF6),
        };

        unsafe {
            let mut bi: BITMAPINFO = std::mem::zeroed();
            bi.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
            bi.bmiHeader.biWidth = SIZE;
            bi.bmiHeader.biHeight = -SIZE; // top-down DIB
            bi.bmiHeader.biPlanes = 1;
            bi.bmiHeader.biBitCount = 32;
            bi.bmiHeader.biCompression = BI_RGB;

            let mut bits: *mut core::ffi::c_void = std::ptr::null_mut();
            let hdc = CreateCompatibleDC(std::ptr::null_mut());
            let color_bmp: HBITMAP =
                CreateDIBSection(hdc, &bi, DIB_RGB_COLORS, &mut bits, std::ptr::null_mut(), 0);

            if !color_bmp.is_null() && !bits.is_null() {
                let pixels = std::slice::from_raw_parts_mut(
                    bits as *mut u8,
                    (SIZE * SIZE * 4) as usize,
                );
                let cx = 15.5f32;
                let cy = 15.5f32;
                for y in 0..SIZE {
                    for x in 0..SIZE {
                        let dx = x as f32 - cx;
                        let dy = y as f32 - cy;
                        let dist = (dx * dx + dy * dy).sqrt();
                        let idx = ((y * SIZE + x) * 4) as usize;

                        if dist <= 14.5 {
                            // Anti-aliased circular badge
                            let alpha = if dist > 13.5 {
                                ((14.5 - dist).clamp(0.0, 1.0) * 255.0) as u8
                            } else {
                                255u8
                            };

                            // Crisp white stylized "M" glyph inside the badge
                            let in_m = is_m_glyph(x, y);
                            let (pr, pg, pb) = if in_m {
                                (255u8, 255u8, 255u8)
                            } else {
                                (r, g, b)
                            };
                            pixels[idx] = pb;
                            pixels[idx + 1] = pg;
                            pixels[idx + 2] = pr;
                            pixels[idx + 3] = alpha;
                        } else {
                            pixels[idx] = 0;
                            pixels[idx + 1] = 0;
                            pixels[idx + 2] = 0;
                            pixels[idx + 3] = 0;
                        }
                    }
                }
            }

            let mask_bmp = CreateBitmap(SIZE, SIZE, 1, 1, std::ptr::null());
            let ii = ICONINFO {
                fIcon: 1,
                xHotspot: 0,
                yHotspot: 0,
                hbmMask: mask_bmp,
                hbmColor: color_bmp,
            };
            let hicon = CreateIconIndirect(&ii);
            DeleteObject(color_bmp as _);
            DeleteObject(mask_bmp as _);
            DeleteDC(hdc);
            hicon
        }
    }

    fn is_m_glyph(x: i32, y: i32) -> bool {
        if !(9..=22).contains(&y) || !(8..=23).contains(&x) {
            return false;
        }
        // Left & right vertical pillars of 'M'
        if (8..=10).contains(&x) || (21..=23).contains(&x) {
            return true;
        }
        // Diagonal V strokes meeting at center (15..16, 17)
        if y <= 18 {
            let left_diag = 10 + (y - 9) * 6 / 9;
            let right_diag = 21 - (y - 9) * 6 / 9;
            if (x - left_diag).abs() <= 1 || (x - right_diag).abs() <= 1 {
                return true;
            }
        }
        false
    }
}

#[cfg(not(windows))]
mod win {
    use super::*;
    pub struct SingleInstanceGuard;
    pub fn acquire_single_instance() -> Option<SingleInstanceGuard> {
        Some(SingleInstanceGuard)
    }
    pub fn find_current_process_main_hwnd() -> Option<isize> {
        None
    }
    pub fn hide_main_window(_: isize) {}
    pub fn show_and_focus_main_window(_: isize) {}
    pub fn apply_window_icon(_: isize) {}

    #[derive(Clone)]
    pub struct TrayController;
    impl TrayController {
        pub fn start(_: TrayState, _: mpsc::Sender<TrayCommand>) -> Self {
            Self
        }
        pub fn update(&self, _: impl FnOnce(&mut TrayState)) {}
        pub fn shutdown(&self) {}
    }
}

pub use win::*;
