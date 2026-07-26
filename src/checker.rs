use crate::builder::build_outbound_from_link;
use crate::models::*;
use futures_util::StreamExt;
use reqwest::Client;
use std::collections::HashMap;
use std::net::TcpListener;
use std::sync::Arc;
use std::time::Duration;
use tokio::process::Command;
use tokio::sync::Semaphore;
use tokio::time::{sleep, timeout, Instant};

#[cfg(target_os = "windows")]
pub const CREATE_NO_WINDOW: u32 = 0x08000000;


pub fn get_free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .and_then(|l| l.local_addr())
        .map(|addr| addr.port())
        .unwrap_or(10808)
}

pub fn link_to_config(candidate: &ProxyCandidate, port: u16) -> XrayConfig {
    let outbound = build_outbound_from_link(&candidate.link);
    XrayConfig::new_with_proxy(outbound, port)
}

fn get_flag_emoji(country_code: &str) -> Option<String> {
    if country_code.len() != 2 {
        return None;
    }

    let mut flag = String::new();

    for ch in country_code.to_ascii_uppercase().chars() {
        if ch.is_ascii_alphabetic() {
            let code_point = ch as u32 + 127397;

            if let Some(flag_char) = std::char::from_u32(code_point) {
                flag.push(flag_char);
            } else {
                return None;
            }
        } else {
            return None;
        }
    }
    
    Some(flag)
}

pub async fn check_country_flag(socks_port: u16, timeout_dur: Duration) -> Option<String> {
    let proxy_url = format!("http://127.0.0.1:{}", socks_port);

    let client = match Client::builder()
        .proxy(reqwest::Proxy::all(&proxy_url).ok()?)
        .timeout(timeout_dur)
        .build()
    {
        Ok(c) => c,
        Err(_) => return None,
    };

    let text = match timeout(
        timeout_dur,
        client.get("https://cloudflare.com/cdn-cgi/trace").send(),
    )
    .await
    {
        Ok(Ok(resp)) => match resp.text().await {
            Ok(t) => t,
            Err(_) => return None,
        },
        _ => return None,
    };

    // Парсим формат key=value в HashMap
    let trace: HashMap<&str, &str> = text
        .lines()
        .filter_map(|line| line.split_once('='))
        .collect();

    let country = trace.get("loc").unwrap_or(&"XX");
    let flag = get_flag_emoji(country)?;

    Some(flag)
}

pub async fn measure_speed_kbps(
    socks_port: u16,
    timeout_dur: Duration,
    min_speed_kbps: f64,
) -> Option<f64> {
    let proxy_url = format!("http://127.0.0.1:{}", socks_port);

    let client = Client::builder()
        .proxy(reqwest::Proxy::all(&proxy_url).ok()?)
        .timeout(timeout_dur)
        .build()
        .ok()?;

    let bytes_to_fetch = if min_speed_kbps >= 500.0 { 2_000_000 } else { 500_000 };
    let test_url = format!("https://speed.cloudflare.com/__down?bytes={}", bytes_to_fetch);

    let response = timeout(timeout_dur, client.get(&test_url).send())
        .await
        .ok()?
        .ok()?;

    if !response.status().is_success() {
        return None;
    }

    let download_start = Instant::now();
    let mut downloaded_bytes = 0;
    let mut stream = response.bytes_stream();

    while let Ok(Some(chunk_result)) = timeout(timeout_dur, stream.next()).await {
        if let Ok(chunk) = chunk_result {
            downloaded_bytes += chunk.len();
        } else {
            break;
        }
    }

    let duration_secs = download_start.elapsed().as_secs_f64();

    if duration_secs > 0.05 && downloaded_bytes > 0 {
        let speed_kbps = (downloaded_bytes as f64 / 1024.0) / duration_secs;
        Some(speed_kbps)
    } else {
        None
    }
}

