use crate::checker::link_to_config;
use crate::models::{ProxyCandidate, APP_DIR};
use std::path::PathBuf;
use sysproxy::Sysproxy;
use tokio::process::{Child, Command};

pub struct XrayService {
    child: Option<Child>,
    pub port: u16,
}

impl XrayService {
    pub fn new(port: u16) -> Self {
        Self { child: None, port }
    }

    /// Генерирует и записывает JSON конфигурацию для Xray
    pub async fn write_config(&self, candidate: &ProxyCandidate) -> Result<PathBuf, String> {
        let config_dir = APP_DIR.join("config");
        tokio::fs::create_dir_all(&config_dir)
            .await
            .map_err(|e| format!("Ошибка создания папки конфига: {}", e))?;

        let config_file = config_dir.join("xray.json");
        let xray_config = link_to_config(candidate, self.port);

        let config_json = serde_json::to_string_pretty(&xray_config)
            .map_err(|e| format!("Ошибка сериализации JSON: {}", e))?;

        tokio::fs::write(&config_file, config_json)
            .await
            .map_err(|e| format!("Ошибка записи файла xray.json: {}", e))?;

        Ok(config_file)
    }

    /// Запускает процесс Xray с указанным конфигом (без вывода в консоль при `silent = true`)
    pub fn spawn_process(&mut self, config_path: &PathBuf, silent: bool) -> Result<(), String> {
        let xray_exe = APP_DIR.join("xray.exe");

        let mut cmd = Command::new(&xray_exe);
        cmd.arg("run").arg("-c").arg(config_path);

        if silent {
            cmd.stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null());
        }

        let child = cmd
            .spawn()
            .map_err(|e| format!("Не удалось запустить xray.exe: {}", e))?;

        self.child = Some(child);
        Ok(())
    }

    /// Включает или выключает системный прокси Windows
    pub fn set_system_proxy(&self, enable: bool) {
        if Sysproxy::is_support() {
            let sysprox = Sysproxy {
                enable,
                host: "127.0.0.1".to_string(),
                port: self.port,
                bypass: "localhost;127.*;10.*;172.16.*;172.17.*;172.18.*;172.19.*;172.20.*;172.21.*;172.22.*;172.23.*;172.24.*;172.25.*;172.26.*;172.27.*;172.28.*;172.29.*;172.30.*;172.31.*;192.168.*".to_string(),
            };
            let _ = sysprox.set_system_proxy();
        }
    }

    /// Безопасно останавливает Xray и снимает системный прокси
    pub async fn stop(&mut self) {
        self.set_system_proxy(false);
        if let Some(mut child) = self.child.take() {
            let _ = child.kill().await;
        }
    }

    /// Дожидается завершения процесса (если он упал сам)
    pub async fn wait(&mut self) -> Option<std::process::ExitStatus> {
        if let Some(mut child) = self.child.take() {
            child.wait().await.ok()
        } else {
            None
        }
    }
}

/// Сохранение списка проверенных рабочих конфигов в файл
pub async fn save_working_configs(candidates: &[ProxyCandidate]) -> Result<PathBuf, String> {
    let out_dir = APP_DIR.join("results");
    tokio::fs::create_dir_all(&out_dir)
        .await
        .map_err(|e| format!("Ошибка создания папки результатов: {}", e))?;

    let file_path = out_dir.join("working_configs.json");
    let json_data = serde_json::to_string_pretty(candidates)
        .map_err(|e| format!("Ошибка сериализации результатов: {}", e))?;

    tokio::fs::write(&file_path, json_data)
        .await
        .map_err(|e| format!("Ошибка записи результатов: {}", e))?;

    Ok(file_path)
}