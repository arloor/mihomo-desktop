#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod config;
mod geo;
mod mihomo;
mod runtime;
mod system;
mod ui;

use gpui_kit::component::TitleBar;
use gpui_kit::*;
use system::service_host::{self, CliMode};
use system::tray;
use ui::MihomoApp;

fn main() {
    // Handle headless Windows Service & Setup CLI modes before initializing GPUI/GPU context.
    match service_host::parse_cli_args() {
        CliMode::RunService(args) => {
            if let Err(err) = service_host::run_windows_service(args) {
                eprintln!("mihomo-desktop --service failed: {err:#}");
                std::process::exit(1);
            }
            return;
        }
        CliMode::InstallService(args) => {
            if let Err(err) = service_host::install_or_update_service(&args) {
                eprintln!("mihomo-desktop --install-service failed: {err:#}");
                std::process::exit(1);
            }
            return;
        }
        CliMode::UninstallService { service_name } => {
            if let Err(err) = service_host::uninstall_service(&service_name) {
                eprintln!("mihomo-desktop --uninstall-service failed: {err:#}");
                std::process::exit(1);
            }
            return;
        }
        CliMode::Gui => {}
    }

    // Prevent duplicate GUI instances; wake existing instance (including hidden tray window) if already running.
    let Some(_instance_guard) = tray::acquire_single_instance() else {
        return;
    };

    let start_minimized = std::env::args().any(|a| a == "--minimized");
    let start_hidden = start_minimized && config::load_settings().resident_mode;

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    gpui_kit::application()
        .with_assets(gpui_kit::assets::AllAssets)
        .run(move |cx| {
            gpui_kit::init(cx);

            let bounds = Bounds::centered(None, size(px(1180.), px(760.)), cx);
            let mut titlebar = TitleBar::title_bar_options();
            titlebar.title = Some("Mihomo Desktop".into());

            let options = WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: Some(titlebar),
                window_min_size: Some(size(px(900.), px(600.))),
                show: !start_hidden,
                focus: !start_hidden,
                ..TitleBar::window_options()
            };

            gpui_kit::open_window(options, cx, |window, cx| {
                cx.new(|cx| MihomoApp::new(window, cx))
            })
            .expect("failed to open Mihomo Desktop window");
        });
}
