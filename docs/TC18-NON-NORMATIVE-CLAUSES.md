# TC18 SHOULD/MAY clauses with no corresponding requirement

Companion to `.fusa-reqs.json`'s MUST/SHALL requirement catalog. Mirrors
c-RCP's and cpp-RCP's own identical audits (see those repos'
`docs/TC18-NON-NORMATIVE-CLAUSES.md` and ROADMAP milestones). Every
SHOULD/MAY occurrence in TC18 is accounted for here or via an existing
`REQ-*` entry's new `tc18` citation field (introduced by this pass — this
repo's schema previously had no citation field at all).

No spec text is reproduced verbatim; each line is paraphrased and cited
by section/`pdftotext`-extraction line reference.

**Depth note:** like cpp-RCP's pass, this mirrors c-RCP's methodology at
a lighter verification depth given this repo's near-zero prior citation
density (0/608 requirements had a `tc18` citation before this pass).
Four MAY clauses below are flagged ⚠ as genuinely uncertain or
gap-suggesting rather than cited — they need their own follow-up
investigation. A fuller MUST-clause citation backfill remains separate,
larger, future work.

## SHOULD (10, all non-testable — identical disposition to every x-RCP repo, this is about the standard's own text)

Six §2 design-goal statements (L773/790/795/798/803/805), two
client-request-composition advice pieces for repetitive compound/
compound-wait requests (L1213/L1312), one client-config-authoring
byte_bus_id/endpoint-type-consistency advice (L2988), one ISELED
hardware-calibration guideline (L5525, moot here — this repo does not
implement an ISELED endpoint). None are library-testable; see c-RCP's
own document for the identical per-line reasoning.

## MAY — implemented and now cited (7)

`REQ-PWR-001` (power modes, L2268), `REQ-LIFE-001` (three lifecycle
states, L2063), `REQ-SPI-005` (SPI 6 channels, L4192), `REQ-ADC-007`
(samples-per-interval R/W choice, L5040), `REQ-TRIG-001` (Triggered
requests exist as an optional feature, L1413), `REQ-SEQ-001` (fewer
sequencers permitted, L3473), `REQ-SAFEMEAS-001` (Safety_Measure-driven
safe state, L2935).

## MAY — genuinely uncertain / not yet distinctly resolved (4, ⚠ needs follow-up)

| § | Citation | Paraphrase | Finding |
|---|---|---|---|
| §11.2.2.5 | TC18.txt L1649 | RC Server may reject a request whose `presentation_time` is too far in the future | ⚠ **Ambiguous, not cited**: `REQ-TIME-002`/`REQ-TIME-003` (`TimedExecutionTime`/`is_timed_request_ready`) implement execution-*readiness* (has current time passed the scheduled time) via `AvtpTimestamp`, a **32-bit**, ~4.3-second-rollover type — but this is a different concern from admission control (reject an overly-*future* request), and TC18's real presentation_time for a Timed request under ACF_GBB is a **48-bit** value rolling over every 3.25 days (§11.2.2.5 Figure 12), not the 32-bit TSCF `avtp_timestamp` domain. Whether `AvtpTimestamp` is actually the right type for this, or conflates two distinct spec-defined time domains (the same bug class found and fixed this session in go-RCP's request/envelope.go), was not confirmed. Needs its own dedicated investigation before either citing or fixing. |
| §12.9.1.1 | TC18.txt L3220 | An RCP frame may include multiple ACF-types (requests) | ⚠ **No single clean citation found**: multi-frame/split-frame logic exists in `mock.rs`/`udp.rs` but no distinctly-titled requirement was found covering the general "parse and dispatch N ACF messages from one AVTPDU" capability the way c-RCP's `REQ-MOCK-019` or cpp-RCP's `REQ-L2-007` do. May already be covered under a differently-worded existing requirement — needs a closer read, not assumed absent. |
| §13.2 | TC18.txt L3502 | An endpoint may be used or not used in a specific RC Server instantiation (EP_USED bit) | ⚠ **Not found**: no `ep_used`/`EpUsed` concept found in any searched file. Same finding as cpp-RCP; c-RCP is so far the only repo confirmed to model this bit. |
| §13.7.13.1 | TC18.txt L5631 | An RC Server with an integrated PHY may allow access to it via the MDIO EP | ⚠ **Not addressed**: `src/mdio.rs`'s functional-config model doesn't discuss this deployment mode the way c-RCP's `ep_mdio.h` does. Likely fine by the same reasoning (register-map access doesn't inherently require validating a physical pin mapping) but not independently confirmed. |

## MAY — descriptive/out-of-scope (27, identical disposition to c-RCP's/cpp-RCP's own audits)

L640 (Edge Node PTP/MACsec — L1/L2 topology, out of RCP-wire scope),
L909, L1024, L1025, L1588, L1943 (gPTP sync generally — implemented via
this repo's own timestamp/clock handling, no single citable requirement),
L2060, L2062, L2244, L2289, L2355, L2385, L2405, L2565, L2668, L2984,
L2986, L2989, L3197, L3206, L3227, L3252, L4323, L5035, L5164, L5358 —
each is descriptive prose restating an architectural fact from a
different angle, a client-side/hardware-deployment choice outside
library scope, or a non-closed-list permission. See c-RCP's identical
document for the per-line paraphrase and reasoning — the spec text and
its non-normative character don't change per implementation.