pub async fn check_single_candidate(
    candidate: &ProxyCandidate,
    need_speedtest: bool,
    min_speed_kbps: f64,
) -> CheckResult {
    let test_socks_port = get_free_port();

    let xray_config = link_to_config(candidate, test_socks_port);

    let config_json = match serde_json::to_string(&xray_config) {
        Ok(j) => j,
        Err(_) => {
            return CheckResult {
                is_working: false,
                latency_ms: 0,
                speed_kbps: 0.0,
                flag: String::new(),
            }
        }
    };
    let tmp = APP_DIR.join("tmp");

    let temp_config_path = tmp
        .join(format!("xray_check_{}_{}.json", candidate.id, test_socks_port));
    
    let _ = tokio::fs::create_dir_all(&tmp).await;

    if tokio::fs::write(&temp_config_path, config_json)
        .await
        .is_err()
    {
        return CheckResult {
            is_working: false,
            latency_ms: 0,
            speed_kbps: 0.0,
            flag: String::new(),
        };
    }
    
    #[cfg(target_os = "windows")]
    let xray = APP_DIR.join("bin").join("xray.exe");
    #[cfg(target_os = "linux")]
    let xray = APP_DIR.join("bin").join("xray");

    if !xray.exists() {
        let _ = tokio::fs::remove_file(&temp_config_path).await;
        return CheckResult {
            is_working: false,
            latency_ms: 0,
            speed_kbps: 0.0,
            flag: String::new(),
        };
    }

    let mut cmd = Command::new(&xray);
    cmd.arg("run")
        .arg("-c")
        .arg(&temp_config_path)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());

    #[cfg(target_os = "windows")]
    cmd.creation_flags(CREATE_NO_WINDOW);

    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(_) => {
            let _ = tokio::fs::remove_file(&temp_config_path).await;
            return CheckResult {
                is_working: false,
                latency_ms: 0,
                speed_kbps: 0.0,
                flag: String::new(),
            };
        }
    };
    
    sleep(Duration::from_millis(400)).await;

    let start_time = Instant::now();
    let flag_option = check_country_flag(test_socks_port, Duration::from_secs(5)).await;
    let latency_ms = start_time.elapsed().as_millis();
    
    // Используем флаг если получили, иначе - глобус
    let flag = flag_option.clone().unwrap_or_else(|| "🌐".to_string());

    let mut speed_kbps = 0.0;
    let mut is_working = false;

    // Проверяем, удалось ли получить флаг (прокси ответил)
    if flag_option.is_some() {
        if need_speedtest {
            if let Some(speed) = measure_speed_kbps(test_socks_port, Duration::from_secs(6), min_speed_kbps).await {
                speed_kbps = speed;
                is_working = true;
            }
        } else {
            is_working = true;
        }
    }

    let _ = child.kill().await;
    let _ = tokio::fs::remove_file(&temp_config_path).await;

    CheckResult {
        is_working,
        latency_ms,
        speed_kbps,
        flag,
    }
}

pub async fn run_pipeline(
    initial_links: Vec<ProxyLink>,
    stages: Vec<TestStage>,
) -> Vec<ProxyCandidate> {
    let mut candidates: Vec<ProxyCandidate> = initial_links
        .into_iter()
        .enumerate()
        .map(|(i, link)| ProxyCandidate {
            id: format!("cfg_{}", i + 1),
            link,
            last_latency: 0,
            last_speed_kbps: 0.0,
            flag: String::new(),
        })
        .collect();

    println!("🚀 СТАРТ ВОРОНКИ ПРОВЕРКИ. Всего кандидатов: {}\n", candidates.len());

    for (stage_idx, stage) in stages.iter().enumerate() {
        if candidates.is_empty() {
            println!("⚠️ Все конфиги отсеялись. Проверка завершена.");
            break;
        }

        println!(
            "==================================================\n\
             🔹 ЭТАП {}/{}: {}\n\
             На входе: {} | Потоков: {} | Повторов: {} | Мин.Скорость: {} КБ/с\n\
             ==================================================",
            stage_idx + 1,
            stages.len(),
            stage.name,
            candidates.len(),
            stage.threads,
            stage.repeats,
            stage.min_speed_kbps
        );

        let semaphore = Arc::new(Semaphore::new(stage.threads));
        let mut tasks = vec![];

        for candidate in candidates {
            let sem = Arc::clone(&semaphore);
            let stage_info = stage.clone();

            tasks.push(tokio::spawn(async move {
                let _permit = sem.acquire().await.unwrap();

                let mut passed_all_repeats = true;
                let mut total_speed = 0.0;
                let mut total_latency = 0;
                let mut last_flag = String::new();

                for r in 0..stage_info.repeats {
                    if r > 0 && stage_info.interval_sec > 0 {
                        sleep(Duration::from_secs(stage_info.interval_sec)).await;
                    }

                    let res = check_single_candidate(
                        &candidate, 
                        stage_info.speedtest, 
                        stage_info.min_speed_kbps
                    ).await;

                    if !res.is_working || res.speed_kbps < stage_info.min_speed_kbps {
                        passed_all_repeats = false;
                        break;
                    }

                    total_speed += res.speed_kbps;
                    total_latency += res.latency_ms;
                    last_flag = res.flag.clone();
                }

                if passed_all_repeats {
                    let avg_speed = total_speed / (stage_info.repeats as f64);
                    let avg_latency = total_latency / (stage_info.repeats as u128);

                    Some(ProxyCandidate {
                        id: candidate.id,
                        link: candidate.link,
                        last_latency: avg_latency,
                        last_speed_kbps: avg_speed,
                        flag: last_flag,
                    })
                } else {
                    None
                }
            }));
        }

        let mut next_stage_candidates = Vec::new();
        for task in tasks {
            if let Ok(Some(survived)) = task.await {
                println!(
                    "  [✓] [{}] {} | {} | {} ms | {:.1} KB/s",
                    survived.id,
                    survived.link.remark(),
                    survived.flag,
                    survived.last_latency,
                    survived.last_speed_kbps
                );
                next_stage_candidates.push(survived);
            }
        }

        candidates = next_stage_candidates;
        println!("\nВыжило на этапе '{}': {}\n", stage.name, candidates.len());
    }

    println!("🎉 ВОРОНКА ЗАВЕРШЕНА! Итого отборных конфигов: {}", candidates.len());
    candidates
}