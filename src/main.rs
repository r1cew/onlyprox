mod builder;
mod checker;
mod fetcher;
mod models;
mod parser;

use std::io::{self, Write};
use std::time::Duration;

use checker::run_pipeline;
use fetcher::fetch_subscription;
use models::TestStage;
use parser::parse_subscription_feed;
use tokio::process::Command;
use tokio::time::sleep;

use sysproxy::Sysproxy;

// Константа для статичного порта подключений
const SOCKS_PORT: u16 = 10818;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let feed_url = "https://raw.githubusercontent.com/igareck/vpn-configs-for-russia/refs/heads/main/BLACK_VLESS_RUS_mobile.txt";

    println!("1. Скачивание подписки...");
    let raw_text = fetch_subscription(feed_url).await?;

    println!("2. Парсинг ссылок...");
    let mut links = parse_subscription_feed(&raw_text);
    links.truncate(50);

    // 3. Конфигурация этапов
    let test_stages = vec![
        TestStage {
            name: "Этап 1: Быстрый экспресс-пинг".to_string(),
            speedtest: false,
            min_speed_kbps: 0.0,
            threads: 50,
            repeats: 1,
            interval_sec: 0,
        },
        TestStage {
            name: "Этап 2: Проверка наличия канала (от 100 КБ/с)".to_string(),
            speedtest: true,
            min_speed_kbps: 100.0,
            threads: 20,
            repeats: 1,
            interval_sec: 2,
        },
        TestStage {
            name: "Этап 3: Финальный замер стабильности (от 1.5 МБ/с)".to_string(),
            speedtest: true,
            min_speed_kbps: 1500.0,
            threads: 8,
            repeats: 1,
            interval_sec: 5,
        },
    ];

    // 4. Запуск воронки
    let working_configs = run_pipeline(links, test_stages).await;

    if working_configs.is_empty() {
        println!("❌ К сожалению, нет доступных рабочих VPN конфигов.");
        return Ok(());
    }

    

    // 5. Вывод списка и интерактивный выбор
    println!("\n=== ДОСТУПНЫЕ ДЛЯ ПОДКЛЮЧЕНИЯ VPN ===");
    for (idx, candidate) in working_configs.iter().enumerate() {
        println!(
            "[{}] {} | Ping: {}ms | Speed: {:.1} KB/s",
            idx + 1,
            candidate.link.remark(),
            candidate.last_latency,
            candidate.last_speed_kbps
        );
    }

    let selected_index = loop {
        print!("\nВыберите номер VPN для подключения (1-{}): ", working_configs.len());
        io::stdout().flush()?;

        let mut input = String::new();
        io::stdin().read_line(&mut input)?;

        match input.trim().parse::<usize>() {
            Ok(num) if num >= 1 && num <= working_configs.len() => break num - 1,
            _ => println!("⚠️ Некорректный ввод. Попробуйте еще раз."),
        }
    };

    let selected_candidate = &working_configs[selected_index];
    println!(
        "\n🚀 Запуск подключения к [{}]...",
        selected_candidate.link.remark()
    );
    
    // 6. Формирование и сохранение конфига на фиксированном порту
    let config_dir = models::APP_DIR.join("config");
    tokio::fs::create_dir_all(&config_dir)
        .await
        .map_err(|e| format!("Не удалось создать директорию конфига: {}", e))?;

    let config_file = config_dir.join("xray.json");
    let xray_config = checker::link_to_config(selected_candidate, SOCKS_PORT);

    let config_json = serde_json::to_string_pretty(&xray_config)
        .map_err(|e| format!("Ошибка сериализации JSON: {}", e))?;

    tokio::fs::write(&config_file, config_json)
        .await
        .map_err(|e| format!("Ошибка записи в файл xray.json: {}", e))?;

    // 7. Запуск процессов Xray
    let xray_exe = models::APP_DIR.join("xray.exe");
    let mut child = Command::new(&xray_exe)
        .arg("run")
        .arg("-c")
        .arg(&config_file)
        .spawn()
        .map_err(|e| format!("Не удалось запустить xray.exe: {}", e))?;

    println!("✅ Подключение установлено!");
    println!("SOCKS5/HTTP Порт: 127.0.0.1:{}", SOCKS_PORT);
    println!("Для завершения работы нажмите Ctrl + C\n");

    if sysproxy::Sysproxy::is_support() {
        let sysprox = sysproxy::Sysproxy{enable: true, host: "127.0.0.1".to_string(), port:SOCKS_PORT, bypass:"http".to_string()};
        _ = sysprox.set_system_proxy();
    }

    
    // Ожидание сигнала от пользователя Ctrl+C для чистой остановки
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {
            _ = sysproxy::Sysproxy{enable: false, host: "127.0.0.1".to_string(), port:SOCKS_PORT, bypass:"http".to_string()}.set_system_proxy();
            println!("\nОстанавливаем Xray и завершаем работу...");
            let _ = child.kill().await;
        }
        status = child.wait() => {
            println!("\nProcess Xray завершился с кодом: {:?}", status);
        }
    }

    Ok(())
}