mod builder;
mod checker;
mod fetcher;
mod models;
mod parser;

use checker::ConfigChecker;
use fetcher::fetch_subscription;
use parser::parse_subscription_feed;

pub async fn process_subscription(url: &str, limit: usize) -> Result<(), Box<dyn std::error::Error>> {
    println!("1. Скачивание подписки...");
    let raw_text = fetch_subscription(url).await?;

    println!("2. Парсинг ссылок...");
    let mut links = parse_subscription_feed(&raw_text);
    links.truncate(limit);

    println!("3. Запуск проверки {} конфигов...", links.len());
    let checker = ConfigChecker::new(50, 3); // 50 потоков, 10 сек таймаут
    let results = checker.check_all(links).await;

    println!("\n--- РЕЗУЛЬТАТЫ ПРОВЕРКИ ---");
    let mut working_count = 0;
    for res in results {
        if res.is_working {
            working_count += 1;
            println!(
                "[+] [{}] {} - Живой! (Latency: {}ms)",
                res.remark, res.remark, res.latency_ms 
            );
        } else {
            println!("[-] [{}] {} - Мертвый", res.config_id, res.remark);
        }
    }

    println!("\nИтого рабочих конфигов: {}", working_count);

    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    
    let feed_url = "https://raw.githubusercontent.com/igareck/vpn-configs-for-russia/refs/heads/main/BLACK_VLESS_RUS_mobile.txt";
    
    // Прогоняем первые 10 элементов
    process_subscription(feed_url, 150).await?;

    Ok(())
}