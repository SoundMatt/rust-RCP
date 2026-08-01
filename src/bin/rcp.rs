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
//! `ROADMAP.md` Milestone 10 ("CLI (`rust-rcp`) command surface updated:
//! discovery, register read/write, per-endpoint drive commands, replacing
//! the old `send`/`zones`/`status --zone` shape"): this file's command
//! surface is rebuilt against the OPEN Alliance TC18 Remote Control
//! Protocol Specification v0.5.1_RC's `(`[`rcp::avtp::StreamId`]`,
//! byte_bus_id)`-addressed endpoint model — the same core
//! [`rcp::adapt`]'s own Milestone 10 rebuild targets — in place of the
//! retired `Zone`/`Command`/`Controller`/`Registry` model. `zones`/`send`/
//! `status --zone` are gone; `discover`/`register`/`endpoint` take their
//! place. `version`/`capabilities`/`status` are unchanged in shape (none
//! of them ever referenced `Zone`), with `capabilities`'s
//! `commands`/`interfaces` JSON fields updated to describe the new set.
//! `convert` (rust-RCP-FS-01) is rebuilt against the real canonical
//! `rcp.Message` type RELAY spec §15.5 defines
//! (`byte_bus_id`/`transaction_num`/`control`/`read_size_or_segment`/
//! `body`, per `spec/schemas/rcp-message.json`) and its
//! `ToMessage()`/§15.7.5 conversion, in place of the retired placeholder
//! `Zone`-numbered `Status` document's `zone`/`seq`/`healthy`/`payload`
//! shape that used to map a `zone` field to a `FrontLeft`/…/`Central`
//! positional-speaker name with no TC18 counterpart at all. See
//! `convert_rcp_message`'s own doc comment for the verification this
//! rebuild was checked against.
//!
//! Usage:
//!   rust-rcp version [--format json]
//!   rust-rcp capabilities
//!   rust-rcp status [--format json]
//!   rust-rcp convert --protocol RCP [--format json]
//!   rust-rcp discover [--transaction <n>] [--format json]
//!   rust-rcp register read  [--stream <hex>] [--format json]
//!   rust-rcp register write --payload <hex> [--stream <hex>] [--root]
//!   rust-rcp endpoint read  --bus-id <n> [--stream <hex>] [--ep-type <n>]
//!                           [--initial <hex>] [--read-size <n>] [--format json]
//!   rust-rcp endpoint write --bus-id <n> --payload <hex> [--stream <hex>]
//!                           [--ep-type <n>] [--initial <hex>]
//!   rust-rcp serve --udp <bind-ip> [--port <n>] [--stream <hex>]
//!                  [--max-requests <n>]
//!
//! ## Provenance note
//!
//! `discover`/`register`/`endpoint` each construct and address a fresh
//! in-process [`RcServer`] for the lifetime of one invocation, the same
//! ephemeral-server discipline this file's pre-Milestone-10 `send`/
//! `status --zone` commands already used against a fresh
//! `rcp::mock::MockRegistry` each invocation — not a regression this item
//! introduces, and not something later work reaching for a real transport
//! needs to preserve. Each invocation therefore starts from
//! [`GeneralRegisters::default`] and an empty endpoint table; there is no
//! state carried between separate `rust-rcp` invocations. This is flagged
//! here per Guiding Principle 5 rather than left an unstated limitation.
//!
//! `serve` (added alongside [`rcp::udp::StdUdpSocket`]) is this binary's
//! first command backed by a real OS socket rather than an in-process
//! `RcServer` invoked directly: it runs [`rcp::udp::UdpRcServer`] —
//! previously only ever exercised in this crate's own unit tests, against
//! mock sockets — over a real, bound [`rcp::udp::StdUdpSocket`], serving
//! real inbound UDP datagrams until `--max-requests` is reached (default:
//! unbounded, run until a fatal socket error). Before this item, this
//! crate had no concrete `rcp::udp::UdpSocket` implementation over a real
//! OS socket at all — only the in-process [`rcp::mock::RcServer`] test
//! double and `rcp::udp`'s own unit-test fakes existed, and every command
//! above predates that gap closing. `discover`/`register`/`endpoint`
//! remain deliberately ephemeral/in-process per the note above; `serve` is
//! the new, real-network-facing counterpart, not a replacement for them.

use std::io::Read;
use std::process;
use std::sync::Arc;
use std::time::Instant;

use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;
use rcp::acf::{AcfAbbMessage, ByteMessageInfo, ReadSizeOrSegment};
use rcp::avtp::StreamId;
use rcp::discovery;
use rcp::ep0::EP0_BYTE_BUS_ID;
use rcp::mock::{MockEndpoint, RcServer};
use rcp::regmap::{EndpointType, GeneralRegisters};
use rcp::udp::{StdUdpSocket, UdpRcServer};

const TOOL: &str = "rust-rcp";
const PROTOCOL: &str = "RCP";
const PROTOCOL_INT: u8 = 5;

