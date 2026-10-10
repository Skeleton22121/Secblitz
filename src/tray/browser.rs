//! Windows side of the browser warning: whether a window is a browser, what its tab shows, where
//! it is on screen, and the two keys sent to it.
use super::warn_logic::{self, Browser, Rect};
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};
use windows_sys::Win32::{
    Foundation::{CloseHandle, HWND, RECT},
    Graphics::{
        Dwm::{DwmGetWindowAttribute, DWMWA_EXTENDED_FRAME_BOUNDS},
        Gdi::{GetMonitorInfoW, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTONEAREST},
    },
    System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_QUERY_LIMITED_INFORMATION,
    },
    UI::{
        HiDpi::GetDpiForWindow,
        Input::KeyboardAndMouse::{
            GetAsyncKeyState, SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT,
            KEYEVENTF_EXTENDEDKEY, KEYEVENTF_KEYUP, VK_ESCAPE, VK_F5, VK_LEFT, VK_MENU,
        },
        WindowsAndMessaging::{
            GetAncestor, GetForegroundWindow, GetWindowRect, GetWindowTextW,
            GetWindowThreadProcessId, IsIconic, IsWindow, IsWindowVisible, SetForegroundWindow,
            ShowWindow, GA_ROOT, SW_RESTORE,
        },
    },
};

pub fn handle(window: usize) -> HWND {
    window as HWND
}

pub fn foreground() -> usize {
    unsafe { GetForegroundWindow() as usize }
}

fn image_name(window: usize) -> Option<String> {
    let mut pid = 0;
    // SAFETY: plain queries about a window handle that may be stale; stale handles just fail.
    unsafe {
        GetWindowThreadProcessId(handle(window), &mut pid);
        if pid == 0 {
            return None;
        }
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if process.is_null() {
            return None;
        }
        let mut buffer = [0u16; 1024];
        let mut len = buffer.len() as u32;
        let ok = QueryFullProcessImageNameW(process, 0, buffer.as_mut_ptr(), &mut len);
        CloseHandle(process);
        (ok != 0).then(|| String::from_utf16_lossy(&buffer[..len as usize]))
    }
}

/// The browser that owns this window, when it is a visible, normal top-level window.
pub fn browser_of(window: usize) -> Option<Browser> {
    let hwnd = handle(window);
    // SAFETY: plain queries about a window handle that may be stale.
    let usable = unsafe {
        IsWindow(hwnd) != 0
            && IsWindowVisible(hwnd) != 0
            && IsIconic(hwnd) == 0
            && GetAncestor(hwnd, GA_ROOT) == hwnd
    };
    if !usable {
        return None;
    }
    warn_logic::browser_of_image(&image_name(window)?)
}

/// True while the window exists and is shown (not minimised).
pub fn is_shown(window: usize) -> bool {
    let hwnd = handle(window);
    // SAFETY: plain queries about a window handle that may be stale.
    unsafe { IsWindow(hwnd) != 0 && IsWindowVisible(hwnd) != 0 && IsIconic(hwnd) == 0 }
}

pub fn title(window: usize) -> String {
    let mut buffer = [0u16; 512];
    // SAFETY: the buffer length passed is the real one.
    let len = unsafe { GetWindowTextW(handle(window), buffer.as_mut_ptr(), buffer.len() as i32) };
    String::from_utf16_lossy(&buffer[..len.clamp(0, 511) as usize])
}

fn rect(r: RECT) -> Rect {
    Rect {
        left: r.left,
        top: r.top,
        right: r.right,
        bottom: r.bottom,
    }
}

/// The visible frame of the window, without the invisible resize border Windows adds.
pub fn bounds(window: usize) -> Option<Rect> {
    let hwnd = handle(window);
    // SAFETY: `r` is a RECT of exactly the size passed.
    unsafe {
        let mut r: RECT = std::mem::zeroed();
        let hr = DwmGetWindowAttribute(
            hwnd,
            DWMWA_EXTENDED_FRAME_BOUNDS as u32,
            (&raw mut r).cast(),
            std::mem::size_of::<RECT>() as u32,
        );
        if hr != 0 && GetWindowRect(hwnd, &mut r) == 0 {
            return None;
        }
        Some(rect(r))
    }
}

pub fn dpi(window: usize) -> u32 {
    // SAFETY: plain query; zero for a bad handle is treated as 100%.
    unsafe { GetDpiForWindow(handle(window)) }
}

/// The part of the screen the window is on that taskbars leave free.
pub fn work_area(window: usize) -> Option<Rect> {
    // SAFETY: `info` is a MONITORINFO with its size set.
    unsafe {
        let monitor = MonitorFromWindow(handle(window), MONITOR_DEFAULTTONEAREST);
        if monitor.is_null() {
            return None;
        }
        let mut info: MONITORINFO = std::mem::zeroed();
        info.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
        (GetMonitorInfoW(monitor, &mut info) != 0).then(|| rect(info.rcWork))
    }
}

/// Down now, or pressed since the last look: a quick tap falls between two checks.
pub fn escape_down() -> bool {
    // SAFETY: plain query of the key state.
    unsafe { GetAsyncKeyState(i32::from(VK_ESCAPE)) as u16 & 0x8001 != 0 }
}

