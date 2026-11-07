#![cfg_attr(windows, windows_subsystem = "windows")]
mod forward;
fn main() {
    forward::run("wave-app", false);
}
