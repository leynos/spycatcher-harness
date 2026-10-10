# RFC 0001: Drive ACP agents with cassette-controlled inference

## Preamble

- **RFC number:** 0001
- **Status:** Proposed
- **Created:** 2026-09-07
- **Audience:** Harness maintainers and agent integration-test authors.
- **Authority:** This proposal extends the
  [harness design](../spycatcher-harness-design.md); existing replay APIs and
  cassette formats remain authoritative until an implementation is accepted.

## Summary

Add an optional Agent Client Protocol (ACP) scenario runner to Spycatcher.
The runner puppeteers the agent's client-facing act/expect loop while the
existing HTTP replay service supplies its model-facing inference. A scenario
can therefore require a particular advertised tool, emit its call, observe
permission and execution, and withhold the next model response until the
agent returns the correct tool result. This is the “marionette talking to a
glove puppet” arrangement: client actions and model responses are controlled,
while the installed agent between them remains real.

The proposal adds a versioned scenario sidecar and an owned-process ACP client;
it does not turn HTTP cassettes into a universal event bus. VidaiMock remains
a complementary provider-physics backend for timing and chaos profiles.

## Problem

A provider cassette can show that an agent sent a particular HTTP request.
It cannot, by itself, prove which ACP client capability was negotiated,
whether a permission request was denied, whether a terminal command actually
ran, or whether a loaded session retained the intended tool surface. An ACP
smoke test that merely initializes the agent has the inverse blind spot: it
can pass while tool arguments or results disappear inside the agent loop.

The motivating VTCode recovery needs both views, an independent effect oracle
and installed-binary provenance.[^1] A list/load skill success, for example,
does not establish that a restricted mode advertises the same tool, and
apparently correct read-window metadata does not establish correct bytes.

## Current state and compatibility boundary

The [replay matching guide](../replay-matching-guide.md) defines sequential
strict and keyed matching. Strict mismatches do not consume interactions;
HTTP replay maps mismatches to 409 diagnostics. The
[roadmap](../roadmap.md) still schedules Responses routing, native timing and
optional VidaiMock integration. This proposal depends first on existing Chat
Completions replay and adds neither a requirement to finish those items nor a
claim that they already exist.

The current design excludes external-tool mocking. Acceptance of this RFC
would narrow that exclusion: an opt-in ACP runner may supply declared client
filesystem, terminal and permission fixtures. General service mocks, including
Model Context Protocol (MCP) fixtures, remain separate processes supplied by a
scenario. The provider HTTP server continues to know nothing about ACP.

## Goals and non-goals

The runner must reproduce an agent interaction without credentials, assert
causal ordering across both boundaries, detect denied operations with zero
successful effects, and retain sanitized recordings with a decisive first
failure. It must preserve legitimate differences between frontend capability
profiles instead of demanding that every frontend expose every tool.

The runner does not evaluate model intelligence, promise identical agent
behaviour across versions, replace an agent's internal runtime event contract,
or establish isolation merely by providing fake client methods. It does not
record a personal home directory as a fixture or contact a live provider in
replay mode. Automatic cassette re-recording after a mismatch is excluded.

## Proposed design

### Components and ownership

An optional `acp` library surface owns scenario parsing, lifecycle state,
ACP transport, client fixtures and the run report. CLI integration delegates
to that library; a proposed `run-acp --scenario <path> --agent <absolute-path>`
command is an interface for review, not an existing command. The runner starts
one replay harness per scenario and launches the selected agent with its
provider URL directed to that loopback listener.

The HTTP service keeps its present cassette contract. The scenario references
an immutable provider cassette by digest, declares ACP actions and predicates,
and correlates observations after protocol-specific validation. No generic
cross-protocol message type becomes a new agent runtime contract.

