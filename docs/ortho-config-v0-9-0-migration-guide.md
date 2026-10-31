# Migration guide: v0.8.0 to v0.9.0

## Who should read this

Read this guide before upgrading an application or library from OrthoConfig
v0.8.0 to v0.9.0. Most existing derives continue to compile unchanged, but two
runtime contracts need review:

- `ConfigDiscovery::load_first` now reports accumulated candidate failures; and
- YAML files use YAML 1.2 parsing and reject duplicate keys.

Everything else is additive or removes integration work. The sections below
separate required changes from improvements that can be adopted when useful.

## Impact at a glance

| Priority               | Area                              | What to do                                                                         |
| ---------------------- | --------------------------------- | ---------------------------------------------------------------------------------- |
| **Required**           | Dependency versions               | Update every direct `ortho_config` and `ortho_config_macros` requirement together. |
| **Required if called** | `ConfigDiscovery::load_first`     | Handle `Err` when candidates exist but all readable candidates fail.               |
| **Review if enabled**  | YAML                              | Test representative files against YAML 1.2 booleans and duplicate-key rejection.   |
| **Recommended**        | Discovery tests                   | Replace process-environment mutation with `MapEnv`.                                |
| **Recommended**        | Custom discovery                  | Move bespoke flag and filename wiring into `discovery(...)`.                       |
| **Recommended**        | Localized CLIs                    | Parse through `LocalizedParse` or `parse_localized_command`.                       |
| **Recommended**        | Subcommand documentation          | Derive `OrthoConfigSubcommandDocs` so generated metadata is complete.              |
| **Optional, low cost** | Dependency aliases and re-exports | Point derives at an alias and remove direct implementation-only dependencies.      |
| **Optional, low cost** | Tracing and metrics               | Observe discovery decisions; enable counters only when wanted.                     |
| **Optional**           | Agent context                     | Publish a compact machine-readable command contract.                               |

_Table 1: Application-facing work when moving from v0.8.0 to v0.9.0._

## 1. Update dependency versions

Update the runtime and macro crates as one unit if both are direct dependencies.

Before:

```toml
[dependencies]
ortho_config = { version = "0.8.0", features = ["yaml"] }
ortho_config_macros = "0.8.0"
```

After:

```toml
[dependencies]
ortho_config = { version = "0.9.0", features = ["yaml"] }
ortho_config_macros = "0.9.0"
```

Most applications only need `ortho_config`; it re-exports the derive macros. If
application source does not import `ortho_config_macros` directly, remove that
dependency rather than carrying two version requirements.

Format features now flow from `ortho_config` to `ortho_config_macros`, so keep
`toml`, `json5`, and `yaml` on the runtime dependency. Equivalent feature
selections no longer need coordination on a direct macro dependency.

## 2. Handle failed discovery explicitly

### What breaks

In v0.8.0, a caller could receive `Ok(None)` after discovery found candidates
but failed to load all of them. In v0.9.0, `ConfigDiscovery::load_first`
returns:

- `Ok(Some(figment))` when a candidate loads;
- `Ok(None)` only when no candidate exists and discovery records no error; or
- `Err(error)` when every candidate fails and at least one failure was
  recorded.

This prevents a malformed, unreadable, or otherwise broken configuration from
being mistaken for an absent configuration.

### Migration

Do not collapse `Err` and `Ok(None)` into the same fallback:

```rust
use ortho_config::{ConfigDiscovery, OrthoResult};

fn load_optional() -> OrthoResult<()> {
    let discovery = ConfigDiscovery::builder("acme").build();

    match discovery.load_first()? {
        Some(figment) => {
            let _loaded = figment;
            println!("configuration loaded");
        }
        None => println!("no configuration file found; using defaults"),
    }

    Ok(())
}
```

If v0.8.0 code intentionally ignored malformed optional files, reproduce that
policy explicitly at the application boundary and log it. Do not convert every
error into `None`; doing so reinstates the ambiguity this change removes.

## 3. Review YAML files and feature selection

### What can break

The `yaml` feature now uses the `SaphyrYaml` provider backed by `serde-saphyr`.
It follows YAML 1.2 semantics:

- unquoted `yes`, `no`, `on`, and `off` are strings rather than booleans; and
- duplicate mapping keys are errors rather than silently overwriting an
  earlier value.

A v0.8.0 file that relied on YAML 1.1 boolean spellings may fail to deserialize
into a Boolean field. A file containing duplicate keys now fails early.

Before, where `yes` could be interpreted as a Boolean:

```yaml
enabled: yes
```

After, use an unambiguous YAML 1.2 Boolean:

```yaml
enabled: true
```

Remove duplicate keys and decide which value should survive. Run production
samples through v0.9.0 as part of the upgrade rather than waiting for the first
deployment load.

The `yaml` feature requires `serde_json`, which is enabled by default. A
consumer using `default-features = false` must select both:

