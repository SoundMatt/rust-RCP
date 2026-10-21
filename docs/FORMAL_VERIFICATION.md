# Verification parity with c-RCP: TLA+, Miri, branch coverage

Issue #221 (part of #172). c-RCP runs three kinds of verification beyond its
unit tests: ASan/UBSan, TLA+/TLC model checking, and a ratchet-gated MC/DC
measurement. This document records the rust-RCP equivalent of each, what it
covers, and what it leaves out.

| c-RCP (`.github/workflows/ci.yml`) | rust-RCP CI job | Gate |
|---|---|---|
| `sanitize` (ASan + UBSan, line 86) | `miri` | any Miri error or test failure |
| `formal-verification` (TLC, line 193) | `formal-verification` | any TLC violation or deadlock |
| `mcdc` + "MC/DC regression gate" (line 379) | `branch-coverage` | branch % below `.branch-coverage-ratchet.json` floor |

## TLA+ / TLC

The specs are in `tla/`. Each `<Spec>.cfg` sets `SPECIFICATION FairSpec`, so a
single TLC run checks both the safety properties and the liveness property:

```bash
curl -sSL -o tla2tools.jar https://github.com/tlaplus/tlaplus/releases/download/v1.7.4/tla2tools.jar
cd tla
java -jar ../tla2tools.jar -workers 4 -metadir /tmp/tlc-states LifecycleStateMachine.tla
java -jar ../tla2tools.jar -workers 4 -metadir /tmp/tlc-states E2ESafePoint.tla
java -jar ../tla2tools.jar -workers 4 -metadir /tmp/tlc-states RequestLifecycle.tla
```

| Spec | Origin | Rust code modeled | Properties |
|---|---|---|---|
| `LifecycleStateMachine.tla` | c-RCP `tla/` (a4f3a1f), plus one action, `DemoteRcpToHwConfigured` | `lifecycle::RcServerState::try_transition`, `is_register_writable` (W* lock) | SP1 NoSkipConfiguration, SP2 FieldLockMonotonicWhileConfigured, LP1 EventuallyRcpConfigured |
| `E2ESafePoint.tla` | c-RCP `tla/` (a4f3a1f), unchanged except for a rust mapping comment | SP1 only: `request::check_watchdog_overflow_purge`, `purge_normal_priority_on_watchdog_overflow` | SP1 SafetyRequestsSurvivePurge, SP2 NoUnsafeSafetyExecution, LP1 EventuallySafetyExecutes |
| `RequestLifecycle.tla` | written for rust-RCP (c-RCP has no first-class request lifecycle type) | `request::RequestLifecycleState::try_transition`, `try_force_cancel_{all,non_safestate,single}`, `check_watchdog_overflow_purge` | SP1 NoSkipHop, SP2 FinalizedTerminal, SP3 SafetyRequestsSurvivePurge, SP4 SafestateRequestsSurviveNonSafestateClear, LP1 EventuallyFinalized |

Measured locally with TLC v1.7.4: Lifecycle has 12 distinct states,
E2ESafePoint 256, and RequestLifecycle 56. TLC found no errors in any of them.

### Linking the specs to the code: `src/formal.rs` refinement tests

Before #221, `src/formal.rs` held only generic `Invariant`/`check_all`/`witness`
helpers and checked nothing about the real state machines. Now its
`tests::refinement` module runs breadth-first search over each spec's finite
abstract state space. It computes every successor by calling the real rust
function, then uses `witness` to assert that each observed step is a step of the
spec's `Next` relation and satisfies the spec's safety properties:

- `lifecycle_refines_tla_spec` checks `Next`, SP1, and SP2. It also checks a
  bounded form of LP1: with both inputs consistent, `RcpConfigured` is reachable
  from every reachable state. The consistency closure returns `true` for every
  unguarded shape, so a forbidden transition cannot hide behind the closure.
- `request_lifecycle_refines_tla_spec` covers all 12 `RequestKind`s, every guard
  input shape with both verdicts, all three cancellations, and the purge in both
  its single-request and slice forms. It checks `Next` and SP1-SP4.
