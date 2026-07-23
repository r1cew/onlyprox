pub mod builder;
pub mod checker;
pub mod fetcher;
pub mod models;
pub mod parser;
pub mod xray;

use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use slint::{ComponentHandle, Model, ModelRc, SharedString, VecModel};
use tokio::sync::{mpsc, Mutex};

use checker::run_pipeline;
use fetcher::fetch_subscription;
use models::{ProxyCandidate, TestStage, APP_DIR};
use parser::parse_subscription_feed;
use xray::{save_working_configs, XrayService};

slint::include_modules!();

const SOCKS_PORT: u16 = 10818;
const FEED_URL: &str = "https://raw.githubusercontent.com/igareck/vpn-configs-for-russia/refs/heads/main/BLACK_VLESS_RUS_mobile.txt";

// Состояния, передаваемые из фонового потока в UI
enum AppCommand {
    StartPipeline,
    ToggleConnect,
    SelectConfig(usize),
}

// Хранимый в потоке активный сервис Xray
struct AppState {
    working_configs: Vec<ProxyCandidate>,
    selected_index: Option<usize>,
    xray_service: Option<XrayService>,
    is_connected: bool,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            working_configs: Vec::new(),
            selected_index: None,
            xray_service: None,
            is_connected: false,
        }
    }
}

/// Единая точка запуска GUI-приложения
pub fn run_app() -> Result<(), slint::PlatformError> {
    let ui = MainWindow::new()?;

    // Канал связи между Slint UI и фоновым Tokio Worker
    let (tx, mut rx) = mpsc::channel::<AppCommand>(32);

    // Внутреннее состояние приложения (живет в фоновом Tokio task)
    let state = Arc::new(Mutex::new(AppState::default()));

    // --- 1. РЕГИСТРАЦИЯ КОЛЛБЭКОВ SLINT ---

    // Запуск воронки / обновление
    let tx_search = tx.clone();
    ui.on_start_search(move || {
        let _ = tx_search.try_send(AppCommand::StartPipeline);
    });

    let tx_refresh = tx.clone();
    ui.on_refresh_configs(move || {
        let _ = tx_refresh.try_send(AppCommand::StartPipeline);
    });

    // Включение / Выключение VPN
    let tx_toggle = tx.clone();
    ui.on_toggle_vpn(move || {
        let _ = tx_toggle.try_send(AppCommand::ToggleConnect);
    });

    // Выбор конфига в списке
    let tx_select = tx.clone();
    ui.on_select_config(move |index| {
        let _ = tx_select.try_send(AppCommand::SelectConfig(index as usize));
    });

    // --- 2. ЗАПУСК ФОНОВОГО WORKER (TOKIO RUNTIME) ---
    let ui_handle = ui.as_weak();
    let state_clone = Arc::clone(&state);

    std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async move {
            while let Some(cmd) = rx.recv().await {
                let ui_weak = ui_handle.clone();
                let state = Arc::clone(&state_clone);

                match cmd {
                    AppCommand::StartPipeline => {
                        handle_pipeline(ui_weak, state).await;
                    }
                    AppCommand::ToggleConnect => {
                        handle_toggle_vpn(ui_weak, state).await;
                    }
                    AppCommand::SelectConfig(index) => {
                        handle_select_config(ui_weak, state, index).await;
                    }
                }
            }
        });
    });

    // Автоматический запуск поиска при старте приложения
    let _ = tx.try_send(AppCommand::StartPipeline);

    ui.run()
}

// ============================================================================
//   ХЭНДЛЕРЫ ЛОГИКИ (РАБОТАЮТ В ФОНЕ)
// ============================================================================