```toml
ortho_config = {
  version = "0.9.0",
  default-features = false,
  features = ["serde_json", "yaml"]
}
```

The old transitive `figment/yaml` integration is gone. Application code that
directly named its provider should use `ortho_config::serde_saphyr` or
OrthoConfig's file-loading APIs.

## 4. Expect clearer inheritance errors

Missing `extends` targets now report the resolved absolute path and the file
that referenced it. This changes error text, not the success path.

Update snapshot or approval tests that assert the v0.8.0 message. Replace any
parsing of human-readable error strings with matching on the public error type
or with an application-owned error mapping. The more precise message is
intended for people and is not a stable machine protocol.

## 5. Adopt hermetic discovery tests

### Why change an existing pattern

Production discovery still reads the live process environment by default, so no
application change is required. Tests can now inject `MapEnv` through
`ConfigDiscoveryBuilder::env_source`, avoiding global environment mutation and
serialization locks:

```rust
use ortho_config::{ConfigDiscovery, MapEnv};
use std::sync::Arc;

let environment = Arc::new(
    MapEnv::new().with_var("ACME_CONFIG", "/etc/acme/config.toml"),
);
let discovery = ConfigDiscovery::builder("acme")
    .env_var("ACME_CONFIG")
    .env_source(environment)
    .build();

assert_eq!(
    discovery.candidates().first().map(|path| path.as_path()),
    Some(std::path::Path::new("/etc/acme/config.toml"))
);
```

`MapEnv` supports `with_var`, `insert`, `remove`, and `FromIterator`. A custom
source can implement the object-safe `EnvSource` trait.

Two boundaries matter:

- injection controls discovery inputs: the explicit selector, XDG or Windows
  base directories, and home-directory resolution;
- it does not yet replace the `APP_*` configuration-value merge layer, which
  still uses the process environment.

`MapEnv::home_fallback` returns `None`, preventing an injected test from
silently using the host's home directory. `ProcessEnv`, the production default,
preserves the v0.8.0 platform fallback.

## 6. Declare discovery beside the configuration

v0.8.0 applications often assembled `ConfigDiscovery` manually to rename the
configuration option or searched files. v0.9.0 can generate that wiring from
the derive:

```rust
use ortho_config::OrthoConfig;
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, OrthoConfig)]
#[ortho_config(
    prefix = "ACME_",
    discovery(
        app_name = "acme-server",
        config_file_name = "server.toml",
        dotfile_name = ".acme-server.toml",
        project_file_name = ".acme-server.toml",
        config_cli_long = "config",
        config_cli_short = 'c',
        config_cli_visible = true
    )
)]
struct Config {
    #[ortho_config(default = 8080)]
    port: u16,
}
```

This is recommended when those names are part of the public CLI contract. It
keeps loading and generated documentation in agreement. Existing manual
`ConfigDiscovery` code remains supported and need not change if it performs
application-specific work that the attribute does not express.

## 7. Complete documentation for subcommands

`OrthoConfigDocs` metadata now supports recursive `DocMetadata.subcommands`.
Derive `OrthoConfigSubcommandDocs` on the enum stored in a
`#[command(subcommand)]` field:

```rust
use clap::{Args, Parser, Subcommand};
use ortho_config::{OrthoConfig, OrthoConfigSubcommandDocs};
use serde::{Deserialize, Serialize};

#[derive(Parser, Deserialize, Serialize, OrthoConfig)]
#[ortho_config(prefix = "ACME_")]
struct Cli {
    #[serde(skip)]
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, OrthoConfigSubcommandDocs)]
enum Commands {
    Serve(ServeConfig),
}

impl Default for Commands {
    fn default() -> Self {
        Self::Serve(ServeConfig::default())
    }
}

#[derive(Default, Args, Deserialize, Serialize, OrthoConfig)]
#[ortho_config(prefix = "ACME_SERVE_")]
struct ServeConfig {
    #[arg(long)]
    port: Option<u16>,
}
```

Loading behaviour is unchanged. Adopt the derive to let `cargo-orthohelp`
produce complete nested IR, man pages, PowerShell help, and agent context.
Renderer integrations that deserialize `DocMetadata` should tolerate the new
recursive field and use `ORTHO_DOCS_IR_VERSION` rather than assuming the v0.8.0
shape.

Generated support structs now carry doc comments. Crates with strict
`missing_docs` should be able to remove workarounds that suppressed warnings
originating in the derive.

## 8. Localize the whole parse path

v0.8.0 exposed helpers for translating command metadata and errors separately.
It was easy for an application to localize `--help` but return an untranslated
parse error. v0.9.0 adds two public entry points:

- `LocalizedParse` builds, localizes, parses, and localizes failures for any
  `clap::Parser`;
- `parse_localized_command` does the same when the application has already
  built a command or uses `LocalizeCmd::with_base`.

For the common case:

