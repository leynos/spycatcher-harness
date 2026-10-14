# Debugging plan: executable test fixture reports text file busy

- **Generated**: 2026-10-14
- **Issue**: spycatcher-harness pull request #175
- **Severity**: Hosted test gate failure
- **Status**: Local models falsified; hosted cause inconclusive
- **Falsification role**: `alchemist`

**Planning agent boundary**: The planning agent prepared this document only.
The parent selects one hypothesis per alchemist assignment. No experiment,
build, gate, source change, commit, or publication has been performed by the
planning agent.

## Problem statement

The hosted coverage test expected a stand-in executable to exit with status 4
and preserve separate standard output and error payloads. Its launch instead
failed with Linux `ETXTBSY` (text file busy, operating-system error 26). The
contract binary reported 68 passing tests and one failure. The inquiry asks
which additional writer could survive the fixture's own file closure; it does
not assume that the file remained open in its creating function.

## Context summary

Table 1. Evidence and inquiry boundary.

| Aspect             | Details                                                                                                    |
| ------------------ | ---------------------------------------------------------------------------------------------------------- |
| Baseline           | `093f55dc64dc26c44f68d1829c13f462573998ff`, tree `19ce1feaf8245a67dcc908590abc9fc0a443e812`                |
| Checkout           | `deliver-test-quality-20261014`; clean at intake                                                           |
| Hosted observation | Run `38504143579`, attempt 1, coverage step, 2026-10-14 03:14:07 UTC                                       |
| Fixture            | `tests/build_standard/reader_tests.rs:289-355`                                                             |
| Launch             | `tests/build_standard/support.rs:116-151`, `dry_run_with`                                                  |
| Reproduction rate  | One supplied hosted failure; recurrence rate unknown                                                       |
| Scope              | Two deterministic Linux/Python mechanism experiments; one execution per selected hypothesis                |
| Exclusions         | Rust, Cargo, full gates, services, credentials, unrelated processes, stress loops, CI reruns, source fixes |

The reviewed `fake_program` removes and recreates
`CARGO_TARGET_TMPDIR/failing-make`, opens `make` for exclusive creation and
writing with mode `0755`, writes the script, and returns its pathname. Its local
`File` is dropped on return. `dry_run_with` subsequently calls
`Command::output`. It reports launch failures separately from non-zero exits.

The supplied Wyvern triage found only one fixture caller and no confirmed
collision inside that test binary. The nearby failed-Make test uses real
`make`. These are triage claims, not evidence that no second invocation or
concurrent child existed in the hosted process tree.

### Error artefacts

The canonical log is `/tmp/spycatcher-pr175-rehead-monitor-20261014-oLsTjb/`
`run-38504143579-attempt-1/failed.log`, lines 1051-1076.

```plaintext
`exit status: 4` missing from: cannot run
`.../target/llvm-cov-target/tmp/failing-make/make -n lint` on Linux:
Text file busy (os error 26)
test result: FAILED. 68 passed; 1 failed; 0 ignored
```

Source SHA-256 values at intake:

- `reader_tests.rs`:
  `ac5ef78ffe3ec5fc774425a6b7c03c7832d65543a1e37f8a13d62c469602562d`
- `support.rs`:
  `33a6a87a62bfd8d800b9dba4f9db5e7d4943c65035102669172f10edde90fe95`

### Information gaps and alternatives

There is no supplied trace of writer descriptors, fork/exec system calls, inode
identities, or simultaneous test invocations from the failed runner. The
fixture's actual descriptor flags and the launch implementation used by each
concurrent command have not been measured. The experiments model Linux
behaviour with explicit `fork`; they do not establish that Rust used that path,
rather than a different spawning mechanism, in this run.

A third explanation, such as another unrecognized writer or filesystem
behaviour, remains possible. The hypotheses need not be mutually exclusive in
the hosted run. Each experiment excludes the other mechanism locally.

## Hypotheses

### H1: another invocation replaces the shared fixture path

**Claim**: A second invocation removes and recreates the fixed fixture path
after the first invocation closes its script. The first invocation resolves the
new inode while the second invocation still holds its writable file.

**Plausibility**: Low to medium. The path and replacement operations are
observed, but a second invocation sharing that path is unconfirmed.

