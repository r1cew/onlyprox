    use crate::models::*;
    use serde_json::{json, Value};

    pub fn build_outbound_from_link(link: &ProxyLink) -> Value {
        match link {
            ProxyLink::Vless(vless) => build_vless_outbound(vless),
            ProxyLink::Vmess(vmess) => build_vmess_outbound(vmess),
            ProxyLink::Trojan(trojan) => build_trojan_outbound(trojan),
            ProxyLink::Shadowsocks(ss) => build_ss_outbound(ss),
        }
    }

    fn build_vless_outbound(vless: &VlessData) -> Value {
        let net = vless.params.get("type").map(|s| s.as_str()).unwrap_or("tcp");
        let security = vless.params.get("security").map(|s| s.as_str()).unwrap_or("none");
        let flow = vless.params.get("flow").cloned().unwrap_or_default();

        let mut stream_settings = json!({
            "network": net,
            "security": security
        });

        if security == "reality" {
            stream_settings["realitySettings"] = json!({
                "serverName": vless.params.get("sni").unwrap_or(&String::new()),
                "fingerprint": vless.params.get("fp").unwrap_or(&"chrome".to_string()),
                "show": false,
                "publicKey": vless.params.get("pbk").unwrap_or(&String::new()),
                "shortId": vless.params.get("sid").unwrap_or(&String::new()),
                "spiderX": vless.params.get("spx").unwrap_or(&String::new())
            });
        } else if security == "tls" {
            stream_settings["tlsSettings"] = json!({
                "serverName": vless.params.get("sni").unwrap_or(&String::new()),
                "fingerprint": vless.params.get("fp").unwrap_or(&"chrome".to_string()),
                "allowInsecure": vless.params.get("allowInsecure").map(|v| v == "1" || v == "true").unwrap_or(false)
            });
        }

        if net == "ws" {
            stream_settings["wsSettings"] = json!({
                "path": vless.params.get("path").unwrap_or(&"/".to_string()),
                "headers": { "Host": vless.params.get("host").unwrap_or(&String::new()) }
            });
        } else if net == "grpc" {
            stream_settings["grpcSettings"] = json!({
                "serviceName": vless.params.get("serviceName").unwrap_or(&String::new())
            });
        }

        json!({
            "tag": "proxy",
            "protocol": "vless",
            "settings": {
                "vnext": [{
                    "address": vless.address,
                    "port": vless.port,
                    "users": [{
                        "id": vless.uuid,
                        "encryption": "none",
                        "flow": flow,
                        "level": 0
                    }]
                }]
            },
            "streamSettings": stream_settings,
            "mux": { "enabled": false, "concurrency": -1 }
        })
    }

    fn build_vmess_outbound(vmess: &VmessData) -> Value {
        let sec = if vmess.tls.as_deref() == Some("tls") { "tls" } else { "none" };

        let mut stream_settings = json!({
            "network": vmess.network,
            "security": sec
        });

        if sec == "tls" {
            stream_settings["tlsSettings"] = json!({
                "serverName": vmess.sni.as_deref().unwrap_or("")
            });
        }

        if vmess.network == "ws" {
            stream_settings["wsSettings"] = json!({
                "path": vmess.path.as_deref().unwrap_or("/"),
                "headers": { "Host": vmess.host.as_deref().unwrap_or("") }
            });
        }

        json!({
            "tag": "proxy",
            "protocol": "vmess",
            "settings": {
                "vnext": [{
                    "address": vmess.address,
                    "port": vmess.port,
                    "users": [{
                        "id": vmess.uuid,
                        "alterId": 0,
                        "security": if vmess.security.is_empty() { "auto" } else { &vmess.security },
                        "level": 0
                    }]
                }]
            },
            "streamSettings": stream_settings
        })
    }

    fn build_trojan_outbound(trojan: &TrojanData) -> Value {
        json!({
            "tag": "proxy",
            "protocol": "trojan",
            "settings": {
                "servers": [{
                    "address": trojan.address,
                    "port": trojan.port,
                    "password": trojan.password
                }]
            },
            "streamSettings": {
                "network": trojan.params.get("type").unwrap_or(&"tcp".to_string()),
                "security": "tls",
                "tlsSettings": {
                    "serverName": trojan.params.get("sni").unwrap_or(&String::new())
                }
            }
        })
    }

    fn build_ss_outbound(ss: &SsData) -> Value {
        json!({
            "tag": "proxy",
            "protocol": "shadowsocks",
            "settings": {
                "servers": [{
                    "address": ss.address,
                    "port": ss.port,
                    "method": ss.method,
                    "password": ss.password,
                    "uot": true
                }]
            }
        })
    }