| Owner | Input | Observable output |
| --- | --- | --- |
| Scenario runner | Versioned sidecar, binary and fixture roots | Step verdicts and owned-process cleanup |
| ACP client adapter | JSON-RPC requests and notifications | Session, tool and client-capability observations |
| Client fixtures | Declared filesystem, terminal and permission operations | Deterministic replies and independent effect ledger |
| Provider replay | Existing HTTP cassette and matching policy | Inference responses and match outcomes |
| Evidence writer | Validated observations and redaction policy | Sanitized protocol streams and provenance manifest |

_Table 1: Ownership boundaries in the proposed extension._

### Scenario sidecar and act/expect semantics

Version 1 of the sidecar has required fields for its format version, protocol
version, agent launch configuration, capability profile, fixture and cassette
digests, bounds, steps and final invariants. Unknown versions and unknown
fields fail before launch. The serialized syntax and Rust type names remain
an implementation decision; the following contract is normative for review.

An **act** sends one declared ACP request or notification, or releases one
predeclared client-fixture response. An **expect** observes a specific channel,
method or event kind, correlates identifiers and applies a typed predicate.
An expectation never issues a tool directly inside the agent. A step has a
stable fixture-local name, deadline and exact required occurrence count.

Predicates support exact values, required field presence, bounded lengths and
explicitly declared identifier capture. JSON object key order is immaterial;
array order, tool argument values and tool result contents are material.
Captured identifiers may be reused only through named bindings; duplicate or
contradictory bindings fail. Arbitrary scripts and shell interpolation are
excluded from the initial format.

Expected observations register before the triggering act, so a fast agent
cannot outrun the matcher. Concurrent protocol observations use a declared
partial order: a filesystem response precedes the inference request that
contains its result, but unrelated progress updates need not have a fabricated
total order. Unrecognized messages fail by default. A profile can explicitly
allow bounded informational notifications; required effects and provider
requests cannot be ignored by such a rule.

Each matched provider request consumes one cassette interaction only after
all step predicates governing its response release succeed. Existing strict
hash matching remains necessary. The adapter supplies a response-release
barrier around matching rather than teaching canonicalization to recognize
ACP state. Unmet predicates terminate the scenario, preserving the first
failure; retries cannot turn that run green.

Final success requires every mandatory step and cassette interaction consumed,
no unmatched observations, all effect-count assertions satisfied, the declared
prompt stop reason and successful cleanup. A final assistant sentence alone
never satisfies success.

### Concrete read-window scenario

A synthetic workspace file contains `alpha`, `bravo`, `charlie`, `delta`,
`echo` and `foxtrot`, each followed by a newline. The fixture pins the exact
advertised file-read tool schema of the chosen VTCode build and requests the
third and fourth lines. It must not assume that `line`, `limit` or internal
`offset` aliases have equivalent semantics.

The scenario first initializes ACP and creates a session. Its first provider
request must advertise the selected tool and window parameters. The first
cassette response emits exactly one call with a fixed tool-call identifier.
An allowed client fixture records the expected read arguments. Its contract
may permit one bounded lookahead line for continuation metadata; that extra
line must not appear in the model-visible result. The next provider request
must contain exactly `charlie\ndelta\n` and the corresponding call identifier,
with truthful bounded-window metadata if the
tool supplies it. Only then does the second cassette response supply the
final reply. Completion requires exactly one successful read.

Separate negative scenarios remove advertisement, mutate an argument, deny
admission, return wrong bytes with plausible metadata, and issue an extra
inference request. The denial case uses a dedicated cassette expecting a
denial result, not the golden success result. A profile that never requests
permission cannot claim client-permission denial coverage; select a synthetic
policy-controlled target or report that coverage unavailable.

Large-file scenarios extend the same oracle: assert payload byte bounds,
actual start/end positions, truncation truth and continuation semantics.
Returning a truncation flag while retaining an unbounded payload is a failure.
The oracle derives contents from the fixture, independently of agent metadata.

### ACP lifecycle and capabilities

The adapter implements initialization before session creation and gates
optional operations on negotiated support. Loading is attempted only when
`loadSession` is advertised.[^2] Prompt turns can produce updates and client
requests before the original prompt request completes; the reader remains
active while a request is outstanding.[^3]