fn main() {
    let args: Vec<String> = std::env::args().collect();

    if args.len() < 2 {
        eprintln!("Usage: rust-rcp <command> [options]");
        eprintln!(
            "Commands: version, capabilities, status, convert, discover, register, endpoint, serve"
        );
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
        // "fragmentation" reflects ROADMAP.md Milestone 8's "go" decision:
        // crate::fragment::FragmentReassemblyBuffer implements ms/segment_num
        // multi-AVTPDU reassembly bounded by rx_stream_max_request_size.
        // "commands"/"interfaces" updated by Milestone 10's CLI rebuild:
        // "send"/"zones" -> "discover"/"register"/"endpoint";
        // "Controller"/"Registry" -> "RcServer"/"Endpoint" (this file's new
        // backing types — see this file's own doc comment).
        //
        // "no-live-subscribe" (in "features"): RELAY spec §10.4/§15.7.5
        // already documents that RCP has no server-initiated push and that
        // `Node::subscribe` is expected to return a permanently-empty
        // stream for this protocol — see rcp::adapt's own "Provenance
        // note" for how `RcpAdapter::subscribe`
        // (crate::relay::Node::subscribe) realizes that as an
        // immediately-closed channel, and for the still-open question of
        // whether "immediately closed" vs. "stays open but never yields"
        // is the more faithful reading of "permanently-empty stream." This
        // makes that limitation machine-readable here rather than leaving
        // a caller of the `Adapt()`-wrapped `Node` to discover it only by
        // observing an empty channel at runtime. Added to "features"
        // (a free-form, protocol-specific string array per §12.2) rather
        // than as a new top-level property: `relay conform --strict`'s
        // §12.2 schema check rejects unrecognized top-level properties, and
        // "features" is the schema's own extension point for exactly this
        // kind of protocol-specific detail.
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
                    "    \"commands\": [\"version\",\"capabilities\",\"status\",\"convert\",\"discover\",\"register\",\"endpoint\"],\n",
                    "    \"transports\": [],\n",
                    "    \"features\": [\"loaning\",\"fragmentation\",\"no-live-subscribe\"],\n",
                    "    \"interfaces\": [\"RcServer\",\"Endpoint\"],\n",
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
        // fusa:req REQ-CLI-008
        // The old --zone-addressed subscription branch is gone (there is no
        // Zone/Controller left to subscribe against — see rcp::mock's own
        // doc comment on why RcServer models no live-notification mechanism
        // in its place). What remains is exactly the protocol-agnostic
        // system-status document this command already produced regardless
        // of --zone.
        "status" => {
            let format = flag_value(&args, "--format").unwrap_or("text");

            if format == "json" {
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
            match convert_rcp_message(input.trim()) {
                Ok(json) => println!("{}", json),
                Err(()) => {
                    eprintln!("ErrInvalidInput");
                    process::exit(1);
                }
            }
        }

        // ── discover ──────────────────────────────────────────────────────────
        // fusa:req REQ-CLI-001
        "discover" => cmd_discover(&args),

        // ── register read / register write ──────────────────────────────────
        // fusa:req REQ-CLI-002
        // fusa:req REQ-CLI-005
        "register" => cmd_register(&args),

        // ── endpoint read / endpoint write ───────────────────────────────────
        // fusa:req REQ-CLI-004
        // fusa:req REQ-CLI-005
        "endpoint" => cmd_endpoint(&args),

        // ── serve ─────────────────────────────────────────────────────────────
        // fusa:req REQ-CLI-010
        "serve" => cmd_serve(&args),

        cmd => {
            eprintln!("unknown command: {}", cmd);
            process::exit(1);
        }
    }
}

// ── discover ─────────────────────────────────────────────────────────────────

/// `rust-rcp discover [--transaction <n>] [--format json]`.
///
/// Builds a discovery request via [`discovery::build_discovery_request`],
/// then answers it — against a fresh in-process [`RcServer`] (see this
/// file's own doc comment) — via [`discovery::build_discovery_response`],
/// and decodes/prints the resulting [`GeneralRegisters`] snapshot.
// fusa:req REQ-CLI-001
fn cmd_discover(args: &[String]) {
    let transaction_num = parse_u8_arg(args, "--transaction").unwrap_or(0);
    let format = flag_value(args, "--format").unwrap_or("text");

    let server = RcServer::new(GeneralRegisters::default());
    let request = discovery::build_discovery_request(transaction_num);

    let response = match discovery::build_discovery_response(
        &request.info,
        server.state(),
        &server.general_registers(),
    ) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("error: {}", e);
            process::exit(3);
        }
    };

    let regs = match GeneralRegisters::decode(&response.payload) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("error: {}", e);
            process::exit(3);
        }
    };

    if format == "json" {
        println!(
            concat!(
                "{{\n",
                "    \"transaction_num\": {tn},\n",
                "    \"state\": \"{state:?}\",\n",
                "    \"registers\": {regs}\n",
                "}}"
            ),
            tn = transaction_num,
            state = server.state(),
            regs = serde_json::to_string(&regs).unwrap(),
        );
    } else {
        println!(
            "discover: transaction={} state={:?}",
            transaction_num,
            server.state()
        );
        println!(
            "  svr_oa_tc18_magic_nr=0x{:08x} svr_version={} svr_vendor_id=0x{:04x} svr_device_id=0x{:04x} svr_ep_count={}",
            regs.svr_oa_tc18_magic_nr, regs.svr_version, regs.svr_vendor_id, regs.svr_device_id, regs.svr_ep_count,
        );
    }
}

// ── register read / register write ──────────────────────────────────────────

/// `rust-rcp register <read|write> [options]`.
///
/// Dispatches to [`cmd_register_read`]/[`cmd_register_write`], both
/// addressed via [`EP0_BYTE_BUS_ID`] through [`RcServer::handle_abb`].
// fusa:req REQ-CLI-002
fn cmd_register(args: &[String]) {
    match args.get(2).map(String::as_str) {
        Some("read") => cmd_register_read(args),
        Some("write") => cmd_register_write(args),
        _ => {
            eprintln!("usage: rust-rcp register <read|write> [options]");
            process::exit(1);
        }
    }
}