- `watchdog_purge_refines_e2e_safe_point_sp1` checks E2ESafePoint SP1 over a
  mixed queue.

The checker accepts a subset of the spec. rust-RCP does not implement the
`RcpConfigured` exits yet (issue #189), and the refinement still holds. When
#189 ports those arrows they are already in the spec, so the tests keep passing
unless the port takes a step the spec does not allow.

Mutations tested locally:
- Adding a `HwUnconfigured -> RcpConfigured` arm to `try_transition` makes
  `lifecycle_refines_tla_spec` fail.
- Dropping the `!is_safety_tagged()` exemption from
  `check_watchdog_overflow_purge` makes both request tests fail.
- Allowing `Pending -> UnderExecution` makes the request-lifecycle test fail.
- Removing `~safety_tagged` from the spec's `WatchdogPurge` makes TLC report
  `SafetyRequestsSurvivePurge is violated`.

### Gaps (not modeled or not linked)

- **E2ESafePoint SP2/LP1.** These model c-RCP's `rcp_e2e_request_may_execute()`
  / `rcp_e2e_endpoint_in_safe_state()` gate, which only executes a
  safety-tagged request once its endpoint reports safe state. rust-RCP has no
  port of that gate yet (see #187 and #203). TLC still checks the shared design,
  but no rust test links SP2/LP1 to code.
- **The liveness properties link to code only in bounded form.** TLC checks the
  fairness-conditioned temporal properties against the spec. The rust tests
  check reachability, not fairness.
- **Abstractions:** authorization, idleness, and plausibility checks are free
  environment booleans, as in c-RCP. The c-RCP abstractions documented in its
  `FORMAL_VERIFICATION.md` "Assumptions and Abstractions" section also apply to
  the two shared specs.

## Miri

`bash scripts/miri.sh` runs `cargo +nightly-2026-10-21 miri test --lib` with
`MIRIFLAGS=-Zmiri-disable-isolation`. Without that flag, tests that read
`CLOCK_REALTIME` abort the whole binary. Disabling isolation does not weaken
UB checking. The crate is `#![forbid(unsafe_code)]`, but Miri still checks the
unsafe code it runs in std and in dependencies.

The exclusions are listed with reasons in `scripts/miri-skips.txt`:

- `udp::tests::std_udp_socket*` (5 tests). Miri's socket shim only supports
  `SOCK_STREAM`, so binding a `std::net::UdpSocket` aborts.
- Whole targets: the `src/bin/rcp.rs` tests, doctests, benches, and fuzz
  targets. Miri cannot spawn processes, and these targets are covered by other
  jobs.
- The l2 veth round-trip test is already `#[ignore]`d.

Measured locally on the host (`aarch64-apple-darwin`), before the three
refinement tests were added: 1163 passed, 0 failed, 5 filtered out, ~204 s.
Cross-interpreted as `x86_64-unknown-linux-gnu` (`MIRI_TARGET=...`), which is
what CI runs, with the refinement tests: 1165 passed, 1 ignored (the veth
test), 5 filtered out, ~121 s.

## Branch coverage ratchet

c-RCP gates real MC/DC with a regression floor. rustc removed MC/DC
instrumentation; `-Zcoverage-options` now accepts only
`block|branch|condition`. So rust-RCP gates llvm-cov **branch** coverage
instead:

```bash
cargo +nightly-2026-10-21 llvm-cov --branch --all-targets --locked \
  --json --summary-only --output-path branch-summary.json
python3 scripts/branch_coverage_gate.py branch-summary.json
```

The floor is set in `.branch-coverage-ratchet.json`: 90.0% against a measured
92.77% (539/581, macOS, nightly-2026-10-21, including this change's refinement
tests), with the reasoning recorded in that file. The gate fails in two cases:

- coverage drops below the floor;
- the summary carries no branch data. This stops the gate passing vacuously if
  `--branch` is dropped.

When measured coverage runs more than 5 points above the floor, the gate prints
a notice to raise the floor. llvm-cov's Rust branch counters instrument
`if`/`while`/`&&`/`||` conditions only, not `match` arms. That is why there are
only 581 branch points against 17k lines. The existing 90% line gate in the
`coverage` job stays in place.
