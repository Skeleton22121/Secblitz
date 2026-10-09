use super::logic::{self, Icon};
use super::{browser, warn_logic};
use crate::i18n::Lang;
use anyhow::{bail, Result};
use secblitz::filter::config::Notice;
use secblitz::status::{self, Status};
use std::{
    cell::RefCell,
    os::windows::ffi::OsStrExt,
    ptr::{null, null_mut},
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Mutex,
    },
    time::{Duration, Instant},
};
use windows_sys::Win32::{
    Foundation::{
        CloseHandle, GetLastError, ERROR_ALREADY_EXISTS, HWND, LPARAM, LRESULT, POINT,
        WAIT_OBJECT_0, WPARAM,
    },
    Graphics::Gdi::{
        CreateBitmap, CreateDIBSection, DeleteObject, GetDC, ReleaseDC, BITMAPINFO,
        BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS,
    },
    Security::{
        Authorization::{GetSecurityInfo, SE_KERNEL_OBJECT},
        IsWellKnownSid, WinBuiltinAdministratorsSid, WinLocalSystemSid, OWNER_SECURITY_INFORMATION,
    },
    Storage::FileSystem::{
        FindCloseChangeNotification, FindFirstChangeNotificationW, FindNextChangeNotification,
        FILE_NOTIFY_CHANGE_FILE_NAME, FILE_NOTIFY_CHANGE_LAST_WRITE,
    },
    System::{
        LibraryLoader::GetModuleHandleW,
        Threading::{CreateMutexW, OpenEventW, WaitForSingleObject, INFINITE},
    },
    UI::{
        Shell::{
            ShellExecuteW, Shell_NotifyIconW, NIF_ICON, NIF_INFO, NIF_MESSAGE, NIF_SHOWTIP,
            NIF_TIP, NIIF_INFO, NIM_ADD, NIM_DELETE, NIM_MODIFY, NIM_SETVERSION,
            NIN_BALLOONUSERCLICK, NIN_SELECT, NOTIFYICONDATAW, NOTIFYICON_VERSION_4,
        },
        WindowsAndMessaging::{
            AppendMenuW, CreateIconIndirect, CreatePopupMenu, CreateWindowExW, DefWindowProcW,
            DestroyIcon, DestroyMenu, DestroyWindow, DispatchMessageW, GetCursorPos, GetMessageW,
            GetSystemMetrics, KillTimer, PostMessageW, PostQuitMessage, RegisterClassExW,
            RegisterWindowMessageW, SetForegroundWindow, SetTimer, TrackPopupMenu,
            TranslateMessage, HICON, ICONINFO, MF_SEPARATOR, MF_STRING, MSG, SM_CXSMICON,
            SW_SHOWNORMAL, TPM_BOTTOMALIGN, TPM_RETURNCMD, TPM_RIGHTBUTTON, WM_APP, WM_CLOSE,
            WM_CONTEXTMENU, WM_DESTROY, WM_ENDSESSION, WM_LBUTTONUP, WM_NULL, WM_QUERYENDSESSION,
            WM_RBUTTONUP, WM_TIMER, WNDCLASSEXW, WS_EX_TOOLWINDOW, WS_POPUP,
        },
    },
};

#[link(name = "kernel32")]
extern "system" {
    fn LocalFree(p: *mut std::ffi::c_void) -> *mut std::ffi::c_void;
}

const MUTEX: &str = "Local\\SecblitzTray";
const QUIESCE_EVENT: &str = "Global\\SecblitzUpdateQuiesce";
const SYNCHRONIZE: u32 = 0x0010_0000;
const READ_CONTROL: u32 = 0x0002_0000;
const CALLBACK: u32 = WM_APP + 1;
const FOLDER_CHANGED: u32 = WM_APP + 2;
const BALLOON_LATER: u32 = WM_APP + 3;
const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const MATCH_FOR: Duration = Duration::from_secs(4);
const MATCH_EVERY: Duration = Duration::from_millis(200);
const ADDRESS_WAIT: Duration = Duration::from_millis(1500);
/// Reading an address bar walks the browser's accessibility tree, so it is done far less often
/// than the title check.
const ADDRESS_EVERY: Duration = Duration::from_secs(1);
const POLL_TIMER: usize = 1;
const QUIESCE_TIMER: usize = 2;
const POLL_EVERY: u32 = 60_000;
const QUIESCE_EVERY: u32 = 5_000;
const ID_OPEN: usize = 1;
const ID_CHECK: usize = 2;
const ID_QUIT: usize = 3;