/// `rust-rcp register read [--stream <hex>] [--format json]`.
///
/// Reads the whole general register map via an EP0-addressed
/// [`AcfAbbMessage`] read request, dispatched through
/// [`RcServer::handle_abb`] — never root-client-gated, per
/// [`rcp::ep0::check_ep0_access_for_stream`]'s own doc comment.
// fusa:req REQ-CLI-005
fn cmd_register_read(args: &[String]) {
    let stream = parse_stream_arg(args, "--stream").unwrap_or_else(|| StreamId::from_u64(0));
    let format = flag_value(args, "--format").unwrap_or("text");

    let server = RcServer::new(GeneralRegisters::default());
    let request = AcfAbbMessage {
        info: ByteMessageInfo {
            byte_bus_id: EP0_BYTE_BUS_ID,
            op: false,
            read_size_segment: ReadSizeOrSegment(u16::MAX),
            ..ByteMessageInfo::default()
        },
        payload: Vec::new(),
    };

    let response = match server.handle_abb(stream, &request) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("error: {}", e);
            process::exit(3);
        }
    };

    let regs = match GeneralRegisters::decode(&response.payload) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("error: {}", e);
            process::exit(3);
        }
    };

    if format == "json" {
        println!(
            "{{\n    \"stream\": \"{stream}\",\n    \"registers\": {regs}\n}}",
            stream = format_stream_hex(stream),
            regs = serde_json::to_string(&regs).unwrap(),
        );
    } else {
        println!("register read: stream={}", format_stream_hex(stream));
        println!(
            "  svr_oa_tc18_magic_nr=0x{:08x} svr_version={} svr_vendor_id=0x{:04x} svr_device_id=0x{:04x} svr_ep_count={}",
            regs.svr_oa_tc18_magic_nr, regs.svr_version, regs.svr_vendor_id, regs.svr_device_id, regs.svr_ep_count,
        );
    }
}

/// `rust-rcp register write --payload <hex> [--stream <hex>] [--root]`.
///
/// Writes `--payload` to the whole general register map via an
/// EP0-addressed [`AcfAbbMessage`] write request, dispatched through
/// [`RcServer::handle_abb`]. If `--root` is given, `--stream` is first
/// designated the server's root client via [`RcServer::set_root_client`]
/// before the write is attempted — without it, a non-root stream is
/// rejected with `RcpError::UnauthorizedAccess` before ever reaching the
/// write-policy check.
///
/// Reports whatever [`RcServer::handle_abb`] actually returns, including
/// `RcpError::LockedMemAccess` for the root client itself: see
/// [`RcServer::handle_abb`]'s own doc comment for why a general-register
/// write is currently never actually accepted by this in-process server.
// fusa:req REQ-CLI-005
fn cmd_register_write(args: &[String]) {
    let stream = parse_stream_arg(args, "--stream").unwrap_or_else(|| StreamId::from_u64(0));
    let payload = match parse_hex_arg(args, "--payload") {
        Some(p) => p,
        None => {
            eprintln!("error: --payload required");
            process::exit(1);
        }
    };

    let server = RcServer::new(GeneralRegisters::default());
    if has_flag(args, "--root") {
        server.set_root_client(Some(stream));
    }

    let request = AcfAbbMessage {
        info: ByteMessageInfo {
            byte_bus_id: EP0_BYTE_BUS_ID,
            op: true,
            read_size_segment: ReadSizeOrSegment(payload.len() as u16),
            ..ByteMessageInfo::default()
        },
        payload,
    };

    match server.handle_abb(stream, &request) {
        Ok(_) => println!("register write: ok stream={}", format_stream_hex(stream)),
        Err(e) => {
            eprintln!("error: {}", e);
            process::exit(3);
        }
    }
}

// ── endpoint read / endpoint write ──────────────────────────────────────────

/// `rust-rcp endpoint <read|write> [options]`.
///
/// Dispatches to [`cmd_endpoint_read`]/[`cmd_endpoint_write`], both
/// addressed via `(--stream, --bus-id)` through [`RcServer::handle_abb`]'s
/// `DeviceEndpoint` route.
// fusa:req REQ-CLI-004
fn cmd_endpoint(args: &[String]) {
    match args.get(2).map(String::as_str) {
        Some("read") => cmd_endpoint_read(args),
        Some("write") => cmd_endpoint_write(args),
        _ => {
            eprintln!("usage: rust-rcp endpoint <read|write> [options]");
            process::exit(1);
        }
    }
}

/// `rust-rcp endpoint read --bus-id <n> [--stream <hex>] [--ep-type <n>]
/// [--initial <hex>] [--read-size <n>] [--format json]`.
///
/// Registers a fresh [`MockEndpoint`] of `--ep-type` (default
/// [`EndpointType::Gpio`]) holding `--initial` (default empty) under
/// `(--stream, --bus-id)`, then issues a read request for `--read-size`
/// bytes (default `u8::MAX`, matching [`rcp::adapt::from_message`]'s own
/// default) via [`RcServer::handle_abb`].
// fusa:req REQ-CLI-005
fn cmd_endpoint_read(args: &[String]) {
    let stream = parse_stream_arg(args, "--stream").unwrap_or_else(|| StreamId::from_u64(0));
    let bus_id = match parse_u16_arg(args, "--bus-id") {
        Some(b) => b,
        None => {
            eprintln!("error: --bus-id required");
            process::exit(1);
        }
    };
    let ep_type = parse_ep_type_arg(args).unwrap_or_else(|| {
        eprintln!("error: bad --ep-type");
        process::exit(1);
    });
    let initial = parse_hex_arg(args, "--initial").unwrap_or_default();
    let read_size = parse_u16_arg(args, "--read-size").unwrap_or(u16::MAX);
    let format = flag_value(args, "--format").unwrap_or("text");

    let server = RcServer::new(GeneralRegisters::default());
    let endpoint = MockEndpoint::new(ep_type, initial);
    if let Err(e) = server.register_endpoint(stream, bus_id, endpoint) {
        eprintln!("error: {}", e);
        process::exit(2);
    }

    let request = AcfAbbMessage {
        info: ByteMessageInfo {
            byte_bus_id: bus_id,
            op: false,
            read_size_segment: ReadSizeOrSegment(read_size),
            ..ByteMessageInfo::default()
        },
        payload: Vec::new(),
    };

    match server.handle_abb(stream, &request) {
        Ok(response) => {
            if format == "json" {
                println!(
                    "{{\n    \"stream\": \"{stream}\",\n    \"bus_id\": {bus_id},\n    \"payload\": \"{payload}\"\n}}",
                    stream = format_stream_hex(stream),
                    bus_id = bus_id,
                    payload = hex_encode(&response.payload),
                );
            } else {
                println!(
                    "endpoint read: stream={} bus_id={} payload={}",
                    format_stream_hex(stream),
                    bus_id,
                    hex_encode(&response.payload)
                );
            }
        }
        Err(e) => {
            eprintln!("error: {}", e);
            process::exit(3);
        }
    }
}

