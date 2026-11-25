#![windows_subsystem = "windows"]

mod gui;
mod tray;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("--tray") => tray::run(),
        // Elevated helper that installs Equalizer APO on devices (see apo_setup).
        Some("--apo-setup") => equinox::apo_setup::elevated_main(&args[1..]),
        // `--page mic|automation|settings|devices` opens on that page (devices: the picker).
        Some("--page") => gui::run(args.get(1).map(String::as_str), false),
        // Started by the updater: the old window is still closing.
        Some(equinox::update::RESTART_FLAG) => gui::run(None, true),
        _ => gui::run(None, false),
    }
}
