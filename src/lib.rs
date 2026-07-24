pub mod builder;
pub mod checker;
pub mod fetcher;
pub mod models;
pub mod parser;
pub mod xray;

use std::rc::Rc;
use std::sync::Arc;

use slint::{ComponentHandle, ModelRc, SharedString, VecModel};
use tokio::sync::{mpsc, Mutex};

use checker::run_pipeline;
use fetcher::fetch_subscription;
use models::{ProxyCandidate, TestStage, APP_DIR};
use parser::parse_subscription_feed;
use xray::{save_working_configs, XrayService};

slint::include_modules!();

const SOCKS_PORT: u16 = 10818;
const FEED_URL: &str = "https://raw.githubusercontent.com/igareck/vpn-configs-for-russia/refs/heads/main/BLACK_VLESS_RUS_mobile.txt";


enum AppCommand {
    LoadSavedConfigs,
    StartPipeline,
    ToggleConnect,
    SelectConfig(usize),
    Shutdown, // Добавили команду завершения
}

struct AppState {
    working_configs: Vec<ProxyCandidate>,
    selected_index: Option<usize>,
    connected_index: Option<usize>,
    xray_service: Option<XrayService>,
    is_connected: bool,
}

impl AppState {
    /// Полный сброс VPN и корректная остановка Xray
    pub async fn stop_vpn(&mut self) {
        if let Some(ref mut service) = self.xray_service {
            // Внутри service.stop() также должен быть сброс системного прокси
            service.set_system_proxy(false);
            service.stop().await;
        }
        self.xray_service = None;
        self.is_connected = false;
        self.connected_index = None;
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            working_configs: Vec::new(),
            selected_index: None,
            connected_index: None,
            xray_service: None,
            is_connected: false,
        }
    }
}

pub fn run_app() -> Result<(), slint::PlatformError> {
    let ui = MainWindow::new()?;

    let (tx, mut rx) = mpsc::channel::<AppCommand>(32);
    let state = Arc::new(Mutex::new(AppState::default()));

    let tx_search = tx.clone();
    ui.on_start_search(move || {
        let _ = tx_search.try_send(AppCommand::StartPipeline);
    });

    let tx_refresh = tx.clone();
    ui.on_refresh_configs(move || {
        let _ = tx_refresh.try_send(AppCommand::StartPipeline);
    });

    let tx_toggle = tx.clone();
    ui.on_toggle_vpn(move || {
        let _ = tx_toggle.try_send(AppCommand::ToggleConnect);
    });

    let tx_select = tx.clone();
    ui.on_select_config(move |index| {
        let _ = tx_select.try_send(AppCommand::SelectConfig(index as usize));
    });

    // --- ОБРАБОТКА ЗАКРЫТИЯ ОКНА ---
    let tx_close = tx.clone();
    ui.window().on_close_requested(move || {
        // Отправляем сигнал завершения в бэкенд
        let _ = tx_close.try_send(AppCommand::Shutdown);
        
        // Разрешаем закрытие окна
        slint::CloseRequestResponse::HideWindow
    });

    let ui_handle = ui.as_weak();
    let state_clone = Arc::clone(&state);   

    std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async move {
            while let Some(cmd) = rx.recv().await {
                let ui_weak = ui_handle.clone();
                let state = Arc::clone(&state_clone);

                match cmd {
                    AppCommand::LoadSavedConfigs => {
                        handle_load_saved_configs(ui_weak, state).await;
                    }
                    AppCommand::StartPipeline => {
                        handle_pipeline(ui_weak, state).await;
                    }
                    AppCommand::ToggleConnect => {
                        handle_toggle_vpn(ui_weak, state).await;
                    }
                    AppCommand::SelectConfig(index) => {
                        handle_select_config(ui_weak, state, index).await;
                    }
                    AppCommand::Shutdown => {
                        // Очищаем ресурсы Xray перед выходом
                        let mut lock = state.lock().await;
                        lock.stop_vpn().await;
                        break; // Завершаем рабочий цикл async-рантайма
                    }
                }
            }
        });
    });

    let _ = tx.try_send(AppCommand::LoadSavedConfigs);

    ui.run()
}

