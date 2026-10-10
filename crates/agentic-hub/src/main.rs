// Hide the extra console window on Windows in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use agentic_hub_lib::headless;

fn main() {
    let argv: Vec<String> = std::env::args_os()
        .map(|a| a.to_string_lossy().into_owned())
        .collect();
    if let Some(args) = headless::ehub_args(&argv) {
        std::process::exit(headless::run(&args));
    }
    agentic_hub_lib::run();
}