**Assumption**: A launch of the replaced pathname encounters the replacement
writer, independent of whether the first writer has already closed.

**Prediction**: The closed original executes normally. After synchronized
replacement, inode identity changes and launch returns `ETXTBSY`. A separate
closed fixture remains executable. Closing the replacement writer restores exit
4 and both payloads without changing the replacement inode.

#### H1 falsification plan

Run the script below once with `h1`. The child is forked after the original
writer closes, so it cannot inherit that writer. It explicitly unlinks and
recreates the pathname, then holds the new writer until a pipe releases it. A
read-only descriptor keeps the old inode allocated for the identity check.

With successful controls and verified replacement writer/inode, any launch
outcome other than `ETXTBSY`, or continued `ETXTBSY` after the sole writer
closes, falsifies this isolated mechanism model. Failed controls or handshake
failures are inconclusive, not evidence against the hosted claim.

**Confidence limit**: A negative result applies to this host and filesystem. A
positive result establishes feasibility only. Hosted attribution requires
evidence of two overlapping invocations and pathname replacement before the
failed launch. A complete trace showing no replacement in that interval would
falsify H1 for the traced failure. No such trace is currently supplied.

### H2: a forked child temporarily retains the closed writer

**Claim**: While the fixture's writer is open, another command forks from its
process. That child inherits the writer and delays closing or executing. The
fixture function returns and closes the parent's descriptor, but the child's
reference still prevents execution of the same script inode.

**Plausibility**: Medium. A multi-test binary can launch commands concurrently;
the required fork timing, descriptor ownership, and spawn path are unobserved.

**Assumption**: `FD_CLOEXEC` closes a descriptor at successful exec, rather
than at fork; a waiting forked child can retain a writable reference.

**Prediction**: With a unique pathname and unchanged inode, a child holding the
inherited close-on-exec writer causes `ETXTBSY` after the parent closes its
copy. Explicitly closing the child's writer restores the expected exit and
payloads while that child remains alive. A closed-writer control passes.

#### H2 falsification plan

Run the script below once with `h2`. Reopen the existing script for writing
without rewriting it, set and verify close-on-exec, then fork. Close the
parent's writer before launch. The child acknowledges both its descriptor flag
and inode, waits, closes the inherited writer, acknowledges closure, and
remains alive for the released control.

With successful controls and verified inherited descriptor, a successful launch
during the hold, or continued `ETXTBSY` after child closure, falsifies the
isolated mechanism model. Missing close-on-exec, changed inode, failed
controls, or handshake failures make the result inconclusive.

**Confidence limit**: A positive result establishes that parent closure and
close-on-exec alone are insufficient in this explicitly forked model. It does
not prove descriptor inheritance in `cap_std`, Rust's spawning path, or the
hosted runner. Attribution requires a child carrying a writable reference to
the failed inode through the parent's close and failed exec. A complete
descriptor/spawn trace excluding that overlap would falsify H2 for the traced
failure. No such trace is currently supplied.

## Exact experiment script

The executing alchemist saves this block to a scratch Python file and runs
`python3 FILE h1` or `python3 FILE h2`, as assigned by the parent. The script
does not run both experiments. It uses only Python's standard library and
`/bin/sh`. Temporary files are scratch, not build output. Capture standard
output, standard error, exit status, and the supplied hypothesis identifier.

Pipe waits have three-second deadlines and each executable probe has a
two-second deadline. The intended budget is under 30 seconds per invocation. No
loop repeats an experiment. No manual process signals or service commands are
used; a subprocess timeout is confined to that newly created probe. Every actor
is released through its own pipe and reaped with `waitpid`.

