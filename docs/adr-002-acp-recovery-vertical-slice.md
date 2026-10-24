# Architectural decision record (ADR) 002: Prove installed ACP tool recovery with strict replay

## Status

Proposed. This record selects the first implementation slice; it does not
claim that Agent Client Protocol (ACP) orchestration exists in the harness.

## Date

2026-09-07

## Context and problem statement

VTCode recovery needs evidence that an installed agent can advertise a tool,
receive an explicit model tool call, admit the operation, execute it and send
its truthful result into the next model turn. A successful initialization or
a tool name in a schema proves only part of that chain. The motivating
incident includes dropped file-window arguments and permission configuration
that reached one boundary without reaching the execution policy.[^1]

Spycatcher already replays Chat Completions through a local HTTP server and
matches requests sequentially. A mismatch returns HTTP 409 without consuming
the expected interaction. Responses support, replay timing controls and an
ACP client remain future work in the [roadmap](roadmap.md). The first useful
slice should exploit the existing replay boundary rather than make those
extensions prerequisites.[^2]

## Decision drivers

- Exercise the installed executable and its real ACP adapter, configuration,
  tool registry and tool-result serialization.
- Make false success impossible when arguments disappear or tools are denied.
- Run without provider credentials or paid inference.
- Retain enough sanitized evidence to reproduce a failure against the same
  executable and fixture without publishing personal home-directory content.

## Proposed direction

Deliver a narrow ACP fixture runner around the existing strict Chat
Completions replay server. Select VTCode's workspace file-read operation as
its first golden interaction. Negotiate ACP, create one session and submit one
prompt; initialization must precede session creation.[^3] Pin a synthetic
workspace, synthetic home, capability profile, configuration and cassette.
Launch the explicitly selected installed binary by absolute path.

The fixture contains six distinct ASCII lines. The requested window is lines
3 and 4, using the actual advertised tool name and argument schema of the
pinned VTCode build. The authoring step must inspect that schema; `line`,
`limit`, `offset` and `start_line` are not interchangeable aliases by decree.
Store the selected spelling and meaning in the fixture contract.

The golden scenario requires these observations in order:

1. The first inference request advertises the selected tool with the required
   window arguments. The complete canonical request matches the cassette;
   broad removal of the tool schema or message history is prohibited.
2. The cassette emits one explicit tool call with a fixed call identifier and
   the selected two-line window. No natural-language request to use a tool is
   treated as proof of execution.
3. The allowed profile admits the operation. If that profile requires a client
   permission request, the runner records the request and supplies its declared
   response before the fake filesystem operation can succeed.
4. The client filesystem fixture records exactly one read with the expected
   path and declared bounded window. The fixture may permit one lookahead
   line to compute continuation metadata; that bound is explicit in the
   ledger contract. The model-visible result still contains exactly the
   requested two lines. Hidden or unbounded overreads fail. The first profile
   advertises filesystem reads and requires the selected agent route to use
   that capability; an agent-local read profile is a separate follow-up.
5. The second inference request includes the matching tool-call identifier,
   exactly the requested file contents and truthful window metadata, if the
   tool exposes metadata. The strict cassette supplies the final reply only
   after this request matches.
6. The runner observes prompt completion, consumes every required inference
   interaction and reports no unexpected client operation or residual child.

The synthetic file and the fake's independent operation ledger are the
oracles. Neither the agent's final prose nor its own success flag substitutes
for them. The outer ACP transcript and inner inference transcript remain
separate protocol records, correlated in a run manifest.

## Negative controls and acceptance

The following controls make the golden path an effective regression detector.
They run from a fresh cassette cursor and fresh fixture state.

