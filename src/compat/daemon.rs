#![cfg_attr(windows, windows_subsystem = "windows")]
mod forward;
fn main() {
    // A direct legacy launch must respect the same cutover boundary as CLI autostart.
    for _ in 0..40 {
        match cys::rename_handover::autostart_blocked(&cys::socket_path()) {
            Ok(false) => forward::run("waved", true),
            Ok(true) => std::thread::sleep(std::time::Duration::from_millis(250)),
            Err(error) => forward::fail(error),
        }
    }
    forward::fail("업데이트 전환 대기 중입니다. 잠시 후 다시 실행하세요.");
}
