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

pub use flags::{Flags, LINUX_SELECTOR, LINUX_TABLES, MOLD_FLAG, Read, THREADS_FLAG, sources};

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

/// Expands an assigned value as the recipe's shell would, with or without an
/// inherited `RUSTFLAGS`.
fn expanded(value: &str, inherited: Option<&str>) -> Read<Flags> {
    let mut shell = Command::new("bash");
    shell.args(["-c", &format!("printf '%s' \"{value}\"")]);
    with_inherited(&mut shell, inherited);
    let output = shell.output()?;
    if !output.status.success() {
        return Err(format!(
            "the shell could not expand `{value}` ({}): {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }
    let text = String::from_utf8_lossy(&output.stdout).into_owned();
    Ok(Flags::from_words(
        &text.split_whitespace().collect::<Vec<_>>(),
    ))
}

/// Returns the commands `make -n TARGET` would run on the host, with each
/// backslash-continued recipe line joined into one line.
pub fn dry_run(target: &str, host: Host, inherited: Option<&str>) -> Read<String> {
    let mut make = Command::new("make");
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
    let output = make.output()?;
    if !output.status.success() {
        return Err(format!(
            "`make -n {target}` on {host:?} failed ({}): {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }
    Ok(String::from_utf8_lossy(&output.stdout).replace("\\\n", " "))
}

/// Returns the length of the command separator that starts `rest`, if any:
/// `&&`, `||` or `;`.
fn separator_len(rest: &str) -> Option<usize> {
    if rest.starts_with("&&") || rest.starts_with("||") {
        Some(2)
    } else {
        rest.starts_with(';').then_some(1)
    }
}

/// Strips a leading shell keyword that introduces a command, not a command.
fn without_keyword(part: &str) -> &str {
    ["then ", "else ", "do ", "if "]
        .iter()
        .fold(part, |text, keyword| {
            text.strip_prefix(keyword).unwrap_or(text)
        })
        .trim()
}

/// Splits one recipe line into the simple commands it chains.
///
/// A line can hold several commands joined by `&&`, `||` or `;`, or an
/// `if ... then ... else ... fi` block, and each takes its own `RUSTFLAGS`. A
/// separator inside single or double quotes, or after a backslash, does not
/// split.
///
/// # Errors
///
/// An unterminated quote means the reader cannot tell where a command ends, so
/// it fails rather than guessing.
fn commands(line: &str) -> Read<Vec<&str>> {
    let mut parts = Vec::new();
    let (mut start, mut quote, mut escaped) = (0, None, false);
    let mut chars = line.char_indices();
    while let Some((at, c)) = chars.next() {
        if escaped {
            escaped = false;
        } else if c == '\\' && quote != Some('\'') {
            escaped = true;
        } else if let Some(open) = quote {
            if c == open {
                quote = None;
            }
        } else if c == '"' || c == '\'' {
            quote = Some(c);
        } else if let Some(len) = line.get(at..).and_then(separator_len) {
            parts.push(line.get(start..at).unwrap_or_default());
            start = at + len;
            // The second character of `&&` and `||` is part of the separator.
            for _ in 1..len {
                chars.next();
            }
        }
    }
    if quote.is_some() {
        return Err(format!("unterminated quote in `{line}`").into());
    }
    parts.push(line.get(start..).unwrap_or_default());
    Ok(parts
        .into_iter()
        .map(str::trim)
        .map(without_keyword)
        .filter(|part| !part.is_empty())
        .collect())
}

/// Returns what follows an assigned value: a double-quoted value or a word.
fn after_value(value: &str) -> &str {
    let quoted = value.strip_prefix('"');
    let ends_value = |c: char| {
        if quoted.is_some() {
            c == '"'
        } else {
            c.is_whitespace()
        }
    };
    quoted
        .unwrap_or(value)
        .split_once(ends_value)
        .map_or("", |(_, rest)| rest)
}

/// Returns the command word of a simple command, after any leading
/// `NAME=value` assignments, which is what actually runs.
fn command_word(command: &str) -> &str {
    let mut rest = command.trim_start();
    while let Some((name, value)) = rest.split_once('=') {
        let is_name =
            !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
        if !is_name {
            break;
        }
        rest = after_value(value).trim_start();
    }
    rest.split_whitespace().next().unwrap_or_default()
}

/// Returns whether a command runs cargo or Whitaker, as opposed to naming one,
/// as `command -v whitaker` or an `echo` of its path does.
fn runs_cargo_or_whitaker(command: &str) -> bool {
    let word = command_word(command);
    let name = word.rsplit('/').next().unwrap_or(word);
    matches!(name, "cargo" | "whitaker")
}

/// Returns the `RUSTFLAGS` one command assigns, or `None` when it assigns none.
///
/// The name is matched at a word boundary, so `CARGO_ENCODED_RUSTFLAGS="..."`
/// is not read as `RUSTFLAGS`.
///
/// # Errors
///
/// Any spelling other than a double-quoted value still replaces the
/// configuration's sources, so a form this reader cannot parse fails rather
/// than passing.
fn assignment(command: &str, inherited: Option<&str>) -> Read<Option<Flags>> {
    let at_boundary = |at: &usize| {
        command
            .get(..*at)
            .and_then(|before| before.chars().next_back())
            .is_none_or(char::is_whitespace)
    };
    let Some(start) = command
        .match_indices("RUSTFLAGS=")
        .map(|(at, _)| at)
        .find(at_boundary)
    else {
        return Ok(None);
    };
    let rest = command
        .get(start + "RUSTFLAGS=".len()..)
        .and_then(|value| value.strip_prefix('"'))
        .ok_or_else(|| format!("unreadable RUSTFLAGS assignment in `{command}`"))?;
    let (value, _) = rest
        .split_once('"')
        .ok_or_else(|| format!("unterminated RUSTFLAGS in `{command}`"))?;
    expanded(value, inherited).map(Some)
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
    let mut found = Vec::new();
    for line in text.lines() {
        for command in self::commands(line)? {
            if runs_cargo_or_whitaker(command) {
                found.push(assignment(command, inherited)?);
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
    if inherited.is_some_and(|caller| !flags.carries_run(caller)) {
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
