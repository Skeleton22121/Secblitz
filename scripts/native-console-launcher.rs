//! Test-only adapter: cmd opens CONOUT$ write-only, which fails GetConsoleMode.
//! Give the reviewed child read/write console handles, like an actual terminal.
use std::{ffi::c_void, fs::File, os::windows::io::{AsRawHandle, FromRawHandle}, process::{Command, Stdio}};
#[link(name="kernel32")]
extern "system" {
    fn CreateFileW(path: *const u16, access: u32, share: u32, sa: *const c_void, creation: u32, flags: u32, template: *mut c_void) -> *mut c_void;
    fn GetConsoleMode(handle: *mut c_void, mode: *mut u32) -> i32;
}
fn device(name: &str) -> File {
    let wide: Vec<u16> = name.encode_utf16().chain(Some(0)).collect();
    let handle = unsafe { CreateFileW(wide.as_ptr(), 0xc0000000, 3, std::ptr::null(), 3, 0, std::ptr::null_mut()) };
    assert_ne!(handle as isize, -1, "console device unavailable");
    unsafe { File::from_raw_handle(handle) }
}
fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let exe = args.first().expect("missing test image");
    let root = if exe.starts_with(r"C:\Windows\Temp\SecblitzV060UiFinal\") { r"C:\Windows\Temp\SecblitzV060UiFinal" } else { r"C:\Windows\Temp\SecblitzV060CandidateValidation" };
    assert!(exe.starts_with(&format!("{root}\\")));
    let tail = &args[1..];
    let allowed = if exe.ends_with("\\secblitz.exe") {
        tail == ["--lang", "en", "--no-animation", "guide"]
    } else if exe.ends_with("\\native-cli.exe") {
        tail.len() == 5 && tail[0] == "--ignored" && tail[1] == "--exact"
            && matches!(tail[2].as_str(), "guided::tests::native_guided_flow_probe" | "menu::tests::native_scene_probe" | "menu::tests::native_no_animation_probe" | "menu::tests::native_panic_probe" | "menu::tests::native_error_probe" | "menu::tests::native_cancel_probe" | "menu::tests::native_windows_mode_and_page_keys" | "menu::tests::native_resize_consent_probe" | "guided::tests::native_navigation_probe")
            && tail[3] == "--nocapture" && tail[4] == "--test-threads=1"
    } else { exe.ends_with("\\console-probe.exe") && tail.is_empty() };
    assert!(allowed, "unreviewed console command");
    let input = device("CONIN$");
    let output = device("CONOUT$");
    let modes = || {
        [&input, &output, &output].map(|file| {
            let mut flags=0;
            assert_ne!(unsafe { GetConsoleMode(file.as_raw_handle(), &mut flags) },0);
            flags
        })
    };
    let before = modes();
    let status = Command::new(exe).args(tail)
        .stdin(Stdio::from(input.try_clone().unwrap())).stdout(Stdio::from(output.try_clone().unwrap()))
        .stderr(Stdio::from(output.try_clone().unwrap())).status().unwrap();
    let after=modes();
    let code=status.code().unwrap_or(99);
    // Command identity is an allowlisted fixture name, never an account/secret.
    let label=if exe.ends_with("\\secblitz.exe") {"release-guide"}else if tail.len()>2 {tail[2].as_str()}else{"console-probe"};
    let proof=format!("{{\"label\":\"{label}\",\"launcher_pid\":{},\"before\":{before:?},\"after\":{after:?},\"equal\":{},\"child_exit\":{code}}}",std::process::id(),before==after);
    std::fs::write(format!("{root}\\Results\\console-mode-{}.json",std::process::id()),proof).unwrap();
    std::process::exit(if code==0 && before!=after {98}else{code});
}
