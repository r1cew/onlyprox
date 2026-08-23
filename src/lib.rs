pub mod app;
pub mod backend;
pub mod builder;
pub mod checker;
pub mod fetcher;
pub mod models;
pub mod parser;
pub mod sources;
pub mod xray;

#[cfg(target_os = "linux")]
pub mod desktop;

/// Иконка приложения, встраиваемая в бинарник, используется как для
/// значка окна/таскбара (через `eframe`/`winit`), так и для установки
/// в системные каталоги иконок на Linux (см. `desktop.rs`).
pub fn load_icon() -> egui::IconData {
    let bytes = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/shield.png"));
    let image = image::load_from_memory(bytes)
        .expect("встроенная иконка повреждена")
        .into_rgba8();
    let (width, height) = image.dimensions();
    egui::IconData { rgba: image.into_raw(), width, height }
}
