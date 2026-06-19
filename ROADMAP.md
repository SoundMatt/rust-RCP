# rust-RCP Roadmap

## v0.1 — Initial Release (current)

- All 42 protocol modules implemented and unit-tested
- Wire format encoder/decoder with magic-byte and CRC validation
- End-to-end protection (CRC-16/CCITT-FALSE + 32-entry replay guard)
- Priority queue, rate limiter, deadline controller, watchdog monitor
- Power state machine (Active/Sleep/Standby)
- Hot-standby redundancy controller
- Observability hooks and metrics
- TSN traffic-class annotation
- Authorization policy controller
- Firmware update chunking (OTA)
- Bus bridges: CAN FD, LIN, SOME/IP, MQTT, DDS, UDP, shared memory
- Protocol bridges: gRPC, REST, TLS, UDS, DoIP
- Service discovery: mDNS-SD
- C API (`capi`) for FFI embedding
- Protocol adapter framework
- ISO 26262 ASIL-B FuSa artifacts (HARA, safety plan, gap report tooling)
- IEC 62443 SL-2 cybersecurity artifacts
- CI: lint, cross-platform tests, coverage ≥90%, fuzz, benchmark, audit

## v0.2 — Hardening

- [ ] Real mDNS-SD backend (using `mdns-sd` crate) as optional feature
- [ ] AUTOSAR-CP transport backend (optional feature)
- [ ] `async-controller` feature: tokio-based async wrappers around blocking API
- [ ] Persistent audit log module (`auditlog`) for forensic traceability
- [ ] Formal property test suite using `proptest`
- [ ] Codec registry for pluggable serialization (CBOR, Protobuf, Cap'n Proto)
- [ ] ASIL-D mode: independent CRC validator + diversity controller

## v0.3 — Fleet Integration

- [ ] `federation` upgrade: mutual TLS between vehicle registries
- [ ] MQTT v5 quality-of-service guarantees in `mqttbr`
- [ ] DDS-RPC typed topic generation from `codegen` schemas
- [ ] Certificate lifecycle manager (renewal, revocation check)
- [ ] Latency histogram exporter (Prometheus-compatible)
- [ ] Hardware Security Module (HSM) key backend for `tlstransport`

## v1.0 — Production

- [ ] Third-party ISO 26262 tool qualification assessment
- [ ] MISRA-Rust advisory review
- [ ] Stable public API with semver guarantees
- [ ] Crates.io publish
- [ ] Embedded no_std port (core-only feature flag)
- [ ] AUTOSAR Adaptive Platform integration layer

## Compliance Targets

| Standard | Target | Status |
|----------|--------|--------|
| ISO 26262:2018 | ASIL-B | In progress (v0.1 artifacts complete) |
| IEC 62443-4-2 | SL-2 | In progress (v0.1 artifacts complete) |
| ISO 21434:2021 | WP.10 threat model | Implemented in `iso21434` module |
| AUTOSAR CP R23-11 | COM/PDU layer | Planned v0.3 |
