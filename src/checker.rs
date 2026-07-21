use crate::builder::build_outbound_from_link;
use crate::models::{CheckResult, ProxyLink, XrayConfig};
use reqwest::Client;
use std::net::TcpListener;
use std::sync::Arc;
use std::time::Duration;
use tokio::process::Command;
use tokio::sync::{mpsc, Semaphore};
use tokio::time::{timeout, Instant};
use futures_util::StreamExt; // Убедитесь, что эта зависимость есть (обычно идет с reqwest/tokio)

pub struct ConfigChecker {
    concurrency_limit: Arc<Semaphore>,
    timeout_duration: Duration,
}

impl ConfigChecker {
    pub fn new(max_concurrent: usize, timeout_secs: u64) -> Self {
        Self {
            concurrency_limit: Arc::new(Semaphore::new(max_concurrent)),
            timeout_duration: Duration::from_secs(timeout_secs),
        }
    }

    pub async fn check_all(&self, links: Vec<ProxyLink>) -> Vec<CheckResult> {
        let (tx, mut rx) = mpsc::channel(links.len().max(1));

        for (i, link) in links.into_iter().enumerate() {
            let sem = Arc::clone(&self.concurrency_limit);
            let tx = tx.clone();
            let timeout_dur = self.timeout_duration;
            let id = format!("cfg_{}", i + 1);

            tokio::spawn(async move {
                let _permit = sem.acquire().await.unwrap();
                let result = check_single_link(&id, &link, timeout_dur).await;
                let _ = tx.send(result).await;
            });
        }

        drop(tx); // Закрываем исходный отправщик, чтобы rx смог завершиться

        let mut results = Vec::new();
        while let Some(res) = rx.recv().await {
            results.push(res);
        }

        results
    }
}

async fn check_single_link(id: &str, link: &ProxyLink, timeout_dur: Duration) -> CheckResult {
    let test_socks_port = get_free_port();

    let outbound = build_outbound_from_link(link);
    let xray_config = XrayConfig::new_with_proxy(outbound, test_socks_port);
    let config_json = match serde_json::to_string(&xray_config) {
        Ok(j) => j,
        Err(_) => {
            return CheckResult {
                config_id: id.to_string(),
                remark: link.remark().to_string(),
                is_working: false,
                latency_ms: 0,
            }
        }
    };

    let temp_config_path = format!("C:\\Users\\r1ceb\\Desktop\\onlyprox\\tmp\\xray_check_{}_{}.json", id, test_socks_port);
    let _ = tokio::fs::create_dir_all("C:\\Users\\r1ceb\\Desktop\\onlyprox\\tmp").await;
    
    if tokio::fs::write(&temp_config_path, config_json).await.is_err() {
        return CheckResult {
            config_id: id.to_string(),
            remark: link.remark().to_string(),
            is_working: false,
            latency_ms: 0,
        };
    }

    let mut child = match Command::new("C:\\Users\\r1ceb\\Desktop\\Xray-windows-64\\xray.exe")
        .arg("run")
        .arg("-c")
        .arg(&temp_config_path)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
    {
        Ok(c) => c,
        Err(_) => {
            let _ = tokio::fs::remove_file(&temp_config_path).await;
            return CheckResult {
                config_id: id.to_string(),
                remark: link.remark().to_string(),
                is_working: false,
                latency_ms: 0,
            };
        }
    };

    // Увеличенная пауза для стабильного старта TLS / Reality соединений (500мс)
    tokio::time::sleep(Duration::from_millis(200)).await;

    let start_time = std::time::Instant::now();
    // Используем gstatic для проверки
    let is_working = test_http_ping(test_socks_port, timeout_dur).await;
    let latency_ms = start_time.elapsed().as_millis();

    let _ = child.kill().await;
    let _ = tokio::fs::remove_file(&temp_config_path).await;

    CheckResult {
        config_id: id.to_string(),
        remark: link.remark().to_string(),
        is_working,
        latency_ms,
    }
}

async fn test_http_ping(socks_port: u16, timeout_dur: Duration) -> bool {
    let proxy_url = format!("http://127.0.0.1:{}", socks_port);

    let client = match Client::builder()
        .proxy(reqwest::Proxy::all(&proxy_url).unwrap())
        .timeout(timeout_dur)
        .build()
    {
        Ok(c) => c,
        Err(_) => return false,
    };

    // Меняем endpoint проверки на более надежный
    match timeout(
        timeout_dur,
        client.get("    ").send(),
    )
    .await
    {
        Ok(Ok(resp)) => resp.status().is_success(),
        _ => false,
    }
}


async fn measure_speed_and_ping(proxy_port: u16, timeout_dur: Duration) -> (bool, u128, f64) {
    let proxy_url = format!("http://127.0.0.1:{}", proxy_port);

    let client = match Client::builder()
        .proxy(reqwest::Proxy::all(&proxy_url).unwrap())
        .timeout(timeout_dur)
        .build()
    {
        Ok(c) => c,
        Err(_) => return (false, 0, 0.0),
    };

    // Используем небольшой файл на 1 МБ для быстрого замера скорости
    let test_url = "https://speedtest.selectel.ru/10MB";

    let start_time = Instant::now();
    let download_start = Instant::now();

    // Скачиваем файл с общим таймаутом
    let response = match timeout(timeout_dur, client.get(test_url).send()).await {
        Ok(Ok(resp)) if resp.status().is_success() => resp,
        _ => return (false, 0, 0.0),
    };

    let latency_ms = start_time.elapsed().as_millis();

    // Получаем байты целиком (для 1МБ это мгновенно и безопасно для памяти)
    let bytes = match timeout(timeout_dur, response.bytes()).await {
        Ok(Ok(b)) => b,
        _ => return (false, latency_ms, 0.0),
    };

    let duration_secs = download_start.elapsed().as_secs_f64();
    let downloaded_bytes = bytes.len();

    // Считаем скорость в Мбит/с (Megabits per second)
    let speed_mbps = if duration_secs > 0.0 {
        let bits = (downloaded_bytes as f64) * 8.0;
        (bits / duration_secs) / 1_000_000.0
    } else {
        0.0
    };

    let is_working = downloaded_bytes > 0;

    (is_working, latency_ms, speed_mbps)
}


fn get_free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .and_then(|l| l.local_addr())
        .map(|addr| addr.port())
        .unwrap_or(10808)
}