The initial lifecycle vocabulary covers `initialize`, `session/new`,
`session/load`, `session/prompt`, `session/cancel` and `session/set_mode`.
Each capability profile pins protocol/schema revision and supported methods;
mode changes must select an offered mode. Cancellation is a notification, not
a request for an invented response. The runner continues draining output and
expects the pending prompt to settle within its cancellation deadline.[^3]

Use a session-local state machine: uninitialized, initialized, idle, prompting,
cancelling and terminated. Loading and mode changes are explicit transitions
whose legal timing follows the pinned protocol profile. Another session may
progress concurrently only when the scenario declares it. All identifiers,
expectations, fixture state and failure propagation are session-scoped.

Child-session scenarios are agent-specific extensions unless the pinned ACP
schema defines them. They must declare their namespace and parent correlation
rather than inventing a portable ACP child method. A sibling failure must not
consume another session's provider interaction or leak its permissions.

### Client fixtures and independent effects

A capability profile explicitly advertises supported filesystem and terminal
methods and declares permission choices. The runner never advertises a
capability that its fixture cannot implement. A required capability absent
from the agent fails preflight; an explicitly optional scenario reports
`unsupported`, which is distinct from `passed`.

Filesystem fixtures resolve paths within synthetic roots and reject traversal
and symlink escape. Read and write methods have independent attempt and
successful-effect counters. Read-window values, including any explicitly
permitted bounded lookahead, are checked before a reply is released. Terminal
fixtures either model the full declared lifecycle with
scripted output/exit status or launch a fixture-owned process under explicit
containment; the report distinguishes simulated execution from a real command.
No unexpected command falls back to host execution.

Permission fixtures choose only among offered options, record the request and
reply, and bind the decision to the operation under test. Denial requires zero
successful effects. A missing callback is not evidence of denial. ACP fakes do
not observe arbitrary agent-local filesystem or subprocess activity: a local
execution profile requires sandbox telemetry or fixture-owned effect markers,
and states the limits of that observation.

MCP processes use declared executables and synthetic inputs. Scenarios for
canonical aliases and combined allowlists assert both advertisement and the
actual invocation ledger. Skills scenarios use synthetic manifests, with
separate valid, malformed and restricted-mode profiles. They never require
raw home-file access to establish skill resolver correctness.

### Cassette matching and inference determinism

Use sequential strict matching initially. The first request establishes the
tool schema and the later request establishes result feedback. Ignore paths
may cover documented run metadata only; tool declarations, argument fields,
call identifiers and asserted message contents cannot be globally ignored.
If an identifier is nondeterministic, an explicit bijective binding must
preserve its references and detect collision or reassignment.

The response-release barrier makes the cassette a contingent mock, not an
unconditional transcript player. A missing tool result leaves the next
request unmatched; it cannot receive a success answer. Unexpected requests,
exhaustion and unused required interactions are run failures even if the
agent catches the HTTP error and exits normally. Capture a structured bounded
diff and the first violated expectation.

### Timing, cancellation and VidaiMock

Every scenario declares frame-size, output-size, step-time, whole-run and
shutdown bounds. Drain ACP stdout and stderr independently; protocol parsing
must not block progress output handling. Timeout cleanup sends cancellation
where applicable, waits a bounded interval, terminates and reaps only children
owned by this run, and records partial evidence.

The deterministic baseline checks content and causality without asserting
wall-clock latency. A physics profile adds time-to-first-token, paced chunks,
jitter or chaos through the native timing work or an explicitly supported
VidaiMock adapter. Pin the backend version, seed, fixture digest and controls.
Use monotonic time and tolerance envelopes; virtual time can prove scheduler
logic but cannot establish real transport pacing.

A scenario that requests VidaiMock physics must fail or report unsupported
when that backend is unavailable. It must not silently substitute immediate
native replay and claim physics coverage. This is stricter than the optional
fallback contemplated for general replay in roadmap task 3.3.1: the requested
scenario property decides whether fallback is valid. Export relies on an
authoritative versioned VidaiMock fixture schema; this RFC invents none.

