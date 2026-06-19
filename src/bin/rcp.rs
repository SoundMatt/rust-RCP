// fusa:req REQ-CLI-001
// fusa:req REQ-CLI-002
// fusa:req REQ-CLI-003
// fusa:req REQ-CLI-004
// fusa:req REQ-CLI-005

//! RCP command-line interface.
//!
//! Usage:
//!   rcp send  --zone <zone> --type <cmd_type> [--priority <p>] [--payload <hex>]
//!   rcp status --zone <zone>
//!   rcp version

use std::process;
use std::time::Duration;

fn main() {
    let args: Vec<String> = std::env::args().collect();

    if args.len() < 2 {
        eprintln!("Usage: rcp <command> [options]");
        eprintln!("Commands: send, status, version, zones");
        process::exit(1);
    }

    match args[1].as_str() {
        // fusa:req REQ-CLI-003
        "version" => {
            println!("rcp {} (RELAY spec {})", env!("CARGO_PKG_VERSION"), rcp::SPEC_VERSION);
        }

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
                    let cmd = rcp::Command { id: 1, zone, cmd_type, priority, payload };
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

        // fusa:req REQ-CLI-005
        "status" => {
            let zone = parse_zone_arg(&args, "--zone").unwrap_or_else(|| {
                eprintln!("error: --zone required");
                process::exit(1)
            });
            let registry = rcp::mock::MockRegistry::new();
            match registry.lookup(zone) {
                Err(e) => {
                    eprintln!("error: {}", e);
                    process::exit(2);
                }
                Ok(ctrl) => {
                    let sub = ctrl.subscribe().unwrap();
                    println!("subscribed to zone {}; waiting for status...", zone);
                    match sub.recv_timeout(Duration::from_secs(5)) {
                        Some(s) => println!("seq={} healthy={}", s.seq, s.healthy),
                        None    => println!("no status received within 5s"),
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

    #[test]
    // fusa:test REQ-CLI-001
    // fusa:test REQ-CLI-002
    fn flag_value_finds_option() {
        let args: Vec<String> = vec!["rcp".into(), "send".into(), "--zone".into(), "front-left".into(), "--type".into(), "1".into()];
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
    fn spec_version_is_non_empty() {
        assert!(!rcp::SPEC_VERSION.is_empty());
    }

    #[test]
    // fusa:test REQ-CLI-004
    fn all_zones_have_string_names() {
        for z in [rcp::Zone::FRONT_LEFT, rcp::Zone::FRONT_RIGHT,
                  rcp::Zone::REAR_LEFT,  rcp::Zone::REAR_RIGHT, rcp::Zone::CENTRAL] {
            assert!(!z.as_str().is_empty());
        }
    }

    #[test]
    // fusa:test REQ-CLI-005
    fn mock_registry_has_all_zones_for_status() {
        let registry = rcp::mock::MockRegistry::new();
        for zone in [rcp::Zone::FRONT_LEFT, rcp::Zone::FRONT_RIGHT,
                     rcp::Zone::REAR_LEFT,  rcp::Zone::REAR_RIGHT, rcp::Zone::CENTRAL] {
            assert!(registry.lookup(zone).is_ok(), "zone {:?} should be registered", zone);
        }
    }

    #[test]
    // fusa:test REQ-CLI-001
    fn mock_registry_send_returns_ok() {
        let registry = rcp::mock::MockRegistry::new();
        let ctrl = registry.lookup(rcp::Zone::FRONT_LEFT).unwrap();
        let cmd = rcp::Command { id: 1, zone: rcp::Zone::FRONT_LEFT, ..Default::default() };
        let resp = ctrl.send(&cmd, Some(std::time::Duration::from_secs(1))).unwrap();
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
}