```python
import errno
import fcntl
import json
import os
from pathlib import Path
import select
import subprocess
import sys
import tempfile
import time

SCRIPT = b"#!/bin/sh\necho OUT-PAYLOAD\necho ERR-PAYLOAD >&2\nexit 4\n"
EXPECTED = {"exit": 4, "stdout": "OUT-PAYLOAD\n", "stderr": "ERR-PAYLOAD\n"}


def identity(path):
    value = os.stat(path)
    return [value.st_dev, value.st_ino]


def create(path):
    fd = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o755)
    try:
        os.fchmod(fd, 0o755)
        assert os.write(fd, SCRIPT) == len(SCRIPT)
    finally:
        os.close(fd)


def probe(path):
    try:
        result = subprocess.run(
            [str(path), "-n", "-B", "lint"],
            capture_output=True, text=True, timeout=2,
        )
        return {"exit": result.returncode, "stdout": result.stdout, "stderr": result.stderr}
    except OSError as error:
        return {"errno": error.errno}


def read_ready(fd):
    assert select.select([fd], [], [], 3)[0], "handshake timed out"
    message = os.read(fd, 4096)
    assert message, "actor exited before acknowledgement"
    return json.loads(message)


def command(fd, expected):
    assert select.select([fd], [], [], 3)[0], "release timed out"
    assert os.read(fd, 1) == expected, "unexpected release command"


def actor(path, inherited):
    control_r, control_w = os.pipe()
    report_r, report_w = os.pipe()
    pid = os.fork()
    if pid:
        os.close(control_r)
        os.close(report_w)
        return pid, control_w, report_r
    os.close(control_w)
    os.close(report_r)
    writer = inherited
    status = 1
    try:
        if writer is None:
            os.unlink(path)
            writer = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o755)
            os.fchmod(writer, 0o755)
            assert os.write(writer, SCRIPT) == len(SCRIPT)
        flags = fcntl.fcntl(writer, fcntl.F_GETFD)
        ready = {"inode": identity(path), "cloexec": bool(flags & fcntl.FD_CLOEXEC)}
        os.write(report_w, json.dumps(ready).encode() + b"\n")
        command(control_r, b"C")
        os.close(writer)
        writer = None
        os.write(report_w, b'{"closed": true}\n')
        command(control_r, b"Q")
        status = 0
    finally:
        if writer is not None:
            os.close(writer)
        os.close(control_r)
        os.close(report_w)
        os._exit(status)


def run(which):
    started = time.monotonic()
    with tempfile.TemporaryDirectory(prefix="fake-program-busy-") as root:
        path = Path(root) / "make"
        stable = Path(root) / "closed-control"
        create(path)
        create(stable)
        baseline = probe(path)
        assert baseline == EXPECTED, baseline
        original = identity(path)
        old_read = os.open(path, os.O_RDONLY)
        inherited = None
        if which == "h2":
            inherited = os.open(path, os.O_WRONLY)
            fcntl.fcntl(inherited, fcntl.F_SETFD, fcntl.FD_CLOEXEC)
        pid, control, report = actor(path, inherited)
        if inherited is not None:
            os.close(inherited)
        closed = False
        try:
            ready = read_ready(report)
            assert ready["cloexec"], ready
            assert (ready["inode"] == original) == (which == "h2"), ready
            isolated = probe(stable)
            busy = probe(path)
            os.write(control, b"C")
            assert read_ready(report) == {"closed": True}
            closed = True
            released = probe(path)
            assert identity(path) == ready["inode"], "inode changed after writer closure"
            print(json.dumps({
                "hypothesis": which, "original": original, "ready": ready,
                "baseline": baseline, "isolated": isolated,
                "busy": busy, "released": released,
                "elapsed_seconds": time.monotonic() - started,
            }), flush=True)
            assert isolated == EXPECTED and released == EXPECTED, "control failed"
            assert busy == {"errno": errno.ETXTBSY}, "mechanism prediction contradicted"
        finally:
            try:
                os.write(control, b"Q" if closed else b"CQ")
            except BrokenPipeError:
                pass
            os.close(control)
            os.close(report)
            os.close(old_read)
            _, status = os.waitpid(pid, 0)
            assert os.waitstatus_to_exitcode(status) == 0, "actor failed"


assert sys.platform.startswith("linux"), "Linux required"
assert len(sys.argv) == 2 and sys.argv[1] in {"h1", "h2"}
run(sys.argv[1])
```

## Recommended execution order and decision gates

1. **H2 first**: the unique, unchanged inode retires the question of whether
   parent closure and close-on-exec are sufficient. It needs no unconfirmed
   second test invocation and has a direct descriptor-release control.
2. **H1 second**, only when separately assigned: the replacement inode isolates
   the shared-path hazard. Evidence of this mechanism does not exclude H2.

Table 2. Predeclared result interpretation and subsequent decisions.

