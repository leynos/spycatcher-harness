//! Readers and checkers behind the Rust build-standard contract.
//!
//! The contract tests in `build_standard_contract.rs` state what the standard
//! requires; this module reads the Cargo configuration and the commands
//! `make -n` would run, and reports where they disagree. It never asserts, so
//! a reader fault surfaces as an error rather than a pass.
//!
//! The Cargo-configuration readers live in `flags.rs`.

use std::process::Command;

#[path = "flags.rs"]
pub mod flags;
#[path = "shell.rs"]
mod shell;

pub use flags::{Flags, LINUX_SELECTOR, LINUX_TABLES, MOLD_FLAG, Read, THREADS_FLAG, sources};
use shell::Line;

/// A host the Makefile can be read as, through its `BUILD_HOST_OS` override,
/// optionally building for another target through `CARGO_BUILD_TARGET`.
///
/// The host carries no expectation: each test states independently whether mold
/// should apply, so a wrong Makefile rule cannot agree with a copy of itself.
#[derive(Clone, Copy, Debug)]
pub enum Host {
    /// Linux building for itself.
    Linux,
    /// macOS, which keeps its platform linker.
    Darwin,
    /// Linux building for the named target triple, as Cargo matches
    /// `[target.*]` sources against the compilation target.
    LinuxBuildingFor(&'static str),
}

impl Host {
    /// Returns the `uname -s` spelling the Makefile compares against.
    const fn uname(self) -> &'static str {
        match self {
            Self::Linux | Self::LinuxBuildingFor(_) => "Linux",
            Self::Darwin => "Darwin",
        }
    }

    /// Returns the `make` overrides that select this host and target.
    fn overrides(self) -> Vec<String> {
        let host = format!("BUILD_HOST_OS={}", self.uname());
        match self {
            Self::LinuxBuildingFor(triple) => vec![host, format!("CARGO_BUILD_TARGET={triple}")],
            Self::Linux | Self::Darwin => vec![host],
        }
    }
}

/// Sets or clears a child's `RUSTFLAGS`, so the harness's own never leaks in.
fn with_inherited(command: &mut Command, inherited: Option<&str>) {
    command.env_remove("RUSTFLAGS");
    if let Some(flags) = inherited {
        command.env("RUSTFLAGS", flags);
    }
}

/// Expands an assigned value the way the recipe's shell would, for the few
/// expansions a gate recipe uses, without running a shell.
///
/// # Errors
///
/// Any other `$`, a backtick or a backslash is syntax this reader does not
/// model, so it fails rather than guessing what the shell would produce.
fn expand_value(value: &str, inherited: Option<&str>) -> Read<String> {
    let caller = inherited.unwrap_or_default();
    let mut out = String::new();
    let mut rest = value;
    while let Some(c) = rest.chars().next() {
        if let Some(tail) = rest.strip_prefix("${RUSTFLAGS:+$RUSTFLAGS }") {
            if !caller.is_empty() {
                out.push_str(caller);
                out.push(' ');
            }
            rest = tail;
        } else if let Some(tail) = rest.strip_prefix("${RUSTFLAGS-}") {
            out.push_str(caller);
            rest = tail;
        } else if matches!(c, '$' | '`' | '\\') {
            return Err(format!("unsupported expansion `{c}` in `{value}`").into());
        } else {
            out.push(c);
            rest = rest.get(c.len_utf8()..).unwrap_or_default();
        }
    }
    Ok(out)
}

/// Expands an assigned `RUSTFLAGS` value into normalised flags.
fn expanded(value: &str, inherited: Option<&str>) -> Read<Flags> {
    let text = expand_value(value, inherited)?;
    Ok(Flags::from_words(
        &text.split_whitespace().collect::<Vec<_>>(),
    ))
}

/// Returns the commands `make -n TARGET` would run on the host, with each
/// backslash-continued recipe line joined into one line.
pub fn dry_run(target: &str, host: Host, inherited: Option<&str>) -> Read<String> {
    dry_run_with("make", target, host, inherited)
}