struct Tray {
    lang: Lang,
    icons: [HICON; 4],
    taskbar_created: u32,
    last: Option<Status>,
    shown: Option<(usize, String)>,
    opened: Option<Instant>,
    notices: logic::Notices,
    page: Option<&'static str>,
}
/// The one warning panel that may be open, and the blocks waiting for a balloon because no
/// browser showed the site in time. Both are used from the detection threads.
static PANEL: Mutex<Option<OpenPanel>> = Mutex::new(None);
/// Counts the blocks being looked for, so only the newest one may open a panel.
static NEWEST: AtomicU64 = AtomicU64::new(0);
/// The site being looked for, so repeated blocks of it do not start the search over.
static SEARCHING: Mutex<Option<(u64, String)>> = Mutex::new(None);

struct OpenPanel {
    child: std::process::Child,
    window: usize,
    site: String,
}
static LATER: Mutex<Vec<Notice>> = Mutex::new(Vec::new());
static CHANGE_QUEUED: AtomicBool = AtomicBool::new(false);

thread_local! {
    static TRAY: RefCell<Option<Tray>> = const { RefCell::new(None) };
}

fn with_tray<R>(f: impl FnOnce(&mut Tray) -> R) -> Option<R> {
    TRAY.with(|cell| {
        cell.try_borrow_mut()
            .ok()
            .and_then(|mut t| t.as_mut().map(f))
    })
}

fn wide(s: impl AsRef<std::ffi::OsStr>) -> Vec<u16> {
    s.as_ref().encode_wide().chain(Some(0)).collect()
}
fn index(icon: Icon) -> usize {
    logic::ALL.iter().position(|i| *i == icon).unwrap_or(3)
}
fn copy_text<const N: usize>(dst: &mut [u16; N], text: &str) {
    for (slot, unit) in dst.iter_mut().zip(text.encode_utf16().take(N - 1)) {
        *slot = unit;
    }
}

fn make_icon(icon: Icon, size: usize) -> Option<HICON> {
    let pixels = logic::render(icon, size);
    unsafe {
        let header = BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: size as i32,
            biHeight: -(size as i32),
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB,
            biSizeImage: 0,
            biXPelsPerMeter: 0,
            biYPelsPerMeter: 0,
            biClrUsed: 0,
            biClrImportant: 0,
        };
        let info = BITMAPINFO {
            bmiHeader: header,
            bmiColors: [std::mem::zeroed()],
        };
        let dc = GetDC(null_mut());
        let mut bits = null_mut();
        let color = CreateDIBSection(dc, &info, DIB_RGB_COLORS, &mut bits, null_mut(), 0);
        ReleaseDC(null_mut(), dc);
        if color.is_null() || bits.is_null() {
            return None;
        }
        std::ptr::copy_nonoverlapping(pixels.as_ptr(), bits.cast::<u8>(), pixels.len());
        let mask = CreateBitmap(size as i32, size as i32, 1, 1, null());
        let info = ICONINFO {
            fIcon: 1,
            xHotspot: 0,
            yHotspot: 0,
            hbmMask: mask,
            hbmColor: color,
        };
        let handle = CreateIconIndirect(&info);
        DeleteObject(color);
        if !mask.is_null() {
            DeleteObject(mask);
        }
        (!handle.is_null()).then_some(handle)
    }
}

fn data(hwnd: HWND) -> NOTIFYICONDATAW {
    // SAFETY: plain C struct for which all-zero bytes are a valid initial value.
    let mut nid: NOTIFYICONDATAW = unsafe { std::mem::zeroed() };
    nid.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
    nid.hWnd = hwnd;
    nid.uID = 1;
    nid
}

fn add_icon(hwnd: HWND, t: &mut Tray, icon: usize, tip: &str) {
    let mut nid = data(hwnd);
    nid.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP | NIF_SHOWTIP;
    nid.uCallbackMessage = CALLBACK;
    nid.hIcon = t.icons[icon];
    copy_text(&mut nid.szTip, tip);
    unsafe {
        if Shell_NotifyIconW(NIM_ADD, &nid) != 0 {
            nid.Anonymous.uVersion = NOTIFYICON_VERSION_4;
            Shell_NotifyIconW(NIM_SETVERSION, &nid);
            t.shown = Some((icon, tip.to_owned()));
        } else {
            t.shown = None; // Taskbar not ready yet; TaskbarCreated re-adds.
        }
    }
}

