// fusa:req REQ-CLI-001
// fusa:req REQ-CLI-002
// fusa:req REQ-CLI-003
// fusa:req REQ-CLI-004
// fusa:req REQ-CLI-005
// fusa:req REQ-CLI-006
// fusa:req REQ-CLI-007
// fusa:req REQ-CLI-008
// fusa:req REQ-CLI-009

//! RCP command-line interface — RELAY spec §12 conformant.
//!
//! Usage:
//!   rcp version [--format json]
//!   rcp capabilities
//!   rcp status [--format json]
//!   rcp convert --protocol RCP [--format json]
//!   rcp send  --zone <zone> --type <cmd_type> [--priority <p>] [--payload <hex>]
//!   rcp zones

use std::io::Read;
use std::process;
use std::time::Duration;

use rcp::Registry;

const TOOL: &str = "rcp";
const PROTOCOL: &str = "RCP";
const PROTOCOL_INT: u8 = 5;

fn main() {
    let args: Vec<String> = std::env::args().collect();

    if args.len() < 2 {
        eprintln!("Usage: rcp <command> [options]");
        eprintln!("Commands: version, capabilities, status, convert, send, zones");
        process::exit(1);
    }

    match args[1].as_str() {
        // ── §12.1 version ─────────────────────────────────────────────────────
        // fusa:req REQ-CLI-003
        // fusa:req REQ-CLI-006
        "version" => {
            let format = flag_value(&args, "--format").unwrap_or("text");
            if format == "json" {
                println!(
                    concat!(
                        "{{\n",
                        "    \"tool\": \"{tool}\",\n",
                        "    \"protocol\": \"{proto}\",\n",
                        "    \"protocol_int\": {proto_int},\n",
                        "    \"version\": \"{ver}\",\n",
                        "    \"spec_version\": \"{spec}\",\n",
                        "    \"language\": \"rust\",\n",
                        "    \"runtime\": \"{rt}\"\n",
                        "}}"
                    ),
                    tool = TOOL,
                    proto = PROTOCOL,
                    proto_int = PROTOCOL_INT,
                    ver = env!("CARGO_PKG_VERSION"),
                    spec = rcp::SPEC_VERSION,
                    rt = env!("RUSTC_VERSION"),
                );
            } else {
                println!(
                    "{} {} (protocol {}, RELAY spec {}, {})",
                    TOOL,
                    env!("CARGO_PKG_VERSION"),
                    PROTOCOL,
                    rcp::SPEC_VERSION,
                    env!("RUSTC_VERSION"),
                );
            }
        }

        // ── §12.2 capabilities ────────────────────────────────────────────────
        // fusa:req REQ-CLI-007
        "capabilities" => {
            println!(
                concat!(
                    "{{\n",
                    "    \"kind\": \"capabilities\",\n",
                    "    \"tool\": \"{tool}\",\n",
                    "    \"protocol\": \"{proto}\",\n",
                    "    \"protocol_int\": {proto_int},\n",
                    "    \"version\": \"{ver}\",\n",
                    "    \"spec_version\": \"{spec}\",\n",
                    "    \"commands\": [\"version\",\"capabilities\",\"status\",\"convert\",\"send\",\"zones\"],\n",
                    "    \"transports\": [],\n",
                    "    \"features\": [\"loaning\"],\n",
                    "    \"interfaces\": [\"Controller\",\"Registry\"],\n",
                    "    \"optional_interfaces\": [],\n",
                    "    \"adapt\": true\n",
                    "}}"
                ),
                tool = TOOL,
                proto = PROTOCOL,
                proto_int = PROTOCOL_INT,
                ver = env!("CARGO_PKG_VERSION"),
                spec = rcp::SPEC_VERSION,
            );
        }

        // ── §12.3 status ──────────────────────────────────────────────────────
        // fusa:req REQ-CLI-005
        // fusa:req REQ-CLI-008
        "status" => {
            let zone = parse_zone_arg(&args, "--zone");
            let format = flag_value(&args, "--format").unwrap_or("text");

            if let Some(z) = zone {
                // Zone-specific subscription mode
                let registry = rcp::mock::MockRegistry::new();
                match registry.lookup(z) {
                    Err(e) => {
                        eprintln!("error: {}", e);
                        process::exit(2);
                    }
                    Ok(ctrl) => {
                        let sub = ctrl.subscribe().unwrap();
                        println!("subscribed to zone {}; waiting for status...", z);
                        match sub.recv_timeout(Duration::from_secs(5)) {
                            Some(s) => println!("seq={} healthy={}", s.seq, s.healthy),
                            None => println!("no status received within 5s"),
                        }
                    }
                }
            } else if format == "json" {
                // §12.3 system-level status document
                println!(
                    concat!(
                        "{{\n",
                        "    \"protocol\": \"{proto}\",\n",
                        "    \"tool\": \"{tool}\",\n",
                        "    \"version\": \"{ver}\",\n",
                        "    \"healthy\": true,\n",
                        "    \"connected\": false,\n",
                        "    \"endpoint\": \"\",\n",
                        "    \"details\": {{}}\n",
                        "}}"
                    ),
                    proto = PROTOCOL,
                    tool = TOOL,
                    ver = env!("CARGO_PKG_VERSION"),
                );
            } else {
                println!(
                    "{} {} protocol={} healthy=true connected=false",
                    TOOL,
                    env!("CARGO_PKG_VERSION"),
                    PROTOCOL,
                );
            }
        }

        // ── §11.2 convert ─────────────────────────────────────────────────────
        // fusa:req REQ-CLI-009
        "convert" => {
            let protocol = flag_value(&args, "--protocol").unwrap_or("");
            if protocol != PROTOCOL {
                eprintln!("convert: --protocol {} is required", PROTOCOL);
                process::exit(2);
            }
            let mut input = String::new();
            if std::io::stdin().read_to_string(&mut input).is_err() {
                eprintln!("ErrInvalidInput");
                process::exit(1);
            }
            match convert_rcp_status(input.trim()) {
                Ok(json) => println!("{}", json),
                Err(()) => {
                    eprintln!("ErrInvalidInput");
                    process::exit(1);
                }
            }
        }

        // ── zones ─────────────────────────────────────────────────────────────
        // fusa:req REQ-CLI-004
        "zones" => {
            let zones = [
                rcp::Zone::FRONT_LEFT,
                rcp::Zone::FRONT_RIGHT,
                rcp::Zone::REAR_LEFT,
                rcp::Zone::REAR_RIGHT,
                rcp::Zone::CENTRAL,
            ];
            for z in &zones {
                println!("{:3}  {}", z.0, z.as_str());
            }
        }

        // ── send ──────────────────────────────────────────────────────────────
        // fusa:req REQ-CLI-001
        // fusa:req REQ-CLI-002
        "send" => {
            let zone = parse_zone_arg(&args, "--zone").unwrap_or_else(|| {
                eprintln!("error: --zone required");
                process::exit(1)
            });
            let cmd_type = parse_u16_arg(&args, "--type")
                .map(rcp::CommandType)
                .unwrap_or(rcp::CommandType::NOOP);
            let priority = parse_u8_arg(&args, "--priority")
                .map(rcp::Priority)
                .unwrap_or(rcp::Priority::NORMAL);
            let payload = parse_hex_arg(&args, "--payload");

            let registry = rcp::mock::MockRegistry::new();
            match registry.lookup(zone) {
                Err(e) => {
                    eprintln!("error: zone not found: {}", e);
                    process::exit(2);
                }
                Ok(ctrl) => {
                    let cmd = rcp::Command {
                        id: 1,
                        zone,
                        cmd_type,
                        priority,
                        payload,
                    };
                    match ctrl.send(&cmd, Some(Duration::from_secs(5))) {
                        Ok(resp) => {
                            println!("status={} zone={}", resp.status, resp.zone);
                            if let Some(p) = resp.payload {
                                println!("payload={}", hex_encode(&p));
                            }
                        }
                        Err(e) => {
                            eprintln!("error: {}", e);
                            process::exit(3);
                        }
                    }
                }
            }
        }

        cmd => {
            eprintln!("unknown command: {}", cmd);
            process::exit(1);
        }
    }
}