/// Runs `program` as `make -n` would be run, so a test can point it at a missing
/// or failing stand-in and see the contract fail closed with a named error.
///
/// # Errors
///
/// A program that cannot be spawned, or exits non-zero, is an error naming the
/// program, the target and the host, with the status and both output streams.
pub fn dry_run_with(
    program: &str,
    target: &str,
    host: Host,
    inherited: Option<&str>,
) -> Read<String> {
    let mut make = Command::new(program);
    make.args(["-n", "-B"])
        .args(host.overrides())
        .arg(target)
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        // The host and target come from `Host` alone: an exported target, or an
        // outer `make`'s command-line overrides carried in `MAKEFLAGS`, would
        // change what the recipes expand to for reasons unrelated to them.
        .env_remove("CARGO_BUILD_TARGET")
        .env_remove("MAKEFLAGS")
        .env_remove("MFLAGS")
        .env_remove("MAKELEVEL");
    with_inherited(&mut make, inherited);
    let output = make
        .output()
        .map_err(|error| format!("cannot run `{program} -n {target}` on {host:?}: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "`{program} -n {target}` on {host:?} failed ({}); stdout: {}; stderr: {}",
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }
    Ok(String::from_utf8_lossy(&output.stdout).replace("\\\n", " "))
}

/// Returns the `RUSTFLAGS` one command assigns, or `None` when it assigns none.
///
/// The last assignment wins, as in the shell, so a later empty one cannot be
/// hidden by an earlier non-empty one. Only a leading assignment counts: `RUSTFLAGS` after the
/// command word is an argument. A single-quoted value is taken literally, any other is expanded
/// by the modelled forms.
///
/// # Errors
///
/// A value this reader cannot model fails rather than passing.
fn assignment(command: &str, inherited: Option<&str>) -> Read<Option<Flags>> {
    let (assigned, _) = Line(command).assignments()?;
    let Some(rustflags) = assigned.iter().rev().find(|a| a.name == "RUSTFLAGS") else {
        return Ok(None);
    };
    if rustflags.expands {
        return expanded(rustflags.value, inherited).map(Some);
    }
    Ok(Some(Flags::from_words(
        &rustflags.value.split_whitespace().collect::<Vec<_>>(),
    )))
}

/// Returns, for each cargo or whitaker command `make -n TARGET` would run on
/// the host, the `RUSTFLAGS` it assigns (expanded under `inherited`), or
/// `None` when it assigns none.
pub fn make_rustflags(
    target: &str,
    host: Host,
    inherited: Option<&str>,
) -> Read<Vec<Option<Flags>>> {
    let text = dry_run(target, host, inherited)?;
    rustflags_in(&text, target, host, inherited)
}

/// Reads the `RUSTFLAGS` each cargo or whitaker command in a dry-run `text`
/// assigns, wrapping any parse failure with the route that produced it.
///
/// # Errors
///
/// A line that cannot be split, a leading assignment that cannot be read, or a
/// value that cannot be modelled fails with `make -n TARGET` on the host named.
pub fn rustflags_in(
    text: &str,
    target: &str,
    host: Host,
    inherited: Option<&str>,
) -> Read<Vec<Option<Flags>>> {
    let route = |error: Box<dyn std::error::Error>| {
        format!("reading `make -n {target}` on {host:?}: {error}")
    };
    let mut found = Vec::new();
    for line in text.lines() {
        for command in Line(line).commands().map_err(route)? {
            if Line(command).runs_cargo_or_whitaker().map_err(route)? {
                found.push(assignment(command, inherited).map_err(route)?);
            }
        }
    }
    if found.is_empty() {
        return Err(format!("`make -n {target}` runs no cargo command").into());
    }
    Ok(found)
}

/// One expectation: what a development target must assign on a host, for a
/// caller who exports `inherited`.
struct Route<'a> {
    /// The Makefile target under test.
    target: &'a str,
    /// The host and compilation target the Makefile is read as.
    host: Host,
    /// The caller's exported `RUSTFLAGS`, if any.
    inherited: Option<&'a str>,
    /// Whether mold applies on this host and target.
    expects_mold: bool,
}

/// Returns the problems with one assigned `RUSTFLAGS` for one route.
fn flag_problems(route: &Route, flags: &Flags) -> Vec<String> {
    let Route {
        target,
        host,
        inherited,
        expects_mold,
    } = route;
    let mut problems = Vec::new();
    if !flags.names(THREADS_FLAG) {
        problems.push(format!(
            "`make {target}` on {host:?} drops {THREADS_FLAG}: {flags:?}"
        ));
    }
    if flags.names(MOLD_FLAG) != *expects_mold {
        problems.push(format!(
            "`make {target}` on {host:?} gets mold wrong (expected {expects_mold}): {flags:?}"
        ));
    }
    if inherited.is_some_and(|caller| !flags.carries(&Flags::from_text(caller))) {
        problems.push(format!(
            "`make {target}` drops the caller's RUSTFLAGS: {flags:?}"
        ));
    }
    problems
}

/// Returns the problems with one development target on one host.
fn target_problems(route: &Route) -> Read<Vec<String>> {
    let commands = make_rustflags(route.target, route.host, route.inherited)?;
    let mut problems = Vec::new();
    if route.inherited.is_some() && commands.iter().any(Option::is_none) {
        problems.push(format!(
            "`make {}` runs a command that takes only the caller's RUSTFLAGS",
            route.target
        ));
    }
    // An empty assignment is `Some(Flags(vec![]))` and is checked like any other.
    for flags in commands.iter().flatten() {
        problems.extend(flag_problems(route, flags));
    }
    Ok(problems)
}

/// Checks every development target on one host: an assigned `RUSTFLAGS`
/// carries the frontend flag, carries mold exactly when `expects_mold`, and
/// keeps an inherited `RUSTFLAGS`. Under an inherited `RUSTFLAGS` every command
/// must assign, because the caller's value displaces the configuration's
/// sources; setup-rust exports one in CI.
pub fn check_development_targets(
    targets: &[&str],
    host: Host,
    inherited: Option<&str>,
    expects_mold: bool,
) -> Read<Vec<String>> {
    let per_target = targets
        .iter()
        .map(|&target| {
            target_problems(&Route {
                target,
                host,
                inherited,
                expects_mold,
            })
        })
        .collect::<Read<Vec<_>>>()?;
    Ok(per_target.into_iter().flatten().collect())
}

#[cfg(test)]
#[path = "reader_tests.rs"]
mod reader_tests;