fn set_icon(hwnd: HWND, t: &mut Tray, icon: usize, tip: &str) {
    if t.shown
        .as_ref()
        .is_some_and(|(i, s)| *i == icon && s == tip)
    {
        return;
    }
    if t.shown.is_none() {
        return add_icon(hwnd, t, icon, tip);
    }
    let mut nid = data(hwnd);
    nid.uFlags = NIF_ICON | NIF_TIP | NIF_SHOWTIP;
    nid.hIcon = t.icons[icon];
    copy_text(&mut nid.szTip, tip);
    if unsafe { Shell_NotifyIconW(NIM_MODIFY, &nid) } != 0 {
        t.shown = Some((icon, tip.to_owned()));
    } else {
        add_icon(hwnd, t, icon, tip);
    }
}

fn remove_icon(hwnd: HWND) {
    let nid = data(hwnd);
    unsafe {
        Shell_NotifyIconW(NIM_DELETE, &nid);
    }
}

fn balloon(hwnd: HWND, t: &mut Tray, notice: &logic::Balloon) {
    let mut nid = data(hwnd);
    nid.uFlags = NIF_INFO;
    nid.dwInfoFlags = NIIF_INFO;
    copy_text(&mut nid.szInfoTitle, "Secblitz");
    copy_text(&mut nid.szInfo, &notice.text(t.lang));
    t.page = notice.page();
    unsafe {
        Shell_NotifyIconW(NIM_MODIFY, &nid);
    }
}

/// The last scam or dangerous block. The status file is read-only to the tray and names only those two kinds.
fn block_notice() -> Option<secblitz::filter::config::Notice> {
    use std::io::Read;
    let file = std::fs::File::open(secblitz::filter::config::status_path().ok()?).ok()?;
    let mut bytes = Vec::new();
    file.take(logic::NOTICE_LIMIT + 1)
        .read_to_end(&mut bytes)
        .ok()?;
    logic::block_notice(&bytes)
}

fn open_app(t: &mut Tray, page: Option<&str>) {
    if t.opened
        .is_some_and(|at| at.elapsed() < Duration::from_secs(2))
    {
        return; // One click can arrive as several notification messages.
    }
    t.opened = Some(Instant::now());
    let Ok(exe) = std::env::current_exe() else {
        return;
    };
    let file = wide(&exe);
    let dir = exe.parent().map(wide);
    let verb = wide("open");
    let arguments = page.map(|page| wide(format!("--open {page}")));
    unsafe {
        ShellExecuteW(
            null_mut(),
            verb.as_ptr(),
            file.as_ptr(),
            arguments.as_ref().map_or(null(), |a| a.as_ptr()),
            dir.as_ref().map_or(null(), |d| d.as_ptr()),
            SW_SHOWNORMAL,
        );
    }
}

fn refresh(hwnd: HWND, t: &mut Tray) {
    let now = status::read();
    let icon = index(logic::icon_for(now.as_ref()));
    let tip = logic::tooltip(t.lang, now.as_ref());
    set_icon(hwnd, t, icon, &tip);
    let notify = status::read_notify();
    if let Some(now) = now {
        let notice = t
            .last
            .as_ref()
            .and_then(|prev| logic::status_balloon(prev, &now, &notify));
        if let Some(notice) = notice {
            balloon(hwnd, t, &notice);
        }
        t.last = Some(now);
    }
    check_notice(hwnd, t);
}

/// A new block is warned about over the browser when one shows the site, and otherwise gets the
/// balloon (at most one every ten minutes).
fn check_notice(hwnd: HWND, t: &mut Tray) {
    let now = status::now();
    let allowed = status::read_notify().dangerous;
    let Some(fresh) = t.notices.observe_fresh(block_notice(), now, allowed) else {
        return;
    };
    if warn_logic::fresh(fresh.notice.at, now) {
        look_for_browser(hwnd, t.lang, fresh.notice);
    } else if fresh.spaced {
        show_blocked(hwnd, t, fresh.notice, now);
    }
}

fn show_blocked(hwnd: HWND, t: &mut Tray, notice: Notice, now: u64) {
    t.notices.announced(now);
    balloon(hwnd, t, &logic::Balloon::Blocked(notice));
}

