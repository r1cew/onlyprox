use std::sync::{Arc, Mutex};

use tokio::sync::mpsc;

use crate::checker::run_pipeline;
use crate::fetcher::fetch_all_links;
use crate::models::{ProxyCandidate, SourcesConfig, TestStage, APP_DIR};
use crate::sources;
use crate::xray::{kill_stray_xray_processes, save_working_configs, XrayService};

const SOCKS_PORT: u16 = 10818;

/// Одна строка списка конфигов, уже готовая для отрисовки в UI.
#[derive(Clone, Debug, Default)]
pub struct UiConfig {
    pub name: String,
    pub flag: String,
    pub ping: String,
    pub speed: String,
    pub selected: bool,
    pub is_connected: bool,
}

/// Снимок состояния, который читает GUI-поток на каждом кадре.
/// Обновляется только фоновым потоком, под мьютексом.
#[derive(Clone, Debug, Default)]
pub struct UiState {
    pub configs: Vec<UiConfig>,
    pub is_connected: bool,
    pub is_searching: bool,
    pub search_progress: f32,
    pub search_stage: String,
    pub sources: SourcesConfig,
    pub last_error: Option<String>,
}

/// Команды, которые GUI-поток отправляет в фоновый воркер.
pub enum Command {
    LoadAll,
    StartSearch,
    RetestConfigs,
    ToggleVpn,
    SelectConfig(usize),
    AddSubscription { name: String, url: String },
    RemoveSubscription(usize),
    ToggleSubscription(usize),
    AddCustomLink(String),
    RemoveCustomLink(usize),
    ToggleCustomLink(usize),
    Shutdown,
}

/// Внутреннее состояние воркера (не разделяется с GUI напрямую).
struct WorkerState {
    working_configs: Vec<ProxyCandidate>,
    selected_index: Option<usize>,
    connected_index: Option<usize>,
    xray_service: Option<XrayService>,
    is_connected: bool,
    sources: SourcesConfig,
}

impl WorkerState {
    async fn stop_vpn(&mut self) {
        if let Some(mut service) = self.xray_service.take() {
            service.stop().await;
        }
        self.is_connected = false;
        self.connected_index = None;
        kill_stray_xray_processes().await;
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    }
}

/// Запускает фоновый поток с собственным tokio-рантаймом и возвращает
/// канал команд + общий снимок состояния для GUI.
pub fn spawn(ctx: egui::Context) -> (mpsc::Sender<Command>, Arc<Mutex<UiState>>) {
    let (tx, mut rx) = mpsc::channel::<Command>(32);
    let shared = Arc::new(Mutex::new(UiState::default()));
    let shared_worker = Arc::clone(&shared);

    std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().expect("не удалось создать tokio runtime");
        rt.block_on(async move {
            let mut state = WorkerState {
                working_configs: Vec::new(),
                selected_index: None,
                connected_index: None,
                xray_service: None,
                is_connected: false,
                sources: SourcesConfig::load().await,
            };

            publish(&shared_worker, &ctx, &state, false, 0.0, "");

            while let Some(cmd) = rx.recv().await {
                match cmd {
                    Command::LoadAll => {
                        load_saved_configs(&mut state).await;
                        publish(&shared_worker, &ctx, &state, false, 0.0, "");
                    }
                    Command::StartSearch => {
                        run_search(&mut state, &shared_worker, &ctx, false).await;
                    }
                    Command::RetestConfigs => {
                        run_search(&mut state, &shared_worker, &ctx, true).await;
                    }
                    Command::ToggleVpn => {
                        toggle_vpn(&mut state).await;
                        publish(&shared_worker, &ctx, &state, false, 0.0, "");
                    }
                    Command::SelectConfig(index) => {
                        if index < state.working_configs.len() {
                            state.selected_index = Some(index);
                        }
                        publish(&shared_worker, &ctx, &state, false, 0.0, "");
                    }
                    Command::AddSubscription { name, url } => {
                        sources::add_subscription(&mut state.sources, name, url);
                        let _ = state.sources.save().await;
                        publish(&shared_worker, &ctx, &state, false, 0.0, "");
                    }
                    Command::RemoveSubscription(index) => {
                        sources::remove_subscription(&mut state.sources, index);
                        let _ = state.sources.save().await;
                        publish(&shared_worker, &ctx, &state, false, 0.0, "");
                    }
                    Command::ToggleSubscription(index) => {
                        sources::toggle_subscription(&mut state.sources, index);
                        let _ = state.sources.save().await;
                        publish(&shared_worker, &ctx, &state, false, 0.0, "");
                    }
                    Command::AddCustomLink(raw) => {
                        let mut ui = shared_worker.lock().unwrap();
                        if let Err(e) = sources::add_custom_link(&mut state.sources, raw) {
                            ui.last_error = Some(e);
                        } else {
                            ui.last_error = None;
                        }
                        drop(ui);
                        let _ = state.sources.save().await;
                        publish(&shared_worker, &ctx, &state, false, 0.0, "");
                    }
                    Command::RemoveCustomLink(index) => {
                        sources::remove_custom_link(&mut state.sources, index);
                        let _ = state.sources.save().await;
                        publish(&shared_worker, &ctx, &state, false, 0.0, "");
                    }
                    Command::ToggleCustomLink(index) => {
                        sources::toggle_custom_link(&mut state.sources, index);
                        let _ = state.sources.save().await;
                        publish(&shared_worker, &ctx, &state, false, 0.0, "");
                    }
                    Command::Shutdown => {
                        state.stop_vpn().await;
                        break;
                    }
                }
            }
        });
    });

    (tx, shared)
}