/// Синхронизация состояния приложения со Slint UI
fn sync_ui_configs(ui_weak: &slint::Weak<MainWindow>, state: &AppState) {
    let slint_configs: Vec<ServerConfig> = state
        .working_configs
        .iter()
        .enumerate()
        .map(|(i, candidate)| {
            let is_selected = state.selected_index == Some(i);
            // Проверяем именно по connected_index, а не по вычисленному выбору
            let is_connected = state.is_connected && state.connected_index == Some(i);
            let speed_mb = candidate.last_speed_kbps / 1024.0;

            ServerConfig {
                name: SharedString::from(candidate.link.remark()),
                flag: SharedString::from("🌐"),
                ping: SharedString::from(format!("{} ms", candidate.last_latency)),
                speed: SharedString::from(format!("{:.1} MB/s", speed_mb)),
                selected: is_selected,
                is_connected,
            }
        })
        .collect();

    let ui_weak = ui_weak.clone();
    let _ = slint::invoke_from_event_loop(move || {
        if let Some(ui) = ui_weak.upgrade() {
            let model = Rc::new(VecModel::from(slint_configs));
            ui.set_configs(ModelRc::from(model));
        }
    });
}

async fn handle_load_saved_configs(ui_weak: slint::Weak<MainWindow>, state: Arc<Mutex<AppState>>) {
    let saved_file = APP_DIR.join("results").join("working_configs.json");

    if saved_file.exists() {
        if let Ok(content) = tokio::fs::read_to_string(&saved_file).await {
            if let Ok(configs) = serde_json::from_str::<Vec<ProxyCandidate>>(&content) {
                let mut lock = state.lock().await;
                lock.working_configs = configs;
                if !lock.working_configs.is_empty() {
                    lock.selected_index = Some(0);
                }
                sync_ui_configs(&ui_weak, &lock);
            }
        }
    }
}

async fn handle_pipeline(ui_weak: slint::Weak<MainWindow>, state: Arc<Mutex<AppState>>) {
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
    links.truncate(50);

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

    let working_configs = run_pipeline(links, test_stages).await;
    let _ = save_working_configs(&working_configs).await;

    let mut lock = state.lock().await;
    lock.working_configs = working_configs;
    lock.selected_index = if !lock.working_configs.is_empty() { Some(0) } else { None };

    sync_ui_configs(&ui_weak, &lock);

    let _ = slint::invoke_from_event_loop({
        let ui_weak = ui_weak.clone();
        move || {
            if let Some(ui) = ui_weak.upgrade() {
                ui.set_search_progress(1.0);
                ui.set_is_searching(false);
            }
        }
    });
}

async fn handle_toggle_vpn(ui_weak: slint::Weak<MainWindow>, state: Arc<Mutex<AppState>>) {
    let mut lock = state.lock().await;

    if lock.is_connected {
        // Вызываем новый централизованный метод остановки
        lock.stop_vpn().await;

        let ui_weak_clone = ui_weak.clone();
        let _ = slint::invoke_from_event_loop(move || {
            if let Some(ui) = ui_weak_clone.upgrade() {
                ui.set_is_connected(false);
            }
        });

        sync_ui_configs(&ui_weak, &lock);
    } else {
        // Подключаем ТЕКУЩИЙ ВЫБРАННЫЙ сервер (selected_index)
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
                    lock.connected_index = Some(selected_idx);

                    let ui_weak_clone = ui_weak.clone();
                    let _ = slint::invoke_from_event_loop(move || {
                        if let Some(ui) = ui_weak_clone.upgrade() {
                            ui.set_is_connected(true);
                        }
                    });

                    sync_ui_configs(&ui_weak, &lock);
                }
            }
        }
    }
}

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
    // При переклике по списку меняется только selected_index, connected_index остается прежним
    sync_ui_configs(&ui_weak, &lock);
}

fn reset_ui_search(ui_weak: slint::Weak<MainWindow>) {
    let _ = slint::invoke_from_event_loop(move || {
        if let Some(ui) = ui_weak.upgrade() {
            ui.set_is_searching(false);
        }
    });
}