/// Chromium's title is checked first because it costs nothing; the address bar, which a page
/// cannot change, then has to agree.
fn tab_shows(window: usize, site: &str, address_read: &mut Option<Instant>) -> bool {
    let Some(kind) = browser::browser_of(window) else {
        return false;
    };
    if kind == warn_logic::Browser::Chromium
        && !warn_logic::title_shows(&browser::title(window), site)
    {
        return false;
    }
    if address_read.is_some_and(|at| at.elapsed() < ADDRESS_EVERY) {
        return false;
    }
    *address_read = Some(Instant::now());
    browser::address(window, kind, ADDRESS_WAIT)
        .is_some_and(|address| warn_logic::address_shows(&address, site))
}

fn start_panel(lang: Lang, notice: &Notice, window: usize) -> bool {
    use std::os::windows::process::CommandExt;
    let (Some(words), Ok(exe)) = (
        warn_logic::start_arguments(notice.kind, &notice.site, window),
        std::env::current_exe(),
    ) else {
        return false;
    };
    let Ok(mut panel) = PANEL.lock() else {
        return false;
    };
    if let Some(mut old) = panel.take() {
        if matches!(old.child.try_wait(), Ok(None)) {
            if old.window == window && old.site == notice.site {
                *panel = Some(old);
                return true;
            }
            let _ = old.child.kill();
        }
        let _ = old.child.wait();
    }
    match std::process::Command::new(exe)
        .args(["--lang", lang.code()])
        .args(words)
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
    {
        Ok(child) => {
            *panel = Some(OpenPanel {
                child,
                window,
                site: notice.site.clone(),
            });
            true
        }
        Err(_) => false,
    }
}

/// Gives the browser a few seconds to show the blocked site, then either starts the warning
/// panel over it or hands the block back to the tray for the balloon. A newer block ends the
/// search for an older one.
fn look_for_browser(hwnd: HWND, lang: Lang, notice: Notice) {
    let hwnd = hwnd as usize;
    let Ok(mut searching) = SEARCHING.lock() else {
        return;
    };
    if searching
        .as_ref()
        .is_some_and(|(_, site)| *site == notice.site)
    {
        return;
    }
    let mine = NEWEST.fetch_add(1, Ordering::AcqRel) + 1;
    *searching = Some((mine, notice.site.clone()));
    drop(searching);
    let _ = std::thread::Builder::new()
        .name("browser-match".into())
        .spawn(move || {
            search_browser(hwnd, lang, notice, mine);
            if let Ok(mut searching) = SEARCHING.lock() {
                if searching.as_ref().is_some_and(|(id, _)| *id == mine) {
                    *searching = None;
                }
            }
        });
}

fn search_browser(hwnd: usize, lang: Lang, notice: Notice, mine: u64) {
    let until = Instant::now() + MATCH_FOR;
    let mut address_read = None;
    loop {
        if NEWEST.load(Ordering::Acquire) != mine {
            return;
        }
        let window = browser::foreground();
        if window != 0 && tab_shows(window, &notice.site, &mut address_read) {
            if NEWEST.load(Ordering::Acquire) == mine && start_panel(lang, &notice, window) {
                return;
            }
            break;
        }
        if Instant::now() >= until {
            break;
        }
        std::thread::sleep(MATCH_EVERY);
    }
    if let Ok(mut later) = LATER.lock() {
        later.push(notice);
        // SAFETY: posting to the tray window; a window that is gone just fails.
        unsafe { PostMessageW(hwnd as HWND, BALLOON_LATER, 0, 0) };
    }
}

