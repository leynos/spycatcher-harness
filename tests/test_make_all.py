"""Check the comprehensive Make target with harmless replacement gate recipes."""

from __future__ import annotations

import json
import os
import re
import shlex
import subprocess
import sys
import tempfile
import time
import unittest
from collections import Counter
from pathlib import Path


REPOSITORY = Path(__file__).resolve().parents[1]
GATES = ("check-fmt", "lint", "test", "spelling", "test-workflow-contracts")
ALL_TARGET = re.compile(r"^all:\s*([^#\n]*)", re.MULTILINE)


def run_gate(probe_dir: Path, gate: str, should_fail: bool) -> int:
    """Record one simulated gate interval and return its requested status.

    For example, ``run_gate(path, "lint", False)`` writes one timing record and
    returns zero. The hold gives parallel Make jobs time to overlap visibly.
    """
    started = time.monotonic_ns()
    time.sleep(float(os.environ["MAKE_ALL_PROBE_HOLD_SECONDS"]))
    finished = time.monotonic_ns()
    (probe_dir / f"{gate}-{os.getpid()}.json").write_text(
        json.dumps({"gate": gate, "started": started, "finished": finished}),
        encoding="utf-8",
    )
    return 87 if should_fail else 0


def all_prerequisites(makefile: str) -> list[str]:
    """Read the ``all`` dependency list before substituting its leaf recipes.

    For example, an ``all: test lint`` line yields ``["test", "lint"]``. An
    unfamiliar target is refused so the probe never runs its real recipe.
    """
    match = ALL_TARGET.search(makefile)
    if match is None:
        raise AssertionError("Makefile has no all target")
    prerequisites = match.group(1).split()
    if len(prerequisites) != len(set(prerequisites)):
        raise AssertionError("all repeats a prerequisite")
    if not set(prerequisites).issubset(GATES):
        raise AssertionError(f"all has an unstubbed prerequisite: {prerequisites}")
    return prerequisites


def has_overlap(records: list[dict[str, int | str]]) -> bool:
    """Identify concurrent intervals, including intervals nested in another.

    For example, ``[1, 10]`` and ``[2, 3]`` overlap even when a third interval
    begins after the shorter interval ends.
    """
    latest_end = -1
    for record in sorted(records, key=lambda item: int(item["started"])):
        start = int(record["started"])
        if start < latest_end:
            return True
        latest_end = max(latest_end, int(record["finished"]))
    return False


def replace_all_prerequisites(makefile: str, prerequisites: tuple[str, ...]) -> str:
    """Change only the temporary ``all`` line for a regression control.

    For example, passing a reversed gate tuple exercises a different schedule
    while the real target's serial execution requirement remains in force.
    """
    updated, replacements = ALL_TARGET.subn(
        "all: " + " ".join(prerequisites) + " ", makefile, count=1
    )
    if replacements != 1:
        raise AssertionError("could not replace the all target")
    return updated