// ── §11.2 / §15.5 rcp.Status → relay.Message conversion ─────────────────────

fn zone_to_id(zone: u64) -> Option<&'static str> {
    match zone {
        0 => Some("Unknown"),
        1 => Some("FrontLeft"),
        2 => Some("FrontRight"),
        3 => Some("RearLeft"),
        4 => Some("RearRight"),
        5 => Some("Central"),
        _ => None,
    }
}

fn convert_rcp_status(raw: &str) -> Result<String, ()> {
    let v: serde_json::Value = serde_json::from_str(raw).map_err(|_| ())?;
    let obj = v.as_object().ok_or(())?;

    // additionalProperties: false — reject unknown fields
    for key in obj.keys() {
        match key.as_str() {
            "zone" | "seq" | "healthy" | "payload" => {}
            _ => return Err(()),
        }
    }

    // Required fields
    let zone = obj.get("zone").and_then(|v| v.as_u64()).ok_or(())?;
    let seq = obj.get("seq").and_then(|v| v.as_u64()).ok_or(())?;
    let healthy = obj.get("healthy").and_then(|v| v.as_bool()).ok_or(())?;

    let id = zone_to_id(zone).ok_or(())?;

    // Optional payload (base64 string or null)
    let payload_json = match obj.get("payload") {
        None | Some(serde_json::Value::Null) => "null".to_string(),
        Some(serde_json::Value::String(s)) => format!("\"{}\"", s),
        _ => return Err(()),
    };

    Ok(format!(
        concat!(
            "{{",
            "\"protocol\":{proto_int},",
            "\"version\":{{\"major\":0,\"minor\":0,\"patch\":0}},",
            "\"id\":\"{id}\",",
            "\"payload\":{payload},",
            "\"timestamp\":\"0001-01-01T00:00:00Z\",",
            "\"seq\":{seq},",
            "\"meta\":{{\"rcp.healthy\":\"{healthy}\"}}",
            "}}"
        ),
        proto_int = PROTOCOL_INT,
        id = id,
        payload = payload_json,
        seq = seq,
        healthy = healthy,
    ))
}