| Observation                                                                                                  | Verdict                                                  | Parent decision                                                                                                             |
| ------------------------------------------------------------------------------------------------------------ | -------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------- |
| Controls pass; held writer produces `ETXTBSY`; release restores exit 4 and both payloads                     | Mechanism not-falsified; hosted attribution inconclusive | Retain that mechanism as a fixture-design risk; choose bounded attribution evidence or a justified invariant-preserving fix |
| Controls pass and writer/inode invariants hold, but the predicted busy or release transition is contradicted | Mechanism model falsified on this host                   | Do not justify a fix using this mechanism; reassess the runner/filesystem mismatch                                          |
| Timeout, setup/child failure, failed controls, changed inode, or absent close-on-exec                        | Inconclusive                                             | Stop; return the exact artefact without retries or broader investigation                                                    |

## Termination criteria and hand-off

- Stop after the single assigned experiment. No surviving mechanism establishes
  the hosted root cause; do not use the template's stronger root-cause label.
- Report hypothesis, command, script hash, OS/kernel, scratch filesystem type
  if already available, elapsed time, exit status, JSON output, and any error.
  Unknown environment facts remain unknown; do not expand reconnaissance.
- Separate mechanism verdict from hosted-attribution verdict. Return
  `falsified`, `not-falsified`, or `inconclusive`, with the confidence limit.
- Preserve the closed-writer exit, both payloads, no-path-rewrite invariant for
  H2, and changed-inode invariant for H1 as correctness controls.
- If neither model yields decisive evidence, return to the parent. At most one
  separately authorized descriptor/spawn observation of the fixture could
  distinguish the hosted alternatives; no probabilistic stress test or repeated
  continuous-integration run is authorized by this plan.
- The parent owns implementation, full sequential Scrutineer gates, acceptance,
  and publication. Other agents are active; preserve their edits. Only this
  debugging document is owned by the planning agent.

## Planning validation state

The supplied source ranges, hashes, branch, clean intake status, and canonical
log excerpt were read. Before dispatch, the proposed script passed static
Python abstract syntax tree parsing and a static line-length scan. The planning
agent did not execute it. The original predictions and script remain unchanged
after the separately executed results below. No formatter, lint, test, or
Markdown gate was run by the planning agent; the parent must arrange those
before any documentation commit.

## Returned experiment results

Each hypothesis was executed once by its separately assigned execution agent.
The planning agent read both returned logs. The parent supplied the execution
kernel as Linux `6.12.0-211.62.1.el10_2.x86_64`; this was not independently
probed by the planning agent. Filesystem type was not supplied.

Table 3. Observations against the unchanged predictions.

| Hypothesis | Writer and inode controls                                             | Held-writer probe                                     | Other controls                                                                                    | Reported probe elapsed time  | Experiment exit | Verdict                                |
| ---------- | --------------------------------------------------------------------- | ----------------------------------------------------- | ------------------------------------------------------------------------------------------------- | ---------------------------- | --------------- | -------------------------------------- |
| H2         | `FD_CLOEXEC=true`; unchanged identity `[2306, 6829245]`               | Exit 4, `OUT-PAYLOAD` and `ERR-PAYLOAD`; no `ETXTBSY` | Baseline, isolated closed fixture, and released writer all exit 4 with the same distinct payloads | 0.013344504055567086 seconds | 1               | Mechanism model falsified on this host |
| H1         | `FD_CLOEXEC=true`; replacement `[2306, 6954010]` to `[2306, 6954016]` | Exit 4, `OUT-PAYLOAD` and `ERR-PAYLOAD`; no `ETXTBSY` | Baseline, isolated closed fixture, and released writer all exit 4 with the same distinct payloads | 0.013531446980778128 seconds | 1               | Mechanism model falsified on this host |

The JSON observations precede the assertion failure
`mechanism prediction contradicted` in both logs. The experiment's exit 1
records a contradicted prediction with passing controls, rather than a setup,
handshake, or child-reaping failure. The JSON elapsed times measure through the
observations; they are not independent whole-process timings. Both tracebacks
name the same extracted Python script; the JSON hypothesis field identifies the
selected `h2` or `h1` invocation.

Evidence logs:

- H2: `/tmp/fake-program-busy-h2-spycatcher-pr175-20261014.out`.
- H1: `/tmp/fake-program-busy-h1-spycatcher-pr175-20261014.out`.
- Preserved script SHA-256, including the trailing newline:
  `3889bfafaede20fa1de08aa4c896c49cfacd9729395a7b281e585594544ec09f`.