/// Логика прохождения воронки проверок
async fn handle_pipeline(ui_weak: slint::Weak<MainWindow>, state: Arc<Mutex<AppState>>) {
    // 1. Показываем статус "Загрузка" в UI
    let _ = slint::invoke_from_event_loop({
        let ui_weak = ui_weak.clone();
        move || {
            if let Some(ui) = ui_weak.upgrade() {
                ui.set_is_searching(true);
                ui.set_search_progress(0.1);
                ui.set_search_stage(SharedString::from("Скачивание подписки..."));
            }
        }
    });

    // 2. Скачивание и парсинг
    let raw_text = match fetch_subscription(FEED_URL).await {
        Ok(t) => t,
        Err(e) => {
            eprintln!("Ошибка загрузки: {}", e);
            reset_ui_search(ui_weak);
            return;
        }
    };

    let _ = slint::invoke_from_event_loop({
        let ui_weak = ui_weak.clone();
        move || {
            if let Some(ui) = ui_weak.upgrade() {
                ui.set_search_progress(0.25);
                ui.set_search_stage(SharedString::from("Парсинг ссылок..."));
            }
        }
    });

    let mut links = parse_subscription_feed(&raw_text);
    links.truncate(150);

    // 3. Конфигурируем этапы воронки
    let test_stages = vec![
        TestStage {
            name: "Этап 1: Быстрый экспресс-пинг".to_string(),
            speedtest: false,
            min_speed_kbps: 0.0,
            threads: 80,
            repeats: 1,
            interval_sec: 0,
        },
        TestStage {
            name: "Этап 2: Проверка канала".to_string(),
            speedtest: true,
            min_speed_kbps: 100.0,
            threads: 40,
            repeats: 1,
            interval_sec: 2,
        },
        TestStage {
            name: "Этап 3: Замер стабильности".to_string(),
            speedtest: true,
            min_speed_kbps: 1500.0,
            threads: 16,
            repeats: 1,
            interval_sec: 5,
        },
    ];

    let _ = slint::invoke_from_event_loop({
        let ui_weak = ui_weak.clone();
        move || {
            if let Some(ui) = ui_weak.upgrade() {
                ui.set_search_progress(0.40);
                ui.set_search_stage(SharedString::from("Тестирование серверов..."));
            }
        }
    });

    // 4. Запускаем воронку
    let working_configs = run_pipeline(links, test_stages).await;
    let _ = save_working_configs(&working_configs).await;

    // 5. Обновляем состояние приложения и UI
    let mut lock = state.lock().await;
    lock.working_configs = working_configs.clone();
    lock.selected_index = if !working_configs.is_empty() { Some(0) } else { None };

    // Формируем список моделей для Slint
    let slint_configs: Vec<ServerConfig> = working_configs
        .iter()
        .enumerate()
        .map(|(i, candidate)| {
            let speed_mb = candidate.last_speed_kbps / 1024.0;
            ServerConfig {
                name: SharedString::from(candidate.link.remark()),
                flag: SharedString::from("🌐"), // Можно парсить страну из remark
                ping: SharedString::from(format!("{} ms", candidate.last_latency)),
                speed: SharedString::from(format!("{:.1} MB/s", speed_mb)),
                selected: i == 0,
            }
        })
        .collect();

    let _ = slint::invoke_from_event_loop({
        let ui_weak = ui_weak.clone();
        move || {
            if let Some(ui) = ui_weak.upgrade() {
                let model = Rc::new(VecModel::from(slint_configs));
                ui.set_configs(ModelRc::from(model));
                ui.set_search_progress(1.0);
                ui.set_is_searching(false);
            }
        }
    });
}

/// Переключение Вкл/Выкл VPN
async fn handle_toggle_vpn(ui_weak: slint::Weak<MainWindow>, state: Arc<Mutex<AppState>>) {
    let mut lock = state.lock().await;

    if lock.is_connected {
        // Остановка Xray
        if let Some(ref mut service) = lock.xray_service {
            service.stop().await;
        }
        lock.xray_service = None;
        lock.is_connected = false;

        let _ = slint::invoke_from_event_loop(move || {
            if let Some(ui) = ui_weak.upgrade() {
                ui.set_is_connected(false);
            }
        });
    } else {
        // Подключение выбранного конфига
        let selected_idx = match lock.selected_index {
            Some(i) => i,
            None => return,
        };

        if let Some(candidate) = lock.working_configs.get(selected_idx) {
            let mut service = XrayService::new(SOCKS_PORT);
            let final_config = XrayService::build_config(candidate, SOCKS_PORT);
            let config_file = APP_DIR.join("config").join("xray.json");

            if XrayService::write_config_to_file(&final_config, &config_file).await.is_ok() {
                if service.spawn_process(&config_file, false).is_ok() {
                    service.set_system_proxy(true);
                    lock.xray_service = Some(service);
                    lock.is_connected = true;

                    let _ = slint::invoke_from_event_loop(move || {
                        if let Some(ui) = ui_weak.upgrade() {
                            ui.set_is_connected(true);
                        }
                    });
                }
            }
        }
    }
}

/// Выбор конфигурации из списка UI
async fn handle_select_config(
    ui_weak: slint::Weak<MainWindow>,
    state: Arc<Mutex<AppState>>,
    index: usize,
) {
    let mut lock = state.lock().await;
    if index >= lock.working_configs.len() {
        return;
    }

    lock.selected_index = Some(index);

    // Подсветка активной строки в UI
    let _ = slint::invoke_from_event_loop(move || {
        if let Some(ui) = ui_weak.upgrade() {
            let model = ui.get_configs();
            for i in 0..model.row_count() {
                if let Some(mut cfg) = model.row_data(i) {
                    cfg.selected = i == index;
                    model.set_row_data(i, cfg);
                }
            }
        }
    });
}

fn reset_ui_search(ui_weak: slint::Weak<MainWindow>) {
    let _ = slint::invoke_from_event_loop(move || {
        if let Some(ui) = ui_weak.upgrade() {
            ui.set_is_searching(false);
        }
    });
}