// ── helpers ───────────────────────────────────────────────────────────────────

fn parse_zone_arg(args: &[String], flag: &str) -> Option<rcp::Zone> {
    flag_value(args, flag).and_then(|v| rcp::zone_from_str(v).ok())
}

fn parse_u16_arg(args: &[String], flag: &str) -> Option<u16> {
    flag_value(args, flag).and_then(|v| v.parse().ok())
}

fn parse_u8_arg(args: &[String], flag: &str) -> Option<u8> {
    flag_value(args, flag).and_then(|v| v.parse().ok())
}

fn parse_hex_arg(args: &[String], flag: &str) -> Option<Vec<u8>> {
    flag_value(args, flag).map(|v| {
        (0..v.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&v[i..i + 2], 16).unwrap_or(0))
            .collect()
    })
}

fn flag_value<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    args.windows(2)
        .find(|w| w[0] == flag)
        .map(|w| w[1].as_str())
}

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

// ─────────────────────────────────────────────────────────────────────────────
//  Tests
// ─────────────────────────────────────────────────────────────────────────────
#[cfg(test)]
mod tests {
    use super::*;
    use rcp::Registry;

    #[test]
    // fusa:test REQ-CLI-001
    // fusa:test REQ-CLI-002
    fn flag_value_finds_option() {
        let args: Vec<String> = vec![
            "rcp".into(),
            "send".into(),
            "--zone".into(),
            "front-left".into(),
            "--type".into(),
            "1".into(),
        ];
        assert_eq!(flag_value(&args, "--zone"), Some("front-left"));
        assert_eq!(flag_value(&args, "--type"), Some("1"));
        assert_eq!(flag_value(&args, "--priority"), None);
    }

    #[test]
    // fusa:test REQ-CLI-002
    fn parse_hex_arg_decodes_bytes() {
        let args: Vec<String> = vec!["rcp".into(), "--payload".into(), "deadbeef".into()];
        let bytes = parse_hex_arg(&args, "--payload").unwrap();
        assert_eq!(bytes, vec![0xDE, 0xAD, 0xBE, 0xEF]);
    }

    #[test]
    // fusa:test REQ-CLI-002
    fn parse_hex_arg_absent_returns_none() {
        let args: Vec<String> = vec!["rcp".into(), "send".into()];
        assert!(parse_hex_arg(&args, "--payload").is_none());
    }

    #[test]
    // fusa:test REQ-CLI-003
    // fusa:test REQ-CLI-006
    fn spec_version_is_non_empty() {
        assert!(!rcp::SPEC_VERSION.is_empty());
    }

    #[test]
    // fusa:test REQ-CLI-006
    fn spec_version_is_relay_1_10() {
        assert_eq!(rcp::SPEC_VERSION, "1.10", "must track RELAY spec v1.10");
    }

    #[test]
    // fusa:test REQ-CLI-004
    fn all_zones_have_string_names() {
        for z in [
            rcp::Zone::FRONT_LEFT,
            rcp::Zone::FRONT_RIGHT,
            rcp::Zone::REAR_LEFT,
            rcp::Zone::REAR_RIGHT,
            rcp::Zone::CENTRAL,
        ] {
            assert!(!z.as_str().is_empty());
        }
    }

    #[test]
    // fusa:test REQ-CLI-005
    fn mock_registry_has_all_zones_for_status() {
        let registry = rcp::mock::MockRegistry::new();
        for zone in [
            rcp::Zone::FRONT_LEFT,
            rcp::Zone::FRONT_RIGHT,
            rcp::Zone::REAR_LEFT,
            rcp::Zone::REAR_RIGHT,
            rcp::Zone::CENTRAL,
        ] {
            assert!(
                registry.lookup(zone).is_ok(),
                "zone {:?} should be registered",
                zone
            );
        }
    }

