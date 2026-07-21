use base64::{engine::general_purpose::STANDARD, engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use url::Url;


pub async fn parse() -> Result<(), Box<dyn std::error::Error>> {
    let resp = reqwest::get("https://raw.githubusercontent.com/kort0881/vpn-vless-configs-russia/refs/heads/main/data/githubmirror/clean/vless.txt")
        .await?
        .text()
        .await?;

    let supported = vec!["vless://", "vmess://", "ss://", "trojan://", "hysteria://"]; // , "hy2://"

    for (i, line) in resp
        .lines()
        .map(|l| l.trim())
        .filter(|line| supported.iter().any(|p| line.starts_with(p)))
        .enumerate()
    {
        if let Ok(proxy) = parse_link(line) {
            // Преобразуем прокси в готовый JSON-конфиг Xray
            let xray_config = build_xray_config(&proxy);

            println!("--- Конфиг #{} ({}) ---", i + 1, proxy.remark());
            println!("{}", serde_json::to_string_pretty(&xray_config)?);

            // Для примера выведем только первые 2 рабочего конфига

        }
    }

    Ok(())
}

// ============================================================================
// Генератор полного JSON-конфига для Xray
// ============================================================================
pub fn build_xray_config(proxy: &ProxyConfig) -> Value {
    let outbound_proxy = proxy.to_xray_outbound();

    json!({
        "log": {
            "loglevel": "warning"
        },
        "dns": {
            "hosts": {
                "dns.google": ["8.8.8.8"]
            },
            "servers": [
                "1.1.1.1",
                "8.8.8.8",
                "https://dns.google/dns-query"
            ]
        },
        "inbounds": [
            {
                "tag": "socks",
                "port": 10808,
                "listen": "127.0.0.1",
                "protocol": "mixed",
                "sniffing": {
                    "enabled": true,
                    "destOverride": ["http", "tls"],
                    "routeOnly": false
                },
                "settings": {
                    "auth": "noauth",
                    "udp": true,
                    "allowTransparent": false
                }
            }
        ],
        "outbounds": [
            outbound_proxy,
            {
                "tag": "direct",
                "protocol": "freedom"
            },
            {
                "tag": "block",
                "protocol": "blackhole"
            }
        ],
        "routing": {
            "domainStrategy": "AsIs",
            "rules": [
                {
                    "type": "field",
                    "port": "0-65535",
                    "outboundTag": "proxy"
                }
            ]
        }
    })
}

// ============================================================================
// Модели и конвертация протоколов в Xray Outbound
// ============================================================================

#[derive(Debug)]
pub enum ProxyConfig {
    Vless(VlessConfig),
    Vmess(VmessConfig),
    Trojan(TrojanConfig),
    Shadowsocks(ShadowsocksConfig),
}

impl ProxyConfig {
    pub fn remark(&self) -> &str {
        match self {
            ProxyConfig::Vless(c) => &c.remark,
            ProxyConfig::Vmess(c) => &c.remark,
            ProxyConfig::Trojan(c) => &c.remark,
            ProxyConfig::Shadowsocks(c) => &c.remark,
        }
    }

    pub fn to_xray_outbound(&self) -> Value {
        match self {
            ProxyConfig::Vless(c) => c.to_outbound(),
            ProxyConfig::Vmess(c) => c.to_outbound(),
            ProxyConfig::Trojan(c) => c.to_outbound(),
            ProxyConfig::Shadowsocks(c) => c.to_outbound(),
        }
    }
}

#[derive(Debug)]
pub struct VlessConfig {
    pub uuid: String,
    pub address: String,
    pub port: u16,
    pub params: HashMap<String, String>,
    pub remark: String,
}

impl VlessConfig {
    fn to_outbound(&self) -> Value {
        let net = self.params.get("type").map(|s| s.as_str()).unwrap_or("tcp");
        let security = self.params.get("security").map(|s| s.as_str()).unwrap_or("none");
        let flow = self.params.get("flow").cloned().unwrap_or_default();

        let mut stream_settings = json!({
            "network": net,
            "security": security
        });

        // Настройка REALITY
        if security == "reality" {
            stream_settings["realitySettings"] = json!({
                "serverName": self.params.get("sni").unwrap_or(&String::new()),
                "fingerprint": self.params.get("fp").unwrap_or(&"chrome".to_string()),
                "show": false,
                "publicKey": self.params.get("pbk").unwrap_or(&String::new()),
                "shortId": self.params.get("sid").unwrap_or(&String::new()),
                "spiderX": self.params.get("spx").unwrap_or(&String::new())
            });
        } 
        // Настройка TLS
        else if security == "tls" {
            stream_settings["tlsSettings"] = json!({
                "serverName": self.params.get("sni").unwrap_or(&String::new()),
                "fingerprint": self.params.get("fp").unwrap_or(&"chrome".to_string()),
                "allowInsecure": self.params.get("allowInsecure").map(|v| v == "1" || v == "true").unwrap_or(false)
            });
        }

        // Настройка WebSocket / gRPC транспортов
        if net == "ws" {
            stream_settings["wsSettings"] = json!({
                "path": self.params.get("path").unwrap_or(&"/".to_string()),
                "headers": {
                    "Host": self.params.get("host").unwrap_or(&String::new())
                }
            });
        } else if net == "grpc" {
            stream_settings["grpcSettings"] = json!({
                "serviceName": self.params.get("serviceName").unwrap_or(&String::new())
            });
        }

        json!({
            "tag": "proxy",
            "protocol": "vless",
            "settings": {
                "vnext": [
                    {
                        "address": self.address,
                        "port": self.port,
                        "users": [
                            {
                                "id": self.uuid,
                                "encryption": "none",
                                "flow": flow,
                                "level": 0
                            }
                        ]
                    }
                ]
            },
            "streamSettings": stream_settings,
            "mux": {
                "enabled": false,
                "concurrency": -1
            }
        })
    }
}

#[derive(Debug)]
pub struct VmessConfig {
    pub remark: String,
    pub address: String,
    pub port: u16,
    pub uuid: String,
    pub security: String,
    pub network: String,
    pub path: Option<String>,
    pub host: Option<String>,
    pub tls: Option<String>,
    pub sni: Option<String>,
}

impl VmessConfig {
    fn to_outbound(&self) -> Value {
        let sec = if self.tls.as_deref() == Some("tls") { "tls" } else { "none" };

        let mut stream_settings = json!({
            "network": self.network,
            "security": sec
        });

        if sec == "tls" {
            stream_settings["tlsSettings"] = json!({
                "serverName": self.sni.as_deref().unwrap_or("")
            });
        }

        if self.network == "ws" {
            stream_settings["wsSettings"] = json!({
                "path": self.path.as_deref().unwrap_or("/"),
                "headers": {
                    "Host": self.host.as_deref().unwrap_or("")
                }
            });
        }

        json!({
            "tag": "proxy",
            "protocol": "vmess",
            "settings": {
                "vnext": [
                    {
                        "address": self.address,
                        "port": self.port,
                        "users": [
                            {
                                "id": self.uuid,
                                "alterId": 0,
                                "security": if self.security.is_empty() { "auto" } else { &self.security },
                                "level": 0
                            }
                        ]
                    }
                ]
            },
            "streamSettings": stream_settings
        })
    }
}

#[derive(Debug)]
pub struct ShadowsocksConfig {
    pub method: String,
    pub password: String,
    pub address: String,
    pub port: u16,
    pub remark: String,
}

impl ShadowsocksConfig {
    fn to_outbound(&self) -> Value {
        json!({
            "tag": "proxy",
            "protocol": "shadowsocks",
            "settings": {
                "servers": [
                    {
                        "address": self.address,
                        "port": self.port,
                        "method": self.method,
                        "password": self.password,
                        "uot": true
                    }
                ]
            }
        })
    }
}

#[derive(Debug)]
pub struct TrojanConfig {
    pub password: String,
    pub address: String,
    pub port: u16,
    pub params: HashMap<String, String>,
    pub remark: String,
}

impl TrojanConfig {
    fn to_outbound(&self) -> Value {
        json!({
            "tag": "proxy",
            "protocol": "trojan",
            "settings": {
                "servers": [
                    {
                        "address": self.address,
                        "port": self.port,
                        "password": self.password
                    }
                ]
            },
            "streamSettings": {
                "network": self.params.get("type").unwrap_or(&"tcp".to_string()),
                "security": "tls",
                "tlsSettings": {
                    "serverName": self.params.get("sni").unwrap_or(&String::new())
                }
            }
        })
    }
}

// ============================================================================
// Функции Парсинга Ссылок
// ============================================================================

pub fn parse_link(link: &str) -> Result<ProxyConfig, Box<dyn std::error::Error>> {
    if link.starts_with("vless://") {
        Ok(ProxyConfig::Vless(parse_vless(link)?))
    } else if link.starts_with("vmess://") {
        Ok(ProxyConfig::Vmess(parse_vmess(link)?))
    } else if link.starts_with("ss://") {
        Ok(ProxyConfig::Shadowsocks(parse_ss(link)?))
    } else if link.starts_with("trojan://") {
        Ok(ProxyConfig::Trojan(parse_trojan(link)?))
    } else {
        Err("Unsupported protocol".into())
    }
}

fn parse_vless(link: &str) -> Result<VlessConfig, Box<dyn std::error::Error>> {
    let url = Url::parse(link)?;
    let uuid = url.username().to_string();
    let address = url.host_str().ok_or("No host")?.to_string();
    let port = url.port().unwrap_or(443);
    let remark = url::form_urlencoded::parse(url.fragment().unwrap_or("").as_bytes())
        .map(|(k, _)| k)
        .collect::<Vec<_>>()
        .join("");

    let params: HashMap<String, String> = url.query_pairs().into_owned().collect();

    Ok(VlessConfig { uuid, address, port, params, remark })
}

fn parse_vmess(link: &str) -> Result<VmessConfig, Box<dyn std::error::Error>> {
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

    Ok(VmessConfig {
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

fn parse_ss(link: &str) -> Result<ShadowsocksConfig, Box<dyn std::error::Error>> {
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

    Ok(ShadowsocksConfig { method, password, address, port, remark })
}

fn parse_trojan(link: &str) -> Result<TrojanConfig, Box<dyn std::error::Error>> {
    let url = Url::parse(link)?;
    let password = url.username().to_string();
    let address = url.host_str().ok_or("No host")?.to_string();
    let port = url.port().unwrap_or(443);
    let remark = url.fragment().unwrap_or("").to_string();
    let params: HashMap<String, String> = url.query_pairs().into_owned().collect();

    Ok(TrojanConfig { password, address, port, params, remark })
}

fn decode_b64(input: &str) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let cleaned = input.trim().replace("\r", "").replace("\n", "");
    if let Ok(data) = URL_SAFE_NO_PAD.decode(&cleaned) { return Ok(data); }
    if let Ok(data) = STANDARD.decode(&cleaned) { return Ok(data); }

    let mut padded = cleaned.clone();
    while padded.len() % 4 != 0 { padded.push('='); }
    STANDARD.decode(&padded).map_err(|e| e.into())
}