### Consequence for the next decision

The hosted failure's cause remains **inconclusive**. Neither original model
reproduced the predicted script-launch refusal on this host. No fix should be
justified by claiming these experiments positively established either model.
The observations do not establish that writable descriptors were absent from
the failed runner; they contradict the proposed local script behaviour.

The parent commissioned bounded primary-kernel research and returned the
evidence below. The original experiment, hypotheses, and negative results
remain unchanged. No repeated execution or further investigation is authorized
by this document. The parent owns the selected correction and its validation.

## Kernel evidence and transfer limit

The parent's primary-source follow-up identifies Linux commit
`2a010c41285345da60cece35575b4e0af7e7bf44`, titled
`fs: don't block i_writecount during exec`, authored on 31 May 2024 and
committed on 3 June 2024 during the Linux 6.11 development cycle. Its change
removes `deny_write_access` from `do_open_execat`. Its explanation describes
the write-close sequence and a concurrent fork retaining the writer as a cause
of an executable-launch refusal.[^1] The supplied source comparison places the
earlier behaviour in Linux 6.10 and the changed behaviour in Linux
6.11.[^2][^3] These are returned primary-source findings, not a new experiment
by the planning agent.

The local experiments ran on Linux 6.12. Consequently, their negative results
do not rule out the proposed writer mechanism on a runner using the earlier
kernel behaviour. This limits transfer of the local falsification; it does not
reverse the observed verdicts or establish the hosted mechanism.

The captured continuous-integration metadata reports Ubuntu 24.04 and
`ubuntu-latest`, but supplies no kernel release. Neither an older runner kernel
nor an actual writer has been identified. H1 still needs a second invocation
replacing the pathname; H2 still needs a child retaining the writer across the
failed launch. Hosted attribution remains **inconclusive**.

## Parent-selected correction and acceptance invariants

The parent selected a bounded fixture-design correction without claiming a
proven root cause. A source Journeyman owns implementation. Each stand-in will
own a unique temporary directory, and a synchronous child will write its
executable. That child must complete and be reaped before the fixture returns.
The parent process must never open the stand-in script for writing.

The intended benefit is reliable test setup with isolated lifetimes and
preserved execution diagnostics. The design removes shared-path replacement and
parent-process writer inheritance as fixture hazards, independently of which
kernel behaviour the runner uses. This is a prospective design guarantee; it is
not an observation that the current failure had either cause.

The parent-selected acceptance invariants are:

- Two simultaneously live stand-ins using the same caller label own distinct
  paths and cannot replace or delete each other's executable.
- A stand-in's temporary-directory owner remains alive through execution;
  cleanup occurs when that owner is released.
- The parent never owns a writable descriptor for the script. A writing child
  completes and is reaped before the script can be executed by the test.
- Setup failures retain fallible, contextual propagation. Writer-child launch
  or exit failure is checked before a fixture is returned.
- The existing non-zero exit status, separate standard output and error
  payloads, target, and host diagnostic contract remains intact.
- A distinct same-label concurrency regression exercises simultaneous fixture
  ownership and execution. It demonstrates isolation rather than pretending to
  reproduce the historical hosted `ETXTBSY` failure.
- No retry, sleep, global mutex, lint suppression, or weakened assertion is
  introduced to accommodate the failure.

The parent retains source review, sequential Scrutineer gates, acceptance, and
publication. No source implementation, experiment revision, repository gate, or
commit has been performed by the planning agent. This document is frozen for
the parent's documentation gates.

[^1]: [Linux exec write-count change, commit 2a010c4](https://git.kernel.org/pub/scm/linux/kernel/git/torvalds/linux.git/commit/fs/exec.c?id=2a010c41285345da60cece35575b4e0af7e7bf44).
[^2]: [Linux 6.10 executable-loading source](https://git.kernel.org/pub/scm/linux/kernel/git/torvalds/linux.git/tree/fs/exec.c?h=v6.10).
[^3]: [Linux 6.11 executable-loading source](https://git.kernel.org/pub/scm/linux/kernel/git/torvalds/linux.git/tree/fs/exec.c?h=v6.11).