### Recordings, redaction and reproducibility

A run directory contains a versioned manifest, ACP JSON-RPC stream, provider
cassette, independent effect ledger, bounded stderr and a verdict. The
manifest records measured binary SHA-256, declared source revision or unknown,
harness revision, scenario/fixture/cassette digests, negotiated capabilities,
protocol schema revision, backend settings, sanitized configuration digest,
exit status and first failing step. Source provenance and binary identity
remain separate facts.

Normalize fixture root paths to stable tokens while preserving path
relationships. Redact credentials and unapproved message-body fields before
any persistence, including mismatch diffs and stderr. Synthetic fixtures
supply all test content. Header-only redaction is insufficient for an ACP
recording. Canary tests must prove that secrets planted in each channel do
not reach stored or console diagnostics. Avoid redaction rules that erase
asserted tool arguments; reject an unsafe fixture instead.

Replay binds loopback endpoints and runs under a declared network/filesystem
containment profile. Process environment is constructed from an allowlist;
provider credentials and personal configuration are absent. Reproducibility
means the same pinned fixture and executable reproduce the verdict under the
recorded profile; it does not assert reproducible compilation of that binary.

## Verification and rollout

The implementation must make four invariants falsifiable: denial produces
zero successful effects; no response releases before its prerequisites; each
mandatory interaction is consumed exactly once; one session cannot consume
another's bindings or effects. Unit and behavioural checks accompany delivery.
Property-based state-machine checks permute legal progress notifications,
request/reply interleavings and cancellation points; malformed transitions
must fail without advancing unrelated state.

Start with one installed VTCode read-window golden scenario and its mutation
controls against Chat Completions. Next cover new/load/mode transitions,
permission/terminal/filesystem profiles and synthetic skills/MCP routing.
Use pairwise coverage for supported profile combinations, plus explicit
high-risk combinations: restricted mode with skill filtering, alias routing
with both allowlists, cancelled tool calls, sibling-session failure, and
truncated reads with continuation. Unsupported combinations remain visible
in the report rather than disappearing from the denominator.

Responses and real paced-stream/chaos scenarios follow protocol and backend
support. A content-only pass must remain distinct from a physics pass. CI
retains sanitized successful and failing recordings, proves each negative
control can fail the golden contract, and runs the published fixture without
live provider access. Cross-version agent drift requires reviewed fixture
updates with before/after diffs; no automatic golden blessing is allowed.

## Alternatives considered

A standalone ACP script is suitable for first reconnaissance but duplicates
lifecycle, redaction and evidence rules as scenarios grow. Provider-only replay
remains useful for HTTP contract tests but lacks client-side effects.
Replacing Spycatcher with VidaiMock would lose the existing cassette-matching
boundary; replacing VidaiMock with a new physics engine would duplicate its
role. The proposed runner composes the two responsibilities.

A universal event bus would require every protocol and agent to adopt another
runtime contract. Correlation in a test-run report supplies the required
ordering evidence with a smaller compatibility surface.

## Open questions and recommendation

The implementation review must select the pinned ACP schema package, scenario
serialization syntax and minimum containment mechanism for the first supported
platform. It must also choose the narrow response-release observation hook
without exporting private matching internals. These decisions cannot weaken
the invariants or be hidden behind permissive defaults.

Accept the optional ACP runner boundary and versioned sidecar contract, then
implement the narrow installed-agent golden/negative slice before widening
protocols or capability combinations. The separately proposed recovery ADR
and roadmap can proceed independently; this RFC neither marks them complete
nor makes its full interface a prerequisite for the first fixture.

## References

[^1]: VTCode recovery incident context (not a harness coding session):
    <https://lody.ai/leynos/sessions/16f610b6-3473-45d9-9611-fb19d007f54e>.
[^2]: [ACP session setup](https://agentclientprotocol.com/protocol/v1/session-setup).
[^3]: [ACP prompt turn](https://agentclientprotocol.com/protocol/v1/prompt-turn).
