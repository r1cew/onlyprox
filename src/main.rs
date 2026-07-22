mod builder;
mod checker;
mod fetcher;
mod models;
mod parser;

use checker::run_pipeline;
use fetcher::fetch_subscription;
use models::TestStage;
use parser::parse_subscription_feed;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let feed_url = "https://raw.githubusercontent.com/igareck/vpn-configs-for-russia/refs/heads/main/BLACK_VLESS_RUS_mobile.txt";

    println!("1. Скачивание подписки...");
    // Исправлено: передаем переменные напрямую без "url:"
    let raw_text = fetch_subscription(feed_url).await?;

    println!("2. Парсинг ссылок...");
    let mut links = parse_subscription_feed(&raw_text);
    
    // Ограничиваем количество, если нужно
    links.truncate(150);

    // 3. Кастомная конфигурация этапов
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
            repeats: 2,
            interval_sec: 5,
        },
    ];

    // 4. Запуск проверки
    let working_configs = run_pipeline(links, test_stages).await;

    println!("\n--- РЕЗУЛЬТАТЫ ПРОВЕРКИ ---");
    for res in working_configs {
        println!(
            "[+] [{}] {} - Живой! (Latency: {}ms | Speed: {:.1} KB/s)",
            res.id,
            res.link.remark(),
            res.last_latency,
            res.last_speed_kbps
        );
    }

    Ok(())
}