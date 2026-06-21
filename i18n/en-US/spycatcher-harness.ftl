harness-error-invalid-config = invalid configuration: { $message }
harness-error-cassette-not-found = cassette not found: { $cassette_name }
harness-error-request-mismatch = request mismatch at interaction { $interaction_id }: expected { $expected_hash }, observed { $observed_hash }; { $diff_summary }
harness-error-invalid-cassette = invalid cassette: { $message }
harness-error-unsupported-cassette-format-version = unsupported cassette format version { $found }; supported version is { $supported }
harness-error-upstream-request-failed = upstream request failed: { $source }
harness-error-mode-not-yet-implemented = mode not yet implemented: { $mode }
harness-error-io = io failure: { $source }

cli-about = Deterministic record/replay harness for LLM API testing
cli-long-about =
    Deterministic record/replay harness for LLM API testing.
    { $binary } records upstream LLM API traffic into cassettes, replays
    cassette responses for repeatable tests, and verifies cassette integrity
    before those cassettes are used in automation.
cli-usage = { $binary } <COMMAND>
cli-version = { $version }
cli-merge-help =
    Configuration precedence: CLI > env > config files > defaults.
    Subcommand defaults merge from the `cmds` namespace.
    Example:
      { "[" }cmds.record{ "]" }
      cassette_name = "session_a"
      { "[" }cmds.record.upstream{ "]" }
      kind = "openrouter"
      base_url = "https://openrouter.ai/api/v1"
      api_key_env = "OPENROUTER_API_KEY"
    Environment prefix: SPYCATCHER_HARNESS_CMDS_<SUBCOMMAND>_...
    Nested keys use double underscores, e.g.
    SPYCATCHER_HARNESS_CMDS_RECORD_UPSTREAM__BASE_URL.

cli-record-about = Proxy to upstream and record interactions.
cli-replay-about = Replay interactions from a cassette.
cli-verify-about = Verify cassette and configuration integrity.

clap-error-missing-argument = missing required argument: { $argument }
clap-error-unknown-argument = unknown argument: { $argument }
clap-error-invalid-value = invalid value { $value } for { $argument }; valid values: { $valid_values }
clap-error-invalid-subcommand = invalid subcommand { $subcommand }; valid subcommands: { $valid_subcommands }
clap-error-missing-subcommand = missing required subcommand; valid subcommands: { $valid_subcommands }
clap-error-no-equals = expected `=` between { $argument } and { $value }
clap-error-too-many-values = too many values for { $argument }; expected { $expected }, got { $actual }
clap-error-too-few-values = too few values for { $argument }; expected at least { $min }, got { $actual }
clap-error-value-validation = invalid value { $value } for { $argument }
clap-error-argument-conflict = argument conflict involving { $argument }
clap-error-invalid-utf8 = invalid UTF-8 in { $argument }
clap-error-io = I/O failure while reading { $argument }
clap-error-format = invalid command-line format near { $argument }
