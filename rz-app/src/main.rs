//! RazerControl —— 雷蛇鼠标原生驱动（Rust + egui）。
//!
//! 协议逆向自 C:\Program Files\Razer\RazerAppEngine（Synapse 4）的传输层与真机探测。

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod art;
mod backend;
mod cache;
mod config;
mod hook;
mod macros;
mod json;
mod pages;
mod photo;
mod system;
mod theme;
mod tray;
mod widgets;

use egui::ViewportBuilder;

fn main() {
    let mut viewport = ViewportBuilder::default()
        .with_inner_size([1120.0, 720.0])
        .with_min_inner_size([960.0, 620.0])
        .with_decorations(false)
        .with_transparent(false)
        .with_title("RazerControl");
    if let Some(icon) = tray::window_icon() {
        viewport = viewport.with_icon(icon);
    }

    let options = eframe::NativeOptions {
        viewport,
        centered: true,
        ..Default::default()
    };

    let _ = eframe::run_native(
        "RazerControl",
        options,
        Box::new(|cc| Ok(Box::new(app::App::new(cc)))),
    );
}