/// Wakes the tray whenever something in the web protection folder is written, so a block is
/// noticed within a moment instead of at the next poll. The thread ends with the tray process.
fn watch_folder(hwnd: HWND) {
    let hwnd = hwnd as usize;
    let _ = std::thread::Builder::new()
        .name("folder-watch".into())
        .spawn(move || loop {
            let folder = secblitz::filter::config::status_path()
                .ok()
                .and_then(|path| path.parent().map(wide));
            // SAFETY: the folder name is a null-terminated wide string; the handle is closed below.
            let watch = folder.map(|folder| unsafe {
                FindFirstChangeNotificationW(
                    folder.as_ptr(),
                    0,
                    FILE_NOTIFY_CHANGE_LAST_WRITE | FILE_NOTIFY_CHANGE_FILE_NAME,
                )
            });
            match watch {
                Some(handle) if !handle.is_null() && handle as isize != -1 => {
                    // SAFETY: `handle` is the live notification handle created above.
                    unsafe {
                        while WaitForSingleObject(handle, INFINITE) == WAIT_OBJECT_0 {
                            if !CHANGE_QUEUED.swap(true, Ordering::AcqRel)
                                && PostMessageW(hwnd as HWND, FOLDER_CHANGED, 0, 0) == 0
                            {
                                CHANGE_QUEUED.store(false, Ordering::Release);
                            }
                            std::thread::sleep(Duration::from_millis(100));
                            if FindNextChangeNotification(handle) == 0 {
                                break;
                            }
                        }
                        FindCloseChangeNotification(handle);
                    }
                    std::thread::sleep(Duration::from_secs(5));
                }
                // The service has not made its folder yet: look again later.
                _ => std::thread::sleep(Duration::from_secs(30)),
            }
        });
}

fn quiesce_requested() -> bool {
    let name = wide(QUIESCE_EVENT);
    unsafe {
        let h = OpenEventW(SYNCHRONIZE | READ_CONTROL, 0, name.as_ptr());
        if h.is_null() {
            return false;
        }
        // Any local user can create a Global event; only honour one owned by
        // SYSTEM or Administrators (the updater).
        let mut owner = null_mut();
        let mut sd = null_mut();
        let rc = GetSecurityInfo(
            h,
            SE_KERNEL_OBJECT,
            OWNER_SECURITY_INFORMATION,
            &mut owner,
            null_mut(),
            null_mut(),
            null_mut(),
            &mut sd,
        );
        let trusted = rc == 0
            && !owner.is_null()
            && (IsWellKnownSid(owner, WinLocalSystemSid) != 0
                || IsWellKnownSid(owner, WinBuiltinAdministratorsSid) != 0);
        if !sd.is_null() {
            LocalFree(sd);
        }
        if !trusted {
            CloseHandle(h);
            return false;
        }
        let signalled = WaitForSingleObject(h, 0) == WAIT_OBJECT_0;
        CloseHandle(h);
        signalled
    }
}

fn menu(hwnd: HWND, lang: Lang) -> usize {
    unsafe {
        let menu = CreatePopupMenu();
        if menu.is_null() {
            return 0;
        }
        let open = wide(lang.t("Open Secblitz"));
        let check = wide(lang.t("Check now"));
        let quit = wide(lang.t("Quit"));
        AppendMenuW(menu, MF_STRING, ID_OPEN, open.as_ptr());
        AppendMenuW(menu, MF_STRING, ID_CHECK, check.as_ptr());
        AppendMenuW(menu, MF_SEPARATOR, 0, null());
        AppendMenuW(menu, MF_STRING, ID_QUIT, quit.as_ptr());
        let mut at = POINT { x: 0, y: 0 };
        GetCursorPos(&mut at);
        SetForegroundWindow(hwnd);
        let chosen = TrackPopupMenu(
            menu,
            TPM_RETURNCMD | TPM_RIGHTBUTTON | TPM_BOTTOMALIGN,
            at.x,
            at.y,
            0,
            hwnd,
            null(),
        );
        PostMessageW(hwnd, WM_NULL, 0, 0);
        DestroyMenu(menu);
        chosen as usize
    }
}

fn menu_choice(hwnd: HWND, chosen: usize) {
    match chosen {
        ID_OPEN | ID_CHECK => {
            with_tray(|t| open_app(t, None));
        }
        ID_QUIT => unsafe {
            DestroyWindow(hwnd);
        },
        _ => {}
    }
}

