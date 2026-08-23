use crate::models::*;
use base64::{engine::general_purpose::STANDARD, engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use serde::Deserialize;
use std::collections::HashMap;
use url::Url;

pub fn parse_subscription_feed(content: &str) -> Vec<ProxyLink> {
    content
        .lines()
        .map(|l| l.trim())
        .filter_map(parse_link)
        .collect()
}

pub fn parse_link(link: &str) -> Option<ProxyLink> {
    if link.starts_with("vless://") {
        parse_vless(link).ok().map(ProxyLink::Vless)
    } else if link.starts_with("vmess://") {
        parse_vmess(link).ok().map(ProxyLink::Vmess)
    } else if link.starts_with("trojan://") {
        parse_trojan(link).ok().map(ProxyLink::Trojan)
    } else if link.starts_with("ss://") {
        parse_ss(link).ok().map(ProxyLink::Shadowsocks)
    } else {
        None
    }
}

fn parse_vless(link: &str) -> Result<VlessData, Box<dyn std::error::Error>> {
    let url = Url::parse(link)?;
    let uuid = url.username().to_string();
    let address = url.host_str().ok_or("No host")?.to_string();
    let port = url.port().unwrap_or(443);
    let remark = url::form_urlencoded::parse(url.fragment().unwrap_or("").as_bytes())
        .map(|(k, _)| k)
        .collect::<Vec<_>>()
        .join("");

    let params: HashMap<String, String> = url.query_pairs().into_owned().collect();

    Ok(VlessData { uuid, address, port, params, remark })
}

fn parse_vmess(link: &str) -> Result<VmessData, Box<dyn std::error::Error>> {
    let b64_str = link.trim_start_matches("vmess://");
    let decoded = decode_b64(b64_str)?;

    #[derive(Deserialize)]
    struct RawVmess {
        ps: Option<String>,
        add: String,
        port: serde_json::Value,
        id: String,
        scy: Option<String>,
        net: Option<String>,
        path: Option<String>,
        host: Option<String>,
        tls: Option<String>,
        sni: Option<String>,
    }

    let raw: RawVmess = serde_json::from_slice(&decoded)?;
    let port = match raw.port {
        serde_json::Value::Number(n) => n.as_u64().unwrap_or(443) as u16,
        serde_json::Value::String(s) => s.parse().unwrap_or(443),
        _ => 443,
    };

    Ok(VmessData {
        remark: raw.ps.unwrap_or_default(),
        address: raw.add,
        port,
        uuid: raw.id,
        security: raw.scy.unwrap_or_default(),
        network: raw.net.unwrap_or_else(|| "tcp".to_string()),
        path: raw.path,
        host: raw.host,
        tls: raw.tls,
        sni: raw.sni,
    })
}

fn parse_trojan(link: &str) -> Result<TrojanData, Box<dyn std::error::Error>> {
    let url = Url::parse(link)?;
    let password = url.username().to_string();
    let address = url.host_str().ok_or("No host")?.to_string();
    let port = url.port().unwrap_or(443);
    let remark = url.fragment().unwrap_or("").to_string();
    let params: HashMap<String, String> = url.query_pairs().into_owned().collect();

    Ok(TrojanData { password, address, port, params, remark })
}

fn parse_ss(link: &str) -> Result<SsData, Box<dyn std::error::Error>> {
    let url = Url::parse(link)?;
    let address = url.host_str().ok_or("No host")?.to_string();
    let port = url.port().ok_or("No port")?;
    let remark = url.fragment().unwrap_or("").to_string();

    let user_info = url.username();
    let (method, password) = if user_info.contains(':') {
        let parts: Vec<&str> = user_info.splitn(2, ':').collect();
        (parts[0].to_string(), parts[1].to_string())
    } else {
        let decoded = String::from_utf8(decode_b64(user_info)?)?;
        let parts: Vec<&str> = decoded.splitn(2, ':').collect();
        (parts[0].to_string(), parts[1].to_string())
    };

    Ok(SsData { method, password, address, port, remark })
}

fn decode_b64(input: &str) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let cleaned = input.trim().replace('\r', "").replace('\n', "");
    if let Ok(data) = URL_SAFE_NO_PAD.decode(&cleaned) {
        return Ok(data);
    }
    if let Ok(data) = STANDARD.decode(&cleaned) {
        return Ok(data);
    }

    let mut padded = cleaned;
    while padded.len() % 4 != 0 {
        padded.push('=');
    }
    STANDARD.decode(&padded).map_err(|e| e.into())
}