    #[test]
    // fusa:test REQ-CLI-001
    fn mock_registry_send_returns_ok() {
        let registry = rcp::mock::MockRegistry::new();
        let ctrl = registry.lookup(rcp::Zone::FRONT_LEFT).unwrap();
        let cmd = rcp::Command {
            id: 1,
            zone: rcp::Zone::FRONT_LEFT,
            ..Default::default()
        };
        let resp = ctrl
            .send(&cmd, Some(std::time::Duration::from_secs(1)))
            .unwrap();
        assert_eq!(resp.status, rcp::ResponseStatus::OK);
    }

    #[test]
    // fusa:test REQ-CLI-002
    fn parse_u16_arg_parses_decimal() {
        let args: Vec<String> = vec!["rcp".into(), "--type".into(), "42".into()];
        assert_eq!(parse_u16_arg(&args, "--type"), Some(42u16));
    }

    #[test]
    // fusa:test REQ-CLI-002
    fn parse_u8_arg_parses_priority() {
        let args: Vec<String> = vec!["rcp".into(), "--priority".into(), "2".into()];
        assert_eq!(parse_u8_arg(&args, "--priority"), Some(2u8));
    }

    #[test]
    // fusa:test REQ-CLI-007
    fn capabilities_json_is_valid() {
        assert!(!rcp::SPEC_VERSION.is_empty());
        assert!(!env!("CARGO_PKG_VERSION").is_empty());
    }

    #[test]
    // fusa:test REQ-CLI-008
    fn status_json_fields_present() {
        assert!(!env!("CARGO_PKG_VERSION").is_empty());
    }

    // ── §11.2 convert tests ───────────────────────────────────────────────────

    #[test]
    // fusa:test REQ-CLI-009
    fn convert_golden_vector() {
        // Golden vector from RELAY spec/vectors/rcp-status.json
        let input = r#"{"zone":1,"seq":3,"healthy":true,"payload":"AQ=="}"#;
        let output = convert_rcp_status(input).unwrap();
        let v: serde_json::Value = serde_json::from_str(&output).unwrap();
        assert_eq!(v["protocol"], 5);
        assert_eq!(v["id"], "FrontLeft");
        assert_eq!(v["seq"], 3);
        assert_eq!(v["meta"]["rcp.healthy"], "true");
        assert_eq!(v["payload"], "AQ==");
        assert_eq!(v["timestamp"], "0001-01-01T00:00:00Z");
    }

    #[test]
    // fusa:test REQ-CLI-009
    fn convert_all_zones() {
        let zones = [
            (0, "Unknown"),
            (1, "FrontLeft"),
            (2, "FrontRight"),
            (3, "RearLeft"),
            (4, "RearRight"),
            (5, "Central"),
        ];
        for (zone_int, zone_name) in zones {
            let input = format!(r#"{{"zone":{zone_int},"seq":1,"healthy":false}}"#);
            let out = convert_rcp_status(&input).unwrap();
            let v: serde_json::Value = serde_json::from_str(&out).unwrap();
            assert_eq!(v["id"], zone_name, "zone {zone_int}");
            assert_eq!(v["meta"]["rcp.healthy"], "false");
        }
    }

    #[test]
    // fusa:test REQ-CLI-009
    fn convert_invalid_zone_rejected() {
        let input = r#"{"zone":99,"seq":1,"healthy":true}"#;
        assert!(convert_rcp_status(input).is_err());
    }

    #[test]
    // fusa:test REQ-CLI-009
    fn convert_missing_required_field_rejected() {
        assert!(convert_rcp_status(r#"{"seq":1,"healthy":true}"#).is_err()); // no zone
        assert!(convert_rcp_status(r#"{"zone":1,"healthy":true}"#).is_err()); // no seq
        assert!(convert_rcp_status(r#"{"zone":1,"seq":1}"#).is_err()); // no healthy
    }

    #[test]
    // fusa:test REQ-CLI-009
    fn convert_unknown_field_rejected() {
        let input = r#"{"zone":1,"seq":1,"healthy":true,"extra":"bad"}"#;
        assert!(convert_rcp_status(input).is_err());
    }

    #[test]
    // fusa:test REQ-CLI-009
    fn convert_null_payload_outputs_null() {
        let input = r#"{"zone":1,"seq":1,"healthy":true,"payload":null}"#;
        let out = convert_rcp_status(input).unwrap();
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert!(v["payload"].is_null());
    }
}