/// `rust-rcp endpoint write --bus-id <n> --payload <hex> [--stream <hex>]
/// [--ep-type <n>] [--initial <hex>]`.
///
/// Registers a fresh [`MockEndpoint`] of `--ep-type` (default
/// [`EndpointType::Gpio`]) holding `--initial` (default empty) under
/// `(--stream, --bus-id)`, then issues a write request carrying
/// `--payload` via [`RcServer::handle_abb`].
// fusa:req REQ-CLI-005
fn cmd_endpoint_write(args: &[String]) {
    let stream = parse_stream_arg(args, "--stream").unwrap_or_else(|| StreamId::from_u64(0));
    let bus_id = match parse_u16_arg(args, "--bus-id") {
        Some(b) => b,
        None => {
            eprintln!("error: --bus-id required");
            process::exit(1);
        }
    };
    let ep_type = parse_ep_type_arg(args).unwrap_or_else(|| {
        eprintln!("error: bad --ep-type");
        process::exit(1);
    });
    let initial = parse_hex_arg(args, "--initial").unwrap_or_default();
    let payload = match parse_hex_arg(args, "--payload") {
        Some(p) => p,
        None => {
            eprintln!("error: --payload required");
            process::exit(1);
        }
    };

    let server = RcServer::new(GeneralRegisters::default());
    let endpoint = MockEndpoint::new(ep_type, initial);
    if let Err(e) = server.register_endpoint(stream, bus_id, endpoint) {
        eprintln!("error: {}", e);
        process::exit(2);
    }

    let request = AcfAbbMessage {
        info: ByteMessageInfo {
            byte_bus_id: bus_id,
            op: true,
            read_size_segment: ReadSizeOrSegment(payload.len() as u16),
            ..ByteMessageInfo::default()
        },
        payload,
    };

    match server.handle_abb(stream, &request) {
        Ok(_) => println!(
            "endpoint write: ok stream={} bus_id={}",
            format_stream_hex(stream),
            bus_id
        ),
        Err(e) => {
            eprintln!("error: {}", e);
            process::exit(3);
        }
    }
}

// ── serve ────────────────────────────────────────────────────────────────────

/// `rust-rcp serve --udp <bind-ip> [--port <n>] [--stream <hex>]
/// [--max-requests <n>]`.
///
/// Binds a real [`StdUdpSocket`] to `--udp:--port` (default
/// [`rcp::udp::ANNEX_J_CONTROL_PORT`]) and runs [`UdpRcServer`] against it
/// — a fresh [`RcServer`] with [`GeneralRegisters::default`] and an empty
/// endpoint table, the same starting state `discover`/`register`/
/// `endpoint` already use (see this file's own "Provenance note") — until
/// `--max-requests` requests have been served (default: unbounded; runs
/// until a fatal socket error). Each served request is logged to stdout.
///
/// This is the first `rust-rcp` command to talk to a real OS socket rather
/// than dispatching directly against an in-process `RcServer` — see this
/// file's own module doc comment.
// fusa:req REQ-CLI-010
fn cmd_serve(args: &[String]) {
    let bind_ip_str = match flag_value(args, "--udp") {
        Some(ip) => ip,
        None => {
            eprintln!("error: --udp <bind-ip> required (e.g. --udp 0.0.0.0)");
            process::exit(1);
        }
    };
    let bind_ip: std::net::IpAddr = match bind_ip_str.parse() {
        Ok(ip) => ip,
        Err(e) => {
            eprintln!("error: invalid --udp address {bind_ip_str:?}: {e}");
            process::exit(1);
        }
    };
    let port = parse_u16_arg(args, "--port").unwrap_or(rcp::udp::ANNEX_J_CONTROL_PORT);
    let local_stream = parse_stream_arg(args, "--stream").unwrap_or_else(|| StreamId::from_u64(0));
    let max_requests = parse_u32_arg(args, "--max-requests");

    let socket = match StdUdpSocket::bind(std::net::SocketAddr::new(bind_ip, port)) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: {}", e);
            process::exit(3);
        }
    };
    let bound_addr = socket
        .local_addr()
        .map(|a| a.to_string())
        .unwrap_or_else(|_| format!("{bind_ip}:{port}"));

    let rc_server = RcServer::new(GeneralRegisters::default());
    let server = UdpRcServer::new(local_stream, Arc::new(socket), rc_server);

    println!(
        "serve: listening on udp {} (stream={})",
        bound_addr,
        format_stream_hex(local_stream)
    );

    let mut served: u32 = 0;
    loop {
        if let Some(max) = max_requests {
            if served >= max {
                println!("serve: reached --max-requests={max}, stopping");
                break;
            }
        }
        match server.serve_one(
            None,
            served as u8,
            Instant::now(),
            discovery::DISCOVERY_TIME_OUT,
        ) {
            Ok(()) => {
                served += 1;
                println!("serve: dispatched request #{served}");
            }
            Err(e) => {
                eprintln!("serve: fatal error after {served} request(s): {e}");
                process::exit(3);
            }
        }
    }
}