unsafe extern "system" fn proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        CALLBACK => {
            let event = (lparam & 0xFFFF) as u32;
            match event {
                NIN_BALLOONUSERCLICK => {
                    with_tray(|t| {
                        let page = t.page.take();
                        open_app(t, page);
                    });
                }
                WM_LBUTTONUP | NIN_SELECT => {
                    with_tray(|t| open_app(t, None));
                }
                WM_RBUTTONUP | WM_CONTEXTMENU => {
                    if let Some(lang) = with_tray(|t| t.lang) {
                        menu_choice(hwnd, menu(hwnd, lang));
                    }
                }
                _ => {}
            }
            0
        }
        FOLDER_CHANGED => {
            CHANGE_QUEUED.store(false, Ordering::Release);
            with_tray(|t| check_notice(hwnd, t));
            0
        }
        BALLOON_LATER => {
            let waiting = LATER.lock().map(|mut l| std::mem::take(&mut *l));
            for notice in waiting.unwrap_or_default() {
                with_tray(|t| {
                    let now = status::now();
                    if t.notices.spaced(now) {
                        show_blocked(hwnd, t, notice, now);
                    }
                });
            }
            0
        }
        WM_TIMER => {
            match wparam {
                POLL_TIMER => {
                    with_tray(|t| refresh(hwnd, t));
                }
                QUIESCE_TIMER if quiesce_requested() || crate::app::settings::tray_turned_off() => {
                    DestroyWindow(hwnd);
                }
                _ => {}
            }
            0
        }
        WM_QUERYENDSESSION => 1,
        WM_ENDSESSION => {
            if wparam != 0 {
                DestroyWindow(hwnd);
            }
            0
        }
        WM_CLOSE => {
            DestroyWindow(hwnd);
            0
        }
        WM_DESTROY => {
            KillTimer(hwnd, POLL_TIMER);
            KillTimer(hwnd, QUIESCE_TIMER);
            remove_icon(hwnd);
            PostQuitMessage(0);
            0
        }
        m => {
            let rebuilt = with_tray(|t| {
                if t.taskbar_created != m {
                    return false;
                }
                t.shown = None;
                refresh(hwnd, t);
                true
            });
            if rebuilt == Some(true) {
                return 0;
            }
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
    }
}

pub fn run(lang: Lang) -> Result<i32> {
    let mutex_name = wide(MUTEX);
    let mutex = unsafe { CreateMutexW(null(), 0, mutex_name.as_ptr()) };
    if mutex.is_null() {
        bail!("Cannot create tray mutex ({})", unsafe { GetLastError() });
    }
    if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
        unsafe { CloseHandle(mutex) };
        return Ok(0); // Already running in this session.
    }
    if quiesce_requested() {
        unsafe { CloseHandle(mutex) };
        return Ok(0);
    }
    let size = unsafe { GetSystemMetrics(SM_CXSMICON) }.clamp(16, 64) as usize;
    let mut icons = [null_mut(); 4];
    for (slot, icon) in icons.iter_mut().zip(logic::ALL) {
        match make_icon(icon, size) {
            Some(h) => *slot = h,
            None => bail!("Cannot create tray icons"),
        }
    }
    let class_name = wide("SecblitzTrayWindow");
    let title = wide("SecblitzTray");
    let taskbar = wide("TaskbarCreated");
    let hwnd = unsafe {
        let hinstance = GetModuleHandleW(null());
        // SAFETY: plain C struct for which all-zero bytes are a valid initial value.
        let mut class: WNDCLASSEXW = std::mem::zeroed();
        class.cbSize = std::mem::size_of::<WNDCLASSEXW>() as u32;
        class.lpfnWndProc = Some(proc);
        class.hInstance = hinstance;
        class.lpszClassName = class_name.as_ptr();
        if RegisterClassExW(&class) == 0 {
            bail!("Cannot register tray window");
        }
        TRAY.with(|cell| {
            *cell.borrow_mut() = Some(Tray {
                lang,
                icons,
                taskbar_created: RegisterWindowMessageW(taskbar.as_ptr()),
                last: None,
                shown: None,
                opened: None,
                notices: logic::Notices::default(),
                page: None,
            })
        });
        CreateWindowExW(
            WS_EX_TOOLWINDOW,
            class_name.as_ptr(),
            title.as_ptr(),
            WS_POPUP,
            0,
            0,
            0,
            0,
            null_mut(),
            null_mut(),
            hinstance,
            null(),
        )
    };
    if hwnd.is_null() {
        bail!("Cannot create tray window");
    }
    with_tray(|t| refresh(hwnd, t));
    watch_folder(hwnd);
    unsafe {
        SetTimer(hwnd, POLL_TIMER, POLL_EVERY, None);
        SetTimer(hwnd, QUIESCE_TIMER, QUIESCE_EVERY, None);
        // SAFETY: plain C struct for which all-zero bytes are a valid initial value.
        let mut msg: MSG = std::mem::zeroed();
        while GetMessageW(&mut msg, null_mut(), 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        with_tray(|t| {
            for icon in t.icons {
                DestroyIcon(icon);
            }
        });
        CloseHandle(mutex);
    }
    Ok(0)
}
