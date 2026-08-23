use crate::models::{ProxyLink, SourcesConfig};
use crate::parser::{parse_link, parse_subscription_feed};
use std::collections::HashSet;

/// Скачивает содержимое одной подписки (обычный текст со списком ссылок,
/// опционально в base64 — так же, как отдают большинство публичных фидов).
pub async fn fetch_subscription(url: &str) -> Result<String, reqwest::Error> {
    let raw = reqwest::get(url).await?.text().await?;
    Ok(raw)
}

/// Результат сбора конфигов из всех источников: сами ссылки + список
/// ошибок по конкретным подпискам (чтобы не ронять весь пайплайн,
/// если одна из подписок недоступна).
pub struct FetchAllResult {
    pub links: Vec<ProxyLink>,
    pub errors: Vec<String>,
}

/// Собирает конфиги из всех включённых подписок и всех вручную
/// добавленных ссылок, убирая дубликаты (по протоколу+адресу+порту+id).
pub async fn fetch_all_links(sources: &SourcesConfig) -> FetchAllResult {
    let mut links = Vec::new();
    let mut errors = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();

    for sub in sources.subscriptions.iter().filter(|s| s.enabled) {
        match fetch_subscription(&sub.url).await {
            Ok(raw) => {
                for link in parse_subscription_feed(&raw) {
                    let key = link.dedup_key();
                    if seen.insert(key) {
                        links.push(link);
                    }
                }
            }
            Err(e) => {
                errors.push(format!("«{}»: {}", sub.name, e));
            }
        }
    }

    for custom in sources.custom_links.iter().filter(|c| c.enabled) {
        if let Some(link) = parse_link(custom.raw.trim()) {
            let key = link.dedup_key();
            if seen.insert(key) {
                links.push(link);
            }
        } else {
            errors.push(format!("Не удалось разобрать ссылку: {}", custom.label));
        }
    }

    FetchAllResult { links, errors }
}