// ── §11.2 / §15.5 rcp.Message → relay.Message conversion ────────────────────
//
// RELAY spec §11.2's `convert` driver reads one canonical-type value for the
// declared protocol as JSON on stdin and writes the resulting `relay.Message`
// as JSON on stdout, running it through "the implementation's own
// `ToMessage()` conversion... the same code path the implementation uses at
// runtime". For RCP that canonical type is `rcp.Message` (RELAY spec §15.5,
// `spec/schemas/rcp-message.json`): `byte_bus_id`/`transaction_num`/
// `control`/`read_size_or_segment`/`timestamp`/`body`. §15.7.5 states the
// conversion this function implements; RELAY's own Go reference package
// (`rcp.Message.ToMessage()`) is the golden oracle `relay interop` checks
// this binary's `convert` output against.
//
// rust-RCP-FS-01: this replaces the retired placeholder `Zone`-numbered
// `Status` document's `zone`/`seq`/`healthy`/`payload` shape (which mapped a
// numeric `zone` to a `FrontLeft`/…/`Central` positional-speaker name with no
// TC18 counterpart) with the real RCP addressing scheme — decimal
// `byte_bus_id` (0-255), matching `rcp.EndpointIDString`/`ParseEndpointID` in
// RELAY's own reference package — rather than any positional name.
//
// Verified byte-for-byte EQUIVALENT (per `relay interop`'s own
// `canonicalJSON` comparison, which round-trips both outputs through Go's
// `relay.Message` before comparing, so JSON key order/whitespace does not
// matter) against a from-source build of the RELAY v2.0 reference tool's
// `relay convert --protocol RCP`, across a write request with a body, a read
// request with `read_size_or_segment` set, and a minimal all-zero request —
// this function's own tests below pin the same three cases. This does not
// call [`rcp::adapt::to_message`]/[`rcp::adapt::from_message`]: those bind
// this crate's own [`AcfAbbMessage`]/`(StreamId, byte_bus_id)`-addressed
// runtime model, which carries fields (`stream_id`, `evt`, `hs`, `cs`, `ms`,
// `pad`, `mtv`, `rsp`, `ack`) `rcp.Message`/`ToMessage()` has no slot for at
// all and does not itself address by a bare `byte_bus_id` the way
// `EndpointIDString` expects (see `adapt.rs`'s own doc comment on its
// still-unreconciled `Message.id` encoding) — reconciling that broader,
// already-flagged design question is separate, larger work this narrow fix
// does not take on.
//
// Field-level parsing mirrors `rcp.Message`'s own Go zero-value decode
// semantics: a missing field defaults to zero/absent, and RELAY's reference
// `convert` driver does not additionally validate a decoded `rcp.Message`
// (unlike CAN/LIN/SOMEIP, whose canonical types go through a `Validate()`
// call first) — only a malformed value or one that overflows its declared
// field width is rejected. This mirrors that leniency rather than enforcing
// `rcp-message.json`'s stricter `"required"` list, which the reference
// implementation itself does not enforce at this layer.
fn convert_rcp_message(raw: &str) -> Result<String, ()> {
    const CONTROL_WRITE: u64 = 0x20;
    const CONTROL_ERROR: u64 = 0x08;

    let v: serde_json::Value = serde_json::from_str(raw).map_err(|_| ())?;
    let obj = v.as_object().ok_or(())?;

    // additionalProperties: false — reject unknown fields.
    for key in obj.keys() {
        match key.as_str() {
            "byte_bus_id"
            | "transaction_num"
            | "control"
            | "read_size_or_segment"
            | "timestamp"
            | "body" => {}
            _ => return Err(()),
        }
    }

    let u_field = |name: &str, max: u64| -> Result<u64, ()> {
        match obj.get(name) {
            None | Some(serde_json::Value::Null) => Ok(0),
            Some(n) => n.as_u64().filter(|&n| n <= max).ok_or(()),
        }
    };

    let byte_bus_id = u_field("byte_bus_id", 255)?;
    let transaction_num = u_field("transaction_num", u16::MAX as u64)?;
    let control = u_field("control", 255)?;
    let read_size_or_segment = u_field("read_size_or_segment", u16::MAX as u64)?;
    // "timestamp" is accepted for shape validity but carries no weight in
    // the output: ToMessage() never forwards the native AVTP timestamp —
    // relay.Message.Timestamp always reflects (normalized-to-zero) receipt
    // time instead, matching every other protocol's own §15.7 conversion.
    u_field("timestamp", u64::MAX)?;

    let body: Option<Vec<u8>> = match obj.get("body") {
        None | Some(serde_json::Value::Null) => None,
        Some(serde_json::Value::String(s)) => Some(BASE64.decode(s).map_err(|_| ())?),
        _ => return Err(()),
    };

    let op = if control & CONTROL_WRITE != 0 {
        "write"
    } else {
        "read"
    };
    let error = control & CONTROL_ERROR != 0;
    let payload_json = match &body {
        None => "null".to_string(),
        Some(b) => format!("\"{}\"", BASE64.encode(b)),
    };

    Ok(format!(
        concat!(
            "{{",
            "\"protocol\":{proto_int},",
            "\"version\":{{\"major\":0,\"minor\":0,\"patch\":0}},",
            "\"id\":\"{id}\",",
            "\"payload\":{payload},",
            "\"timestamp\":\"0001-01-01T00:00:00Z\",",
            "\"meta\":{{",
            "\"rcp.error\":\"{error}\",",
            "\"rcp.op\":\"{op}\",",
            "\"rcp.read_size_or_segment\":\"{read_size_or_segment}\",",
            "\"rcp.transaction_num\":\"{transaction_num}\"",
            "}}",
            "}}"
        ),
        proto_int = PROTOCOL_INT,
        id = byte_bus_id,
        payload = payload_json,
        error = error,
        op = op,
        read_size_or_segment = read_size_or_segment,
        transaction_num = transaction_num,
    ))
}

