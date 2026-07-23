use crate::builder::build_outbound_from_link;
use crate::models::{ProxyCandidate, XrayConfig, APP_DIR};
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

    /// Генерирует XrayConfig из кандидата
    pub fn build_config(candidate: &ProxyCandidate, port: u16) -> XrayConfig {
        let outbound = build_outbound_from_link(&candidate.link);
        XrayConfig::new_with_proxy(outbound, port)
    }

    /// Генерирует и сохраняет JSON-конфиг во временный или постоянный файл
    pub async fn write_config_to_file(
        config: &XrayConfig,
        target_path: &PathBuf,
    ) -> Result<(), String> {
        if let Some(parent) = target_path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| format!("Ошибка создания директории {:?}: {}", parent, e))?;
        }

        let config_json = serde_json::to_string_pretty(config)
            .map_err(|e| format!("Ошибка сериализации JSON: {}", e))?;

        tokio::fs::write(target_path, config_json)
            .await
            .map_err(|e| format!("Ошибка записи файла конфига: {}", e))?;

        Ok(())
    }

    /// Запускает процесс xray.exe с указанным конфигом
    pub fn spawn_process(&mut self, config_path: &PathBuf, silent: bool) -> Result<(), String> {
        let xray_exe = APP_DIR.join("xray.exe");

        if !xray_exe.exists() {
            return Err(format!("Исполняемый файл xray.exe не найден по пути: {:?}", xray_exe));
        }

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
                bypass: "localhost;127.*;10.*;172.16.*;192.168.*".to_string(),
            };
            let _ = sysprox.set_system_proxy();
        }
    }

    /// Безопасно останавливает Xray и отключает системный прокси
    pub async fn stop(&mut self) {
        self.set_system_proxy(false);
        if let Some(mut child) = self.child.take() {
            let _ = child.kill().await;
        }
    }

    /// Дожидается завершения процесса Xray
    pub async fn wait(&mut self) -> Option<std::process::ExitStatus> {
        if let Some(mut child) = self.child.take() {
            child.wait().await.ok()
        } else {
            None
        }
    }
}

/// Сохраняет отобранные рабочие конфиги в results/working_configs.json
pub async fn save_working_configs(candidates: &[ProxyCandidate]) -> Result<PathBuf, String> {
    let out_dir = APP_DIR.join("results");
    tokio::fs::create_dir_all(&out_dir)
        .await
        .map_err(|e| format!("Ошибка создания папки results: {}", e))?;

    let file_path = out_dir.join("working_configs.json");
    let json_data = serde_json::to_string_pretty(candidates)
        .map_err(|e| format!("Ошибка сериализации результатов: {}", e))?;

    tokio::fs::write(&file_path, json_data)
        .await
        .map_err(|e| format!("Ошибка записи working_configs.json: {}", e))?;

    Ok(file_path)
}