/// Puts the browser in front and waits briefly until Windows agrees, so the keys that follow
/// reach it and nothing else.
pub fn bring_to_front(window: usize) -> bool {
    let hwnd = handle(window);
    // SAFETY: plain window calls on a handle that may be stale.
    unsafe {
        if IsWindow(hwnd) == 0 {
            return false;
        }
        if IsIconic(hwnd) != 0 {
            ShowWindow(hwnd, SW_RESTORE);
        }
        SetForegroundWindow(hwnd);
    }
    let until = Instant::now() + Duration::from_millis(600);
    while Instant::now() < until {
        if foreground() == window {
            return true;
        }
        thread::sleep(Duration::from_millis(30));
    }
    foreground() == window
}

fn key(code: u16, up: bool) -> INPUT {
    let extended = if code == VK_LEFT {
        KEYEVENTF_EXTENDEDKEY
    } else {
        0
    };
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: code,
                wScan: 0,
                dwFlags: extended | if up { KEYEVENTF_KEYUP } else { 0 },
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

fn send(keys: &[INPUT]) -> bool {
    // SAFETY: the pointer and count describe the slice.
    unsafe {
        SendInput(
            keys.len() as u32,
            keys.as_ptr(),
            std::mem::size_of::<INPUT>() as i32,
        ) == keys.len() as u32
    }
}

/// Keys go only to the window that is in front, and only when it is still a browser: a closed
/// window's handle can be reused by another program.
fn press(window: usize, keys: &[INPUT]) -> bool {
    browser_of(window).is_some() && bring_to_front(window) && foreground() == window && send(keys)
}

/// Alt+Left, the browser's own "back".
pub fn go_back(window: usize) -> bool {
    press(
        window,
        &[
            key(VK_MENU, false),
            key(VK_LEFT, false),
            key(VK_LEFT, true),
            key(VK_MENU, true),
        ],
    )
}

pub fn reload(window: usize) -> bool {
    press(window, &[key(VK_F5, false), key(VK_F5, true)])
}

/// The browser's address bar, read through UI Automation on a thread of its own that is given up
/// on after `limit`. Firefox titles its error page in the person's language, and any page can
/// set a Chromium title, so only the address bar shows what the tab really is.
pub fn address(window: usize, browser: Browser, limit: Duration) -> Option<String> {
    // A read stuck inside Firefox must not pile up more threads behind it.
    static READING: AtomicBool = AtomicBool::new(false);
    if READING.swap(true, Ordering::AcqRel) {
        return None;
    }
    let (tx, rx) = mpsc::channel();
    let started = thread::Builder::new()
        .name("address-bar".into())
        .spawn(move || {
            let _ = tx.send(read_address(window, browser));
            READING.store(false, Ordering::Release);
        });
    if started.is_err() {
        READING.store(false, Ordering::Release);
        return None;
    }
    rx.recv_timeout(limit).ok().flatten()
}

fn read_address(window: usize, browser: Browser) -> Option<String> {
    use windows::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_MULTITHREADED};
    struct Com;
    impl Drop for Com {
        fn drop(&mut self) {
            // SAFETY: balances the successful initialisation below.
            unsafe { CoUninitialize() };
        }
    }
    // SAFETY: initialises COM for this thread only; every COM object below is released before
    // `Com` is dropped because it lives in `address_bar`.
    unsafe {
        if CoInitializeEx(None, COINIT_MULTITHREADED).is_err() {
            return None;
        }
    }
    let _com = Com;
    address_bar(window, browser)
}

fn address_bar(window: usize, browser: Browser) -> Option<String> {
    use windows::{
        core::VARIANT,
        Win32::{
            Foundation::HWND,
            System::Com::{CoCreateInstance, CLSCTX_INPROC_SERVER},
            UI::Accessibility::{
                CUIAutomation, IUIAutomation, IUIAutomationValuePattern, TreeScope_Descendants,
                UIA_AutomationIdPropertyId, UIA_ClassNamePropertyId, UIA_ValuePatternId,
            },
        },
    };
    // SAFETY: plain COM calls on interfaces created here.
    unsafe {
        let automation: IUIAutomation =
            CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER).ok()?;
        let root = automation.ElementFromHandle(HWND(window as *mut _)).ok()?;
        let wanted = match browser {
            Browser::Firefox => automation.CreatePropertyCondition(
                UIA_AutomationIdPropertyId,
                &VARIANT::from("urlbar-input"),
            ),
            // Brave gives its address bar a class name of its own.
            Browser::Chromium => automation
                .CreatePropertyCondition(
                    UIA_ClassNamePropertyId,
                    &VARIANT::from("OmniboxViewViews"),
                )
                .and_then(|chromium| {
                    let brave = automation.CreatePropertyCondition(
                        UIA_ClassNamePropertyId,
                        &VARIANT::from("BraveOmniboxViewViews"),
                    )?;
                    automation.CreateOrCondition(&chromium, &brave)
                }),
        }
        .ok()?;
        // The toolbar comes before the page in the tree, so a page element made to look like the
        // address bar is never the first match.
        let bar = root.FindFirst(TreeScope_Descendants, &wanted).ok()?;
        let value: IUIAutomationValuePattern = bar.GetCurrentPatternAs(UIA_ValuePatternId).ok()?;
        Some(value.CurrentValue().ok()?.to_string())
    }
}
