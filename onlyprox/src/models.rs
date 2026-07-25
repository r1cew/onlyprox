use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::LazyLock;

pub static APP_DIR: LazyLock<PathBuf> = LazyLock::new(|| {
    std::env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf()
});

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct XrayConfig {
    pub dns: serde_json::Value,
    pub inbounds: Vec<serde_json::Value>,
    pub log: serde_json::Value,
    pub outbounds: Vec<serde_json::Value>,
    pub routing: serde_json::Value,
}

impl XrayConfig {
    pub fn new_with_proxy(outbound_proxy: serde_json::Value, socks_port: u16) -> Self {
        Self {
            log: serde_json::json!({ "loglevel": "warning" }),
            dns: serde_json::json!({
                "hosts": { "dns.google": ["8.8.8.8"] },
                "servers": ["1.1.1.1", "8.8.8.8", "https://dns.google/dns-query"]
            }),
            inbounds: vec![serde_json::json!({
                "tag": "socks",
                "listen": "127.0.0.1",
                "port": socks_port,
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
            })],
            outbounds: vec![
                outbound_proxy,
                serde_json::json!({ "tag": "direct", "protocol": "freedom" }),
                serde_json::json!({ "tag": "block", "protocol": "blackhole" }),
            ],
            routing: serde_json::json!({
                "domainStrategy": "AsIs",
                "rules": [
                    {
                        "type": "field",
                        "port": "0-65535",
                        "outboundTag": "proxy"
                    }
                ]
            }),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ProxyLink {
    Vless(VlessData),
    Vmess(VmessData),
    Trojan(TrojanData),
    Shadowsocks(SsData),
}

impl ProxyLink {
    pub fn remark(&self) -> &str {
        match self {
            ProxyLink::Vless(d) => &d.remark,
            ProxyLink::Vmess(d) => &d.remark,
            ProxyLink::Trojan(d) => &d.remark,
            ProxyLink::Shadowsocks(d) => &d.remark,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VlessData {
    pub uuid: String,
    pub address: String,
    pub port: u16,
    pub params: HashMap<String, String>,
    pub remark: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VmessData {
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrojanData {
    pub password: String,
    pub address: String,
    pub port: u16,
    pub params: HashMap<String, String>,
    pub remark: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SsData {
    pub method: String,
    pub password: String,
    pub address: String,
    pub port: u16,
    pub remark: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TestStage {
    pub name: String,
    pub speedtest: bool,
    pub min_speed_kbps: f64,
    pub threads: usize,
    pub repeats: usize,
    pub interval_sec: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProxyCandidate {
    pub id: String,
    pub link: ProxyLink,
    pub last_latency: u128,
    pub last_speed_kbps: f64,
    pub flag: String,
}

#[derive(Debug)]
pub struct CheckResult {
    pub is_working: bool,
    pub latency_ms: u128,
    pub speed_kbps: f64,
    pub flag: String,
}

#[derive(Debug, Deserialize)]
pub struct CheckIpInfo {
    pub ip: String,
    pub city: Option<String>,
    pub region: Option<String>,
    pub country: Option<String>,
    pub loc: Option<String>,
    pub org: Option<String>,
    pub postal: Option<String>,
    pub timezone: Option<String>,
}