#[tokio::test]
async fn test() -> Result<(), Box<dyn std::error::Error>> {
    let feed_url = "https://raw.githubusercontent.com/igareck/vpn-configs-for-russia/refs/heads/main/BLACK_VLESS_RUS_mobile.txt";

    println!("1. Скачивание подписки...");
    let raw_text = fetch_subscription(feed_url).await?;

    println!("2. Парсинг ссылок...");
    let mut links = parse_subscription_feed(&raw_text);
    links.truncate(150);

    // 3. Конфигурация этапов
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
            name: "Этап 2: Проверка наличия канала (от 100 КБ/с)".to_string(),
            speedtest: true,
            min_speed_kbps: 100.0,
            threads: 40,
            repeats: 1,
            interval_sec: 2,
        },
        TestStage {
            name: "Этап 3: Финальный замер стабильности (от 1.5 МБ/с)".to_string(),
            speedtest: true,
            min_speed_kbps: 1500.0,
            threads: 16,
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

    // Сохранение результатов в JSON
    if let Ok(path) = save_working_configs(&working_configs).await {
        println!("\n💾 Рабочие конфиги сохранены в файл: {:?}", path);
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
    println!("\n🚀 Запуск подключения к [{}]...", selected_candidate.link.remark());

    // 6. Инициализация XrayService для подключения выбранного конфига
    let mut service = XrayService::new(SOCKS_PORT);
    let final_config = XrayService::build_config(selected_candidate, SOCKS_PORT);
    let config_file = APP_DIR.join("config").join("xray.json");

    XrayService::write_config_to_file(&final_config, &config_file).await?;
    service.spawn_process(&config_file, false)?;
    service.set_system_proxy(true);

    println!("✅ Подключение установлено!");
    println!("SOCKS5/HTTP Порт: 127.0.0.1:{}", SOCKS_PORT);
    println!("Для завершения работы нажмите Ctrl + C\n");

    // 7. Чистое завершение при Ctrl+C или падении процесса
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {
            println!("\nОстанавливаем Xray и восстанавливаем настройки сети...");
            service.stop().await;
        }
        status = service.wait() => {
            service.set_system_proxy(false);
            println!("\nПроцесс Xray завершился с кодом: {:?}", status);
        }
    }

    Ok(())
}