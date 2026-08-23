#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() -> eframe::Result<()> {
    // .desktop-файл + иконка в системных каталогах (только Linux).
    // Делаем это до создания окна — это не блокирует старт GUI, а
    // ошибки внутри намеренно проглатываются (не критично для запуска).
    #[cfg(target_os = "linux")]
    onlyprox::desktop::setup_desktop_integration();
 
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("OnlyProx")
            .with_app_id("onlyprox")
            .with_inner_size([900.0, 640.0])
            .with_min_inner_size([700.0, 480.0])
            // Это и есть настоящая иконка окна/таскбара: eframe передаёт её
            // в winit, а тот выставляет _NET_WM_ICON на X11 и app icon на
            // Wayland — независимо от того, подхватил ли WM .desktop-файл.
            .with_icon(onlyprox::load_icon()),
        ..Default::default()
    };

    // Первый аргумент — app_id: он же используется winit как WM_CLASS на
    // X11 / app_id на Wayland, поэтому должен совпадать со
    // StartupWMClass в onlyprox.desktop.
    eframe::run_native(
        "onlyprox",
        options,
        Box::new(|cc| Box::new(onlyprox::app::OnlyProxApp::new(cc))),
    )
}
