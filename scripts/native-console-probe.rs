use std::io::IsTerminal;
use std::ffi::c_void;
#[link(name="kernel32")]
extern "system" {
    fn GetStdHandle(which: u32) -> *mut c_void;
    fn GetConsoleMode(handle: *mut c_void, mode: *mut u32) -> i32;
    fn GetFileType(handle: *mut c_void) -> u32;
}
fn main() {
    let mut text = format!("Rust terminal input={} output={} error={}\n", std::io::stdin().is_terminal(), std::io::stdout().is_terminal(), std::io::stderr().is_terminal());
    for n in [-10i32,-11,-12] {
        let h = unsafe { GetStdHandle(n as u32) };
        let mut mode=0;
        let ok=unsafe { GetConsoleMode(h,&mut mode) };
        text.push_str(&format!("stdio {n}: console_ok={ok}, mode={mode}, type={}\n",unsafe{GetFileType(h)}));
    }
    std::fs::write(r"C:\Windows\Temp\SecblitzV060CandidateValidation\Results\console-handles.txt", &text).unwrap();
    println!("{text}");
}