class MakeAllContract(unittest.TestCase):
    """Exercise the real Make dependency graph without running repository gates."""

    def run_probe(
        self,
        jobs: int,
        *,
        makefile_text: str | None = None,
        failing_gate: str | None = None,
        hold_seconds: float = 0.2,
    ) -> tuple[subprocess.CompletedProcess[str], list[dict[str, int | str]]]:
        """Run Make with stub leaves and return its exit plus recorded intervals.

        For example, ``run_probe(2)`` checks the two-job scheduler against the
        same Makefile that ordinary contributors invoke.
        """
        source = (REPOSITORY / "Makefile").read_text(encoding="utf-8")
        contents = source if makefile_text is None else makefile_text
        all_prerequisites(contents)
        with tempfile.TemporaryDirectory(prefix="spycatcher-pr129-make-all-") as scratch:
            probe_dir = Path(scratch)
            makefile = REPOSITORY / "Makefile"
            if makefile_text is not None:
                makefile = probe_dir / "Makefile"
                makefile.write_text(contents, encoding="utf-8")
            override = probe_dir / "override.mk"
            python = shlex.quote(sys.executable)
            script = shlex.quote(str(Path(__file__).resolve()))
            output_dir = shlex.quote(str(probe_dir))
            override.write_text(
                "".join(
                    f"{gate}:\n"
                    f"\t@{python} {script} --gate {output_dir} {gate} "
                    f"{int(gate == failing_gate)}\n"
                    for gate in GATES
                ),
                encoding="utf-8",
            )
            environment = os.environ.copy()
            environment.pop("MAKEFLAGS", None)
            environment.pop("GNUMAKEFLAGS", None)
            environment["MAKE_ALL_PROBE_HOLD_SECONDS"] = str(hold_seconds)
            completed = subprocess.run(
                [
                    "make",
                    "--no-print-directory",
                    f"-j{jobs}",
                    "-f",
                    str(makefile),
                    "-f",
                    str(override),
                    "all",
                    "CARGO=/bin/true",
                ],
                cwd=REPOSITORY,
                env=environment,
                capture_output=True,
                text=True,
                timeout=30,
                check=False,
            )
            records = [
                json.loads(path.read_text(encoding="utf-8"))
                for path in probe_dir.glob("*.json")
            ]
            return completed, records

    def assert_gates_are_serial(
        self,
        completed: subprocess.CompletedProcess[str],
        records: list[dict[str, int | str]],
    ) -> None:
        """Require each expected gate once and no overlapping time interval.

        For example, a missing workflow-contract record fails even when Make
        itself exits successfully.
        """
        details = f"stdout={completed.stdout}\nstderr={completed.stderr}\nrecords={records}"
        self.assertEqual(completed.returncode, 0, details)
        self.assertEqual(Counter(record["gate"] for record in records), Counter(GATES), details)
        self.assertFalse(has_overlap(records), details)

    def test_all_gates_are_serial_for_representative_job_counts(self) -> None:
        """The real ``all`` target runs five gates once, even under ``-j``."""
        for jobs in (1, 2, 5):
            with self.subTest(jobs=jobs):
                self.assert_gates_are_serial(*self.run_probe(jobs))

    def test_gate_order_does_not_change_serial_contract(self) -> None:
        """A different prerequisite order still avoids gate overlap."""
        source = (REPOSITORY / "Makefile").read_text(encoding="utf-8")
        reversed_order = replace_all_prerequisites(source, tuple(reversed(GATES)))
        self.assert_gates_are_serial(*self.run_probe(5, makefile_text=reversed_order))

    def test_workflow_contract_failure_reaches_make(self) -> None:
        """A failing workflow contract makes the outer Make invocation fail."""
        completed, records = self.run_probe(5, failing_gate="test-workflow-contracts")
        details = f"stdout={completed.stdout}\nstderr={completed.stderr}\nrecords={records}"
        self.assertNotEqual(completed.returncode, 0, details)
        self.assertEqual(Counter(record["gate"] for record in records), Counter(GATES), details)
        self.assertFalse(has_overlap(records), details)

    def test_missing_contract_dependency_is_detected(self) -> None:
        """The harness refuses a Makefile that omits the CV-005 gate."""
        source = (REPOSITORY / "Makefile").read_text(encoding="utf-8")
        missing = replace_all_prerequisites(source, GATES[:-1])
        completed, records = self.run_probe(5, makefile_text=missing)
        self.assertEqual(completed.returncode, 0, completed.stderr)
        self.assertNotIn("test-workflow-contracts", {record["gate"] for record in records})
        with self.assertRaises(AssertionError):
            self.assert_gates_are_serial(completed, records)

    def test_removed_serialization_is_detected(self) -> None:
        """The harness notices overlapping gates without ``.NOTPARALLEL``."""
        source = (REPOSITORY / "Makefile").read_text(encoding="utf-8")
        self.assertIn(".NOTPARALLEL: all", source)
        parallel = source.replace(".NOTPARALLEL: all", "", 1)
        completed, records = self.run_probe(5, makefile_text=parallel, hold_seconds=1.0)
        self.assertEqual(completed.returncode, 0, completed.stderr)
        self.assertEqual(Counter(record["gate"] for record in records), Counter(GATES))
        self.assertTrue(has_overlap(records))
        with self.assertRaises(AssertionError):
            self.assert_gates_are_serial(completed, records)


if __name__ == "__main__":
    if len(sys.argv) == 5 and sys.argv[1] == "--gate":
        sys.exit(run_gate(Path(sys.argv[2]), sys.argv[3], bool(int(sys.argv[4]))))
    unittest.main()
