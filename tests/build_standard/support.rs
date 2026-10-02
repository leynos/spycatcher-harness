//! Readers and checkers behind the Rust build-standard contract.
//!
//! The contract tests in `build_standard_contract.rs` state what the standard
//! requires; this module reads the Cargo configuration and the commands
//! `make -n` would run, and reports where they disagree. It never asserts, so
//! a reader fault surfaces as an error rather than a pass.
//!
//! File access goes through a `cap_std` directory handle rooted at the crate
//! manifest directory.

use std::{error::Error, process::Command};

use cap_std::{ambient_authority, fs::Dir};

/// The parallel-frontend flag every `rustflags` source must carry.
pub const THREADS_FLAG: &str = "-Zthreads=8";

/// The linker flag the Linux source must add, normalized to one token.
pub const MOLD_FLAG: &str = "-Clink-arg=-fuse-ld=mold";

/// Target table keys that apply on Linux alone.
pub const LINUX_TABLES: [&str; 2] = ["x86_64-unknown-linux-gnu", "cfg(target_os = \"linux\")"];

/// The result of a reader, which the tests unwrap.
pub type Read<T> = Result<T, Box<dyn Error>>;

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

/// One `rustflags` list, with `-C value` pairs joined into `-Cvalue` so both
/// spellings compare equal.
#[derive(Debug)]
pub struct Flags(Vec<String>);

impl Flags {
    /// Normalizes a word list into flags.
    fn from_words<S: AsRef<str>>(words: &[S]) -> Self {
        let mut joined: Vec<String> = Vec::new();
        for word in words.iter().map(AsRef::as_ref) {
            match joined.last_mut() {
                Some(last) if last == "-C" => *last = format!("-C{word}"),
                _ => joined.push(word.to_owned()),
            }
        }
        Self(joined)
    }

    /// Returns whether the list names one flag.
    pub fn names(&self, flag: &str) -> bool {
        self.0.iter().any(|candidate| candidate == flag)
    }

    /// Returns whether the list holds the caller's words as one unbroken run.
    fn carries_run(&self, caller: &str) -> bool {
        let wanted: Vec<&str> = caller.split_whitespace().collect();
        self.0
            .windows(wanted.len())
            .any(|run| run.iter().map(String::as_str).eq(wanted.iter().copied()))
    }

    /// Returns the list without the linker flag, for comparing sources.
    pub fn without_mold(self) -> Vec<String> {
        self.0
            .into_iter()
            .filter(|flag| flag != MOLD_FLAG)
            .collect()
    }
}

/// Reads one table's `rustflags`, if it has any.
fn table_flags(table: &toml::Value) -> Option<Flags> {
    let words: Vec<&str> = table
        .get("rustflags")?
        .as_array()?
        .iter()
        .filter_map(toml::Value::as_str)
        .collect();
    Some(Flags::from_words(&words))
}

/// Reads a file relative to the crate manifest directory.
pub fn read(path: &str) -> Read<String> {
    let root = Dir::open_ambient_dir(env!("CARGO_MANIFEST_DIR"), ambient_authority())?;
    Ok(root.read_to_string(path)?)
}

/// Returns every `rustflags` source in the configuration, by table name.
pub fn sources() -> Read<Vec<(String, Flags)>> {
    let config: toml::Value = toml::from_str(&read(".cargo/config.toml")?)?;
    let mut found = Vec::new();
    if let Some(flags) = config.get("build").and_then(table_flags) {
        found.push(("build".to_owned(), flags));
    }
    if let Some(targets) = config.get("target").and_then(toml::Value::as_table) {
        for (key, table) in targets {
            if let Some(flags) = table_flags(table) {
                found.push((key.clone(), flags));
            }
        }
    }
    Ok(found)
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
        return Err(format!("the shell could not expand `{value}`").into());
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
        return Err(format!("`make -n {target}` failed").into());
    }
    Ok(String::from_utf8_lossy(&output.stdout).replace("\\\n", " "))
}

/// Splits one recipe line into the simple commands it chains.
///
/// A line can hold several commands joined by `&&`, `||` or `;`, or an
/// `if ... then ... else ... fi` block, and each takes its own `RUSTFLAGS`.
fn commands(line: &str) -> Vec<&str> {
    line.split("&&")
        .flat_map(|part| part.split("||"))
        .flat_map(|part| part.split(';'))
        .map(str::trim)
        .map(|part| {
            ["then ", "else ", "do ", "if "]
                .iter()
                .fold(part, |text, keyword| {
                    text.strip_prefix(keyword).unwrap_or(text)
                })
                .trim()
        })
        .filter(|part| !part.is_empty())
        .collect()
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
    let commands = dry_run(target, host, inherited)?
        .lines()
        .flat_map(commands)
        .filter(|command| runs_cargo_or_whitaker(command))
        .map(|command| assignment(command, inherited))
        .collect::<Read<Vec<_>>>()?;
    if commands.is_empty() {
        return Err(format!("`make -n {target}` runs no cargo command").into());
    }
    Ok(commands)
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
