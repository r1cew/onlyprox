//! Интеграция с рабочим столом Linux: устанавливает .desktop-файл и иконку
//! приложения в стандартные XDG-каталоги, чтобы значок корректно
//! показывался в меню приложений и панели задач.
//!
//! Иконка в самом окне (и, соответственно, в таскбаре/альт-табе) задаётся
//! отдельно и напрямую через `eframe`/`winit` (см. `crate::load_icon` и
//! `main.rs`) — это единственный способ, который реально работает во всех
//! desktop environment, независимо от того, нашёл ли WM ваш .desktop-файл.
//! Установка .desktop-файла ниже нужна дополнительно: для значка в меню
//! приложений, при закреплении в панели задач и для `StartupWMClass`.

use std::env;
use std::fs;
use std::path::Path;
use std::process::Command;

/// PNG-иконка приложения, встроенная в бинарник на этапе сборки.
/// Путь берётся относительно Cargo.toml, поэтому работает на любой машине,
/// а не только у автора репозитория.
const ICON_DATA: &[u8] = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/shield.png"));

const APP_ID: &str = "onlyprox";

pub fn setup_desktop_integration() {
    let Some(home) = env::var_os("HOME") else {
        return;
    };
    let home = home.to_string_lossy().into_owned();

    let Ok(exe_path) = env::current_exe() else {
        return;
    };
    let exe_path = exe_path.to_string_lossy().into_owned();

    write_desktop_file(&home, &exe_path);
    write_icon(&home);
    refresh_caches(&home);
}

fn write_desktop_file(home: &str, exe_path: &str) {
    let desktop_file_path = format!("{home}/.local/share/applications/{APP_ID}.desktop");

    // Перезаписываем каждый запуск, чтобы путь к Exec всегда указывал на
    // актуальный бинарник (например, после переноса/обновления AppImage).
    let desktop_content = format!(
        "[Desktop Entry]\n\
         Type=Application\n\
         Name=OnlyProx\n\
         Comment=VPN клиент с автопоиском рабочих конфигов\n\
         Exec={exe_path}\n\
         Icon={APP_ID}\n\
         Terminal=false\n\
         Categories=Network;Security;\n\
         StartupWMClass={APP_ID}\n"
    );

    if let Some(parent) = Path::new(&desktop_file_path).parent() {
        let _ = fs::create_dir_all(parent);
    }
    let _ = fs::write(&desktop_file_path, desktop_content);
}

fn write_icon(home: &str) {
    let icon_dir = format!("{home}/.local/share/icons/hicolor/256x256/apps");
    let _ = fs::create_dir_all(&icon_dir);
    let _ = fs::write(format!("{icon_dir}/{APP_ID}.png"), ICON_DATA);
}

fn refresh_caches(home: &str) {
    let _ = Command::new("gtk-update-icon-cache")
        .args(["-f", "-t", &format!("{home}/.local/share/icons/hicolor")])
        .output();

    // Обновляем кэш меню приложений KDE/GNOME, если утилиты присутствуют.
    let _ = Command::new("kbuildsycoca6").arg("--noincremental").output();
    let _ = Command::new("update-desktop-database")
        .arg(format!("{home}/.local/share/applications"))
        .output();
}