// ── helpers ───────────────────────────────────────────────────────────────────

/// Parse a `--stream` flag as a bare hex `StreamId` wire value (no `0x`
/// prefix), mirroring [`rcp::adapt::format_endpoint_id`]'s own hex
/// rendering of a `StreamId` — this file's addressing convention stays
/// consistent with the already-rebuilt `Adapt()` surface rather than
/// inventing a second one.
fn parse_stream_arg(args: &[String], flag: &str) -> Option<StreamId> {
    flag_value(args, flag)
        .and_then(|v| u64::from_str_radix(v, 16).ok())
        .map(StreamId::from_u64)
}

/// Render a `StreamId` the same way [`parse_stream_arg`] expects it back:
/// bare lowercase hex, no `0x` prefix.
fn format_stream_hex(stream: StreamId) -> String {
    format!("{:016x}", stream.to_u64())
}

/// Parse an `--ep-type` flag as a raw `ep_type` byte
/// ([`EndpointType::from_u8`]), defaulting to [`EndpointType::Gpio`] when
/// absent. `Some(Err)` from a present-but-unrecognized byte is surfaced as
/// `None` here; the caller reports it as a usage error.
fn parse_ep_type_arg(args: &[String]) -> Option<EndpointType> {
    match parse_u8_arg(args, "--ep-type") {
        None => Some(EndpointType::Gpio),
        Some(raw) => EndpointType::from_u8(raw).ok(),
    }
}

fn has_flag(args: &[String], flag: &str) -> bool {
    args.iter().any(|a| a == flag)
}

fn parse_u16_arg(args: &[String], flag: &str) -> Option<u16> {
    flag_value(args, flag).and_then(|v| v.parse().ok())
}

fn parse_u8_arg(args: &[String], flag: &str) -> Option<u8> {
    flag_value(args, flag).and_then(|v| v.parse().ok())
}

