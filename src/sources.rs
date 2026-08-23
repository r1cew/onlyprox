use crate::models::{CustomLink, SourcesConfig, Subscription};
use crate::parser::parse_link;

/// Добавляет подписку по URL. Имя, если не задано, генерируется
/// автоматически по порядковому номеру.
pub fn add_subscription(cfg: &mut SourcesConfig, name: String, url: String) {
    let url = url.trim().to_string();
    if url.is_empty() {
        return;
    }
    // Не добавляем повторно один и тот же URL.
    if cfg.subscriptions.iter().any(|s| s.url == url) {
        return;
    }
    let name = if name.trim().is_empty() {
        format!("Подписка #{}", cfg.subscriptions.len() + 1)
    } else {
        name.trim().to_string()
    };
    cfg.subscriptions.push(Subscription { name, url, enabled: true });
}

pub fn remove_subscription(cfg: &mut SourcesConfig, index: usize) {
    if index < cfg.subscriptions.len() {
        cfg.subscriptions.remove(index);
    }
}

pub fn toggle_subscription(cfg: &mut SourcesConfig, index: usize) {
    if let Some(sub) = cfg.subscriptions.get_mut(index) {
        sub.enabled = !sub.enabled;
    }
}

/// Добавляет ссылку на конкретный сервер (vless://, vmess://, trojan://, ss://),
/// введённую пользователем вручную. Возвращает ошибку, если ссылка не
/// распознана ни одним из поддерживаемых парсеров.
pub fn add_custom_link(cfg: &mut SourcesConfig, raw: String) -> Result<(), String> {
    let raw = raw.trim().to_string();
    if raw.is_empty() {
        return Err("Пустая ссылка".to_string());
    }
    let link = parse_link(&raw).ok_or_else(|| {
        "Не удалось разобрать ссылку. Поддерживаются vless://, vmess://, trojan://, ss://".to_string()
    })?;

    if cfg.custom_links.iter().any(|c| c.raw == raw) {
        return Err("Такая ссылка уже добавлена".to_string());
    }

    let label = {
        let remark = link.remark();
        if remark.is_empty() {
            link.dedup_key()
        } else {
            remark.to_string()
        }
    };

    cfg.custom_links.push(CustomLink { raw, enabled: true, label });
    Ok(())
}

pub fn remove_custom_link(cfg: &mut SourcesConfig, index: usize) {
    if index < cfg.custom_links.len() {
        cfg.custom_links.remove(index);
    }
}

pub fn toggle_custom_link(cfg: &mut SourcesConfig, index: usize) {
    if let Some(link) = cfg.custom_links.get_mut(index) {
        link.enabled = !link.enabled;
    }
}