async fn load_saved_configs(state: &mut WorkerState) {
    let saved_file = APP_DIR.join("results").join("working_configs.json");
    if let Ok(content) = tokio::fs::read_to_string(&saved_file).await {
        if let Ok(configs) = serde_json::from_str::<Vec<ProxyCandidate>>(&content) {
            state.working_configs = configs;
            state.selected_index = if state.working_configs.is_empty() { None } else { Some(0) };
        }
    }
}

fn default_stages(is_retest: bool) -> Vec<TestStage> {
    if !is_retest {
        vec![
            TestStage {
                name: "Быстрый экспресс-пинг".to_string(),
                speedtest: false,
                min_speed_kbps: 0.0,
                threads: 80,
                repeats: 1,
                interval_sec: 0,
            },

        ]
    } else {
        vec![
            TestStage {
                name: "Быстрый экспресс-пинг".to_string(),
                speedtest: false,
                min_speed_kbps: 0.0,
                threads: 80,
                repeats: 1,
                interval_sec: 0,
            },
            TestStage {
                name: "Замер стабильности".to_string(),
                speedtest: true,
                min_speed_kbps: 1500.0,
                threads: 16,
                repeats: 1,
                interval_sec: 5,
            },
        ]
    }
}

async fn run_search(state: &mut WorkerState, shared: &Arc<Mutex<UiState>>, ctx: &egui::Context, is_retest: bool) {
    publish(shared, ctx, state, true, 0.05, "Сбор ссылок из источников...");

    let links = if !is_retest {
        let result = fetch_all_links(&state.sources).await;
        if let Some(err) = result.errors.first() {
            let mut ui = shared.lock().unwrap();
            ui.last_error = Some(err.clone());
        }
        let mut links = result.links;
        links.truncate(150);
        links
    } else {
        state.working_configs.iter().map(|c| c.link.clone()).collect::<Vec<_>>()
    };

    if links.is_empty() {
        publish(shared, ctx, state, false, 0.0, "");
        let mut ui = shared.lock().unwrap();
        ui.last_error = Some("Не найдено ни одной конфигурации. Проверьте источники.".to_string());
        return;
    }

    publish(shared, ctx, state, true, 0.15, "Тестирование серверов...");

    let stages = default_stages(is_retest);
    let stages_len = stages.len();

    let shared_cb = Arc::clone(shared);
    let ctx_cb = ctx.clone();
    let progress_cb: Arc<dyn Fn(usize, usize, &str) + Send + Sync> = Arc::new(move |stage_idx, total, name| {
        let frac = 0.2 + 0.75 * (stage_idx as f32 / total.max(1) as f32);
        let mut ui = shared_cb.lock().unwrap();
        ui.is_searching = true;
        ui.search_progress = frac;
        ui.search_stage = format!("Этап {}/{}: {}", stage_idx + 1, total, name);
        drop(ui);
        ctx_cb.request_repaint();
    });

    let working_configs = run_pipeline(links, stages, Some(progress_cb)).await;
    let _ = save_working_configs(&working_configs).await;

    state.working_configs = working_configs;
    state.selected_index = if state.working_configs.is_empty() { None } else { Some(0) };
    let _ = stages_len;

    publish(shared, ctx, state, false, 1.0, "Готово");
}

async fn toggle_vpn(state: &mut WorkerState) {
    if state.is_connected {
        state.stop_vpn().await;
        return;
    }

    let Some(selected_idx) = state.selected_index else {
        return;
    };
    let Some(candidate) = state.working_configs.get(selected_idx).cloned() else {
        return;
    };

    let mut service = XrayService::new(SOCKS_PORT);
    let final_config = XrayService::build_config(&candidate, SOCKS_PORT);
    let config_file = APP_DIR.join("config").join("xray.json");

    if XrayService::write_config_to_file(&final_config, &config_file).await.is_err() {
        return;
    }
    if service.spawn_process(&config_file, true).is_err() {
        return;
    }

    service.set_system_proxy(true);
    state.xray_service = Some(service);
    state.is_connected = true;
    state.connected_index = Some(selected_idx);
}

/// Пересчитывает `UiState` из внутреннего состояния воркера и публикует
/// его для GUI-потока, попутно запрашивая перерисовку кадра.
fn publish(
    shared: &Arc<Mutex<UiState>>,
    ctx: &egui::Context,
    state: &WorkerState,
    is_searching: bool,
    progress: f32,
    stage_label: &str,
) {
    let configs = state
        .working_configs
        .iter()
        .enumerate()
        .map(|(i, candidate)| {
            let is_selected = state.selected_index == Some(i);
            let is_connected = state.is_connected && state.connected_index == Some(i);
            let speed_mb = candidate.last_speed_kbps / 1024.0;

            UiConfig {
                name: candidate.link.remark().to_string(),
                flag: candidate.flag.clone(),
                ping: format!("{} ms", candidate.last_latency),
                speed: format!("{:.1} MB/s", speed_mb),
                selected: is_selected,
                is_connected,
            }
        })
        .collect();

    let mut ui = shared.lock().unwrap();
    ui.configs = configs;
    ui.is_connected = state.is_connected;
    ui.is_searching = is_searching;
    ui.search_progress = progress;
    if !stage_label.is_empty() {
        ui.search_stage = stage_label.to_string();
    }
    ui.sources = state.sources.clone();
    drop(ui);

    ctx.request_repaint();
}