fn parse_u32_arg(args: &[String], flag: &str) -> Option<u32> {
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
    use rcp::RcpError;

    fn stream(unique_id: u16) -> StreamId {
        StreamId::new([0x02, 0x11, 0x22, 0x33, 0x44, 0x55], unique_id)
    }

    #[test]
    // fusa:test REQ-CLI-002
    fn flag_value_finds_option() {
        let args: Vec<String> = vec![
            "rcp".into(),
            "register".into(),
            "write".into(),
            "--payload".into(),
            "deadbeef".into(),
        ];
        assert_eq!(flag_value(&args, "--payload"), Some("deadbeef"));
        assert_eq!(flag_value(&args, "--stream"), None);
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
        let args: Vec<String> = vec!["rcp".into(), "register".into(), "write".into()];
        assert!(parse_hex_arg(&args, "--payload").is_none());
    }

    #[test]
    // fusa:test REQ-CLI-002
    fn parse_u16_arg_parses_decimal() {
        let args: Vec<String> = vec!["rcp".into(), "--bus-id".into(), "42".into()];
        assert_eq!(parse_u16_arg(&args, "--bus-id"), Some(42u16));
    }

    #[test]
    // fusa:test REQ-CLI-010
    fn parse_u32_arg_parses_decimal() {
        let args: Vec<String> = vec!["rcp".into(), "--max-requests".into(), "5".into()];
        assert_eq!(parse_u32_arg(&args, "--max-requests"), Some(5u32));
    }

    #[test]
    // fusa:test REQ-CLI-010
    fn parse_u32_arg_absent_returns_none() {
        let args: Vec<String> = vec!["rcp".into(), "serve".into()];
        assert!(parse_u32_arg(&args, "--max-requests").is_none());
    }

    #[test]
    // fusa:test REQ-CLI-002
    fn parse_u8_arg_parses_transaction() {
        let args: Vec<String> = vec!["rcp".into(), "--transaction".into(), "2".into()];
        assert_eq!(parse_u8_arg(&args, "--transaction"), Some(2u8));
    }

    #[test]
    // fusa:test REQ-CLI-002
    fn has_flag_detects_bare_flag() {
        let args: Vec<String> = vec![
            "rcp".into(),
            "register".into(),
            "write".into(),
            "--root".into(),
        ];
        assert!(has_flag(&args, "--root"));
        assert!(!has_flag(&args, "--other"));
    }

    #[test]
    // fusa:test REQ-CLI-002
    fn parse_stream_arg_roundtrips_hex() {
        let sid = stream(0x1234);
        let hex = format_stream_hex(sid);
        let args: Vec<String> = vec!["rcp".into(), "--stream".into(), hex];
        assert_eq!(parse_stream_arg(&args, "--stream"), Some(sid));
    }

    #[test]
    // fusa:test REQ-CLI-002
    fn parse_stream_arg_absent_returns_none() {
        let args: Vec<String> = vec!["rcp".into()];
        assert!(parse_stream_arg(&args, "--stream").is_none());
    }

    #[test]
    // fusa:test REQ-CLI-002
    fn parse_ep_type_arg_defaults_to_gpio() {
        let args: Vec<String> = vec!["rcp".into()];
        assert_eq!(parse_ep_type_arg(&args), Some(EndpointType::Gpio));
    }

    #[test]
    // fusa:test REQ-CLI-002
    fn parse_ep_type_arg_rejects_unrecognized_byte() {
        let args: Vec<String> = vec!["rcp".into(), "--ep-type".into(), "255".into()];
        assert_eq!(parse_ep_type_arg(&args), None);
    }

    #[test]
    // fusa:test REQ-CLI-003
    // fusa:test REQ-CLI-006
    fn spec_version_is_non_empty() {
        assert!(!rcp::SPEC_VERSION.is_empty());
    }

    #[test]
    // fusa:test REQ-CLI-006
    fn spec_version_is_relay_2_0() {
        assert_eq!(rcp::SPEC_VERSION, "2.0", "must track RELAY spec v2.0");
    }

    #[test]
    // fusa:test REQ-SPEC-001
    fn relay_spec_version_alias_matches_spec_version() {
        assert_eq!(rcp::RELAY_SPEC_VERSION, rcp::SPEC_VERSION);
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

    // ── discover ──────────────────────────────────────────────────────────────

    #[test]
    // fusa:test REQ-CLI-001
    fn discover_request_is_recognized_and_answered() {
        let server = RcServer::new(GeneralRegisters {
            svr_vendor_id: 0x1234,
            ..Default::default()
        });
        let request = discovery::build_discovery_request(7);
        assert!(discovery::is_discovery_request(&request));

        let response = discovery::build_discovery_response(
            &request.info,
            server.state(),
            &server.general_registers(),
        )
        .unwrap();
        let regs = GeneralRegisters::decode(&response.payload).unwrap();
        assert_eq!(regs.svr_vendor_id, 0x1234);
    }

    // ── register read / register write ───────────────────────────────────────

    #[test]
    // fusa:test REQ-CLI-005
    fn register_read_returns_general_registers_snapshot() {
        let server = RcServer::new(GeneralRegisters {
            svr_device_id: 0xBEEF,
            ..Default::default()
        });
        let sid = stream(1);
        let request = AcfAbbMessage {
            info: ByteMessageInfo {
                byte_bus_id: EP0_BYTE_BUS_ID,
                op: false,
                read_size_segment: ReadSizeOrSegment(u16::MAX),
                ..Default::default()
            },
            payload: Vec::new(),
        };
        let response = server.handle_abb(sid, &request).unwrap();
        let regs = GeneralRegisters::decode(&response.payload).unwrap();
        assert_eq!(regs.svr_device_id, 0xBEEF);
    }

    #[test]
    // fusa:test REQ-CLI-005
    fn register_write_succeeds_for_the_root_client() {
        // Mirrors rcp::mock::rc_server_tests::ep0_write_succeeds_for_the_root_client
        // — RegisterCategory::General now has LockPolicy::W, writable
        // whenever reachable, so a write from the root client succeeds.
        // This is the CLI's own exercise of that same, already-tested gate.
        let server = RcServer::new(GeneralRegisters::default());
        let sid = stream(1);
        server.set_root_client(Some(sid));

        let payload = GeneralRegisters {
            svr_vendor_id: 0xAAAA,
            ..Default::default()
        }
        .encode()
        .to_vec();
        let request = AcfAbbMessage {
            info: ByteMessageInfo {
                byte_bus_id: EP0_BYTE_BUS_ID,
                op: true,
                read_size_segment: ReadSizeOrSegment(payload.len() as u16),
                ..Default::default()
            },
            payload,
        };
        server.handle_abb(sid, &request).unwrap();
        assert_eq!(server.general_registers().svr_vendor_id, 0xAAAA);
    }

    // ── endpoint read / endpoint write ───────────────────────────────────────

    #[test]
    // fusa:test REQ-CLI-004
    fn endpoint_read_returns_registered_endpoint_payload() {
        let server = RcServer::new(GeneralRegisters::default());
        let sid = stream(1);
        let endpoint = MockEndpoint::new(EndpointType::Gpio, vec![0xAA, 0xBB]);
        server.register_endpoint(sid, 7, endpoint).unwrap();

        let request = AcfAbbMessage {
            info: ByteMessageInfo {
                byte_bus_id: 7,
                op: false,
                read_size_segment: ReadSizeOrSegment(u16::MAX),
                ..Default::default()
            },
            payload: Vec::new(),
        };
        let response = server.handle_abb(sid, &request).unwrap();
        assert_eq!(response.payload, vec![0xAA, 0xBB]);
    }

    #[test]
    // fusa:test REQ-CLI-004
    fn endpoint_write_replaces_endpoint_buffer() {
        let server = RcServer::new(GeneralRegisters::default());
        let sid = stream(1);
        let endpoint = MockEndpoint::new(EndpointType::Gpio, vec![0x00]);
        server.register_endpoint(sid, 7, endpoint).unwrap();

        let write_req = AcfAbbMessage {
            info: ByteMessageInfo {
                byte_bus_id: 7,
                op: true,
                read_size_segment: ReadSizeOrSegment(2),
                ..Default::default()
            },
            payload: vec![0xCC, 0xDD],
        };
        server.handle_abb(sid, &write_req).unwrap();

        let read_req = AcfAbbMessage {
            info: ByteMessageInfo {
                byte_bus_id: 7,
                op: false,
                read_size_segment: ReadSizeOrSegment(u16::MAX),
                ..Default::default()
            },
            payload: Vec::new(),
        };
        let response = server.handle_abb(sid, &read_req).unwrap();
        assert_eq!(response.payload, vec![0xCC, 0xDD]);
    }

    #[test]
    // fusa:test REQ-CLI-004
    fn endpoint_read_unregistered_bus_id_is_ep_not_found() {
        let server = RcServer::new(GeneralRegisters::default());
        let sid = stream(1);
        let request = AcfAbbMessage {
            info: ByteMessageInfo {
                byte_bus_id: 99,
                op: false,
                read_size_segment: ReadSizeOrSegment(u16::MAX),
                ..Default::default()
            },
            payload: Vec::new(),
        };
        let err = server.handle_abb(sid, &request).unwrap_err();
        assert_eq!(err, RcpError::EpNotFound);
    }

    // ── §11.2 convert tests ───────────────────────────────────────────────────
    //
    // Every case pinned here was independently cross-checked against a
    // from-source build of the RELAY v2.0 reference tool's
    // `relay convert --protocol RCP` — see `convert_rcp_message`'s own doc
    // comment.

    #[test]
    // fusa:test REQ-CLI-009
    fn convert_official_golden_vector() {
        // RELAY spec/vectors/rcp-message.json — the exact vector
        // `relay interop` feeds this binary's `convert` in CI.
        let input = r#"{"byte_bus_id":9,"transaction_num":42,"control":32,"read_size_or_segment":4,"body":"qg=="}"#;
        let output = convert_rcp_message(input).unwrap();
        let v: serde_json::Value = serde_json::from_str(&output).unwrap();
        assert_eq!(v["protocol"], 5);
        assert_eq!(v["id"], "9");
        assert_eq!(v["payload"], "qg==");
        assert_eq!(v["timestamp"], "0001-01-01T00:00:00Z");
        assert_eq!(v["meta"]["rcp.op"], "write");
        assert_eq!(v["meta"]["rcp.error"], "false");
        assert_eq!(v["meta"]["rcp.transaction_num"], "42");
        assert_eq!(v["meta"]["rcp.read_size_or_segment"], "4");
    }

    #[test]
    // fusa:test REQ-CLI-009
    fn convert_write_with_body_uses_decimal_byte_bus_id_as_id() {
        let input = r#"{"byte_bus_id":7,"transaction_num":17,"control":32,"body":"3q2+7w=="}"#;
        let output = convert_rcp_message(input).unwrap();
        let v: serde_json::Value = serde_json::from_str(&output).unwrap();
        assert_eq!(v["id"], "7");
        assert_eq!(v["payload"], "3q2+7w==");
        assert_eq!(v["meta"]["rcp.op"], "write");
    }

    #[test]
    // fusa:test REQ-CLI-009
    fn convert_read_with_read_size_omits_no_payload_field() {
        let input =
            r#"{"byte_bus_id":1,"transaction_num":17,"control":64,"read_size_or_segment":4}"#;
        let output = convert_rcp_message(input).unwrap();
        let v: serde_json::Value = serde_json::from_str(&output).unwrap();
        assert_eq!(v["id"], "1");
        assert!(v["payload"].is_null());
        assert_eq!(v["meta"]["rcp.op"], "read");
        assert_eq!(v["meta"]["rcp.read_size_or_segment"], "4");
    }

    #[test]
    // fusa:test REQ-CLI-009
    fn convert_minimal_request_defaults_absent_fields_to_zero() {
        let input = r#"{"byte_bus_id":0,"control":32}"#;
        let output = convert_rcp_message(input).unwrap();
        let v: serde_json::Value = serde_json::from_str(&output).unwrap();
        assert_eq!(v["id"], "0");
        assert!(v["payload"].is_null());
        assert_eq!(v["meta"]["rcp.transaction_num"], "0");
        assert_eq!(v["meta"]["rcp.read_size_or_segment"], "0");
    }

    #[test]
    // fusa:test REQ-CLI-009
    fn convert_error_control_bit_sets_rcp_error_meta() {
        let input = r#"{"byte_bus_id":99,"control":72}"#; // Read | Error
        let output = convert_rcp_message(input).unwrap();
        let v: serde_json::Value = serde_json::from_str(&output).unwrap();
        assert_eq!(v["id"], "99");
        assert_eq!(v["meta"]["rcp.op"], "read");
        assert_eq!(v["meta"]["rcp.error"], "true");
    }

    #[test]
    // fusa:test REQ-CLI-009
    fn convert_rejects_byte_bus_id_out_of_range() {
        // byte_bus_id is 0-255 (u8) per spec/schemas/rcp-message.json;
        // the real reference implementation rejects an out-of-range value
        // as a decode-time type error, not a separate validation step.
        let input = r#"{"byte_bus_id":300,"control":32}"#;
        assert!(convert_rcp_message(input).is_err());
    }

    #[test]
    // fusa:test REQ-CLI-009
    fn convert_rejects_unknown_field() {
        let input = r#"{"byte_bus_id":1,"control":32,"extra":"bad"}"#;
        assert!(convert_rcp_message(input).is_err());
    }

    #[test]
    // fusa:test REQ-CLI-009
    fn convert_rejects_malformed_json() {
        assert!(convert_rcp_message("not json").is_err());
        assert!(convert_rcp_message("[]").is_err());
    }

    #[test]
    // fusa:test REQ-CLI-009
    fn convert_rejects_invalid_body_base64() {
        let input = r#"{"byte_bus_id":1,"control":32,"body":"not-base64!!"}"#;
        assert!(convert_rcp_message(input).is_err());
    }
}