| Control | Required observation | Failure detected |
| --- | --- | --- |
| Drop or change a window argument | Client ledger or next inference request differs from the golden contract; no golden final reply is served | Argument loss hidden by success metadata |
| Remove the tool from advertisement | First inference request mismatches before any tool response is released | Listing or mode filtering regression |
| Deny the operation | No filesystem callback succeeds; the next inference request contains a denial result and matches a dedicated denial cassette | Admission bypass or fabricated success |
| Return incorrect bytes with plausible metadata | Next inference request mismatches and the run fails | Metadata-only verification |
| Add an extra inference request | Cassette exhaustion fails the run | Retry or extra-turn drift |

_Table 1: Required negative controls for the first recovery slice._

The denial profile must actually exercise a permission-controlled read route.
If the pinned agent admits workspace reads without prompting, the fixture
must select a synthetic permission-controlled target and policy; silently
counting an absent permission callback as denial coverage is prohibited.
An unexpected direct host read is a profile violation. The fake alone cannot
prove that an arbitrary agent process made no host-system calls: containment
must restrict the process to the synthetic roots and loopback endpoint, and
the report states the containment mechanism used.

Unit and behavioural coverage accompany the runner. Property-based checks
vary valid windows, empty files and boundaries: returned bytes must equal the
fixture slice, and a denied transition must leave the successful-operation
count at zero. Mutating an argument or result must make the golden run fail.
These are fixture and admission invariants, not assertions about arbitrary
model reasoning.

## Evidence contract

Each run retains a versioned manifest, sanitized ACP JSON-RPC transcript,
inference request/response cassette, operation ledger and verdict. Record the
source revision supplied by the build, installed binary SHA-256, harness
revision, cassette and fixture digests, protocol version, capability profile,
configuration digest, exit status and first failing step. A source revision
is a provenance claim distinct from the measured binary digest; unknown build
provenance is reported as unknown.

Capture stdout as protocol data and stderr separately. Bound each phase and
the whole run, drain pipes concurrently, and kill and reap only owned children
on timeout. Unexpected requests, unused required interactions, malformed
frames, process exit and cleanup failure produce non-zero results.

Use fixture-owned content only. Credentials, raw home paths, environment
values and unrelated workspace content must not enter published recordings.
Apply structured redaction before persistence, including mismatch diagnostics;
header redaction alone does not protect message bodies. Stable path tokens
must preserve relationships without deleting asserted arguments or results.
A published fixture includes the sanitization policy and canary checks.

## Options considered

A provider-only replay test is smaller but misses ACP routing and permission
policy. A script that calls internal tool handlers misses installation and
adapter configuration. A broad ACP runner with Responses, every capability
profile and timed chaos would delay the first useful result. The selected
slice crosses the failing boundaries using the provider protocol already
supported by this repository.

VidaiMock remains the complementary backend for timed chunks and chaos.
Native sequential replay establishes content and ordering first; it does not
claim realistic timing. A later physics profile must report unavailable
VidaiMock support explicitly rather than pass by falling back to untimed
replay.

## Consequences and migration

[Roadmap phase 5](roadmap.md#5-installed-agent-recovery-through-acp) adds this
capability without rewriting existing record/replay contracts. The runner is
opt-in and owns ACP/client fixtures; the existing HTTP cassette format remains
a provider-boundary record. Skills, Model Context Protocol (MCP) alias and
allowlist checks, load/mode/child-session isolation, bounded continuation
reads and Responses physics remain subsequent scenarios, not implicit claims
of the first slice.

The wider ACP puppeteering RFC is independently reviewable. This ADR neither
requires its full implementation nor accepts its proposed public interface.
The first implementation must settle the fixture's exact VTCode tool schema
and containment profile before accepting a golden recording.

## References

[^1]: VTCode recovery incident context (not a harness coding session):
    <https://lody.ai/leynos/sessions/16f610b6-3473-45d9-9611-fb19d007f54e>.
[^2]: [Replay matching guide](replay-matching-guide.md) and
    [current replay architecture](spycatcher-harness-design.md#architecture-overview).
[^3]: [ACP session setup](https://agentclientprotocol.com/protocol/v1/session-setup).