```rust
use clap::Parser;
use ortho_config::{LocalizedParse, NoOpLocalizer};

#[derive(Parser)]
#[command(name = "acme", bin_name = "acme")]
struct Cli {
    #[arg(long)]
    verbose: bool,
}

let localizer = NoOpLocalizer::new();
let cli = Cli::try_parse_localized_from(["acme", "--verbose"], &localizer)?;
assert!(cli.verbose);
# Ok::<(), clap::Error>(())
```

Existing calls to `LocalizeCmd`, `localize_clap_error`, and
`localize_clap_error_with_command` remain available. Move to the combined path
when possible so future command changes cannot bypass localization.

## 9. Use dependency aliases and runtime re-exports

### Aliases

Derive-generated paths previously assumed that the dependency was named
`ortho_config`. If a workspace aliases it, tell the derive which path to use:

```toml
[dependencies]
config_layer = { package = "ortho_config", version = "0.9.0" }
```

```rust
use config_layer::OrthoConfig;
use serde::Deserialize;

#[derive(Deserialize, OrthoConfig)]
#[ortho_config(crate = "config_layer", prefix = "ACME_")]
struct Config {
    #[ortho_config(default = 8080)]
    port: u16,
}
```

Apply the same `crate = "..."` attribute to enums deriving
`SelectedSubcommandMerge`. Canonically named dependencies need no attribute.

### Re-exports

The runtime now re-exports `figment`, `uncased`, `xdg` on supported platforms,
and enabled format parsers (`figment_json5`, `json5`, `serde_saphyr`, and
`toml`). Generated code uses those paths, so derive-only consumers can remove
direct dependencies that existed solely to satisfy macro expansion.

Keep a direct dependency when application source imports that crate itself or
needs a different feature set. This cleanup is optional; retaining a compatible
direct dependency does not change behaviour.

## 10. Observe discovery safely

v0.9.0 emits structured `tracing` events for discovery source selection,
selector resolution, platform-directory resolution, attempts, candidate
outcomes, and the terminal load outcome. A successful load identifies the
bounded source category that won.

No feature is required for tracing. The library installs no subscriber, so
existing applications remain quiet unless their subscriber enables these
events. Use `ortho_config=debug` while diagnosing discovery.

Event fields come from a closed vocabulary and exclude environment values,
resolved paths, and file contents. `Debug` implementations for `MapEnv` and
`ConfigDiscoveryBuilder` follow the same rule. Subscriber-added span fields
remain the application's responsibility.

Applications with a metrics recorder can also enable:

```toml
[dependencies]
ortho_config = { version = "0.9.0", features = ["metrics"] }
```

This emits:

- `ortho_config.discovery.attempts`, labelled by operation;
- `ortho_config.discovery.outcomes`, labelled by operation and outcome; and
- `ortho_config.discovery.candidate_failures`, labelled by operation, bounded
  source, and error category.

The feature is off by default and never installs a recorder. It is therefore a
small opt-in for applications that already export `metrics` data.

## 11. Add agent-facing context when useful

v0.9.0 introduces a machine-readable agent-context model alongside the human
documentation IR. Public types include `AgentContext`, `AgentCommand`,
`AgentInput`, `AgentExample`, policy and effect enums, `SkillManifest`, and
`SkillCommandRef`.

Adoption is optional. A CLI can expose a downstream `context --json` command,
and `cargo-orthohelp --format agent-context` generates the same class of
compact contract. `--format all` now includes agent context. Consumers should
use the `schema_version` and `kind` fields when validating the document.

`AgentContext.skill_manifests` contains structured descriptors, not bare paths,
and defaults to an empty list during deserialization. Applications with an
earlier design draft using `skill_manifest_paths` should rename that field and
map entries to `SkillManifest`. That design name was not a v0.8.0 runtime API,
so published v0.8.0 users have no required code change.

## 12. Do not depend on proposed errors

`OrthoError::MissingRequiredValues` is not part of v0.9.0. It remains proposed
future work. Continue matching the error variants actually exported by the
crate, and keep a wildcard arm because `OrthoError` is non-exhaustive.

## Upgrade checklist

- [ ] Update every OrthoConfig crate requirement to v0.9.0.
- [ ] Audit every `ConfigDiscovery::load_first` call for distinct absent and
  failed paths.
- [ ] If YAML is enabled, test real files for YAML 1.2 booleans and duplicate
  keys.
- [ ] Update snapshots that assert missing-`extends` error text.
- [ ] Run tests with the feature combinations shipped by the application.
- [ ] Prefer `MapEnv` in discovery tests that currently mutate global state.
- [ ] Adopt `discovery(...)`, combined localization, and subcommand docs where
  they replace application glue.
- [ ] Remove implementation-only direct dependencies only after confirming
  application source does not import them.
- [ ] Enable metrics or agent context only when the application has a consumer
  for them.

The [user's guide](users-guide.md) contains complete worked examples for each
new pattern. The [changelog](../CHANGELOG.md) remains the concise release
inventory.
