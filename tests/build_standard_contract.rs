//! Contract tests for the Rust build standard.
//!
//! The standard makes the parallel `rustc` frontend the default for every
//! development build and mold the default linker on Linux. Cargo reads both
//! from `.cargo/config.toml`, but it applies a single `rustflags` source rather
//! than merging them, and an assigned `RUSTFLAGS` replaces every source. So the
//! flags must be repeated in each configuration source, restated wherever the
//! Makefile assigns `RUSTFLAGS` for a development target, and kept out of the
//! coverage and release recipes, which measure or ship and so stay on the
//! default flags.
//!
//! The Makefile clauses run `make -n` and read the commands it would run,
//! rather than the Makefile's text, so a flag lost through a variable or a
//! recipe edit fails here. Each assigned value is expanded by the shell, with
//! and without an inherited `RUSTFLAGS`, exactly as the recipe would expand
//! it. The clauses run once as a Linux host and once as a macOS host, because
//! mold is added on Linux alone. File access goes through a
//! `cap_std` directory handle rooted at the crate manifest directory.

use std::{error::Error, process::Command};

use cap_std::{ambient_authority, fs::Dir};

/// The parallel-frontend flag every `rustflags` source must carry.
const THREADS_FLAG: &str = "-Zthreads=8";

/// The linker flag the Linux source must add, normalized to one token.
const MOLD_FLAG: &str = "-Clink-arg=-fuse-ld=mold";

/// Target table keys that apply on Linux alone.
const LINUX_TABLES: [&str; 2] = ["x86_64-unknown-linux-gnu", "cfg(target_os = \"linux\")"];

/// Makefile targets that build for development. A command in one either
/// assigns `RUSTFLAGS` with the standard flags or assigns none and so takes
/// the configuration's.
const DEVELOPMENT_TARGETS: [&str; 4] = ["test", "typecheck", "lint", "build"];

/// Makefile targets that measure or ship, so every command assigns
/// `RUSTFLAGS` and none carries a standard flag.
const HELD_OUT_TARGETS: [&str; 1] = ["release"];

/// Development targets that must assign `RUSTFLAGS` in at least one command,
/// so the restatement checks above cannot pass by finding nothing to check.
const ASSIGNING_TARGETS: [&str; 2] = ["test", "build"];

/// A caller's own flags, distinct from anything a recipe adds, to prove a
/// recipe composes an exported `RUSTFLAGS` with the standard flags rather than
/// replacing either.
const INHERITED: &str = "--cfg inherited_from_caller";

/// The result of a reader, which the tests unwrap.
type Read<T> = Result<T, Box<dyn Error>>;

/// A host the Makefile can be read as, through its `BUILD_HOST_OS` override,
/// optionally building for another target through `CARGO_BUILD_TARGET`.
#[derive(Clone, Copy, Debug)]
enum Host {
    /// Linux building for itself, where the standard adds mold.
    Linux,
    /// macOS, which keeps its platform linker.
    Darwin,
    /// Linux building for the named target triple, which gets mold only when
    /// the triple is itself Linux, as Cargo matches `[target.*]` sources.
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

    /// Returns whether the standard adds mold on this host and target.
    fn expects_mold(self) -> bool {
        match self {
            Self::Linux => true,
            Self::Darwin => false,
            Self::LinuxBuildingFor(triple) => triple.contains("-linux-"),
        }
    }
}

/// One `rustflags` list, with `-C value` pairs joined into `-Cvalue` so both
/// spellings compare equal.
#[derive(Debug)]
struct Flags(Vec<String>);

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
    fn names(&self, flag: &str) -> bool {
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
    fn without_mold(self) -> Vec<String> {
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

/// Returns every `rustflags` source in the configuration, by table name.
fn sources() -> Read<Vec<(String, Flags)>> {
    let root = Dir::open_ambient_dir(env!("CARGO_MANIFEST_DIR"), ambient_authority())?;
    let text = root.read_to_string(".cargo/config.toml")?;
    let config: toml::Value = toml::from_str(&text)?;
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
/// backslash-continued recipe line joined into one command.
fn dry_run(target: &str, host: Host, inherited: Option<&str>) -> Read<String> {
    let mut make = Command::new("make");
    make.args(["-n", "-B"])
        .args(host.overrides())
        .arg(target)
        .current_dir(env!("CARGO_MANIFEST_DIR"));
    with_inherited(&mut make, inherited);
    let output = make.output()?;
    if !output.status.success() {
        return Err(format!("`make -n {target}` failed").into());
    }
    Ok(String::from_utf8_lossy(&output.stdout).replace("\\\n", " "))
}

/// Returns the `RUSTFLAGS` one command assigns, or `None` when it assigns none.
///
/// # Errors
///
/// Any spelling other than a double-quoted value still replaces the
/// configuration's sources, so a form this reader cannot parse fails rather
/// than passing.
fn assignment(line: &str, inherited: Option<&str>) -> Read<Option<Flags>> {
    let Some((_, rest)) = line.split_once("RUSTFLAGS=\"") else {
        return if line.contains("RUSTFLAGS=") {
            Err(format!("unreadable RUSTFLAGS assignment in `{line}`").into())
        } else {
            Ok(None)
        };
    };
    let (value, _) = rest
        .split_once('"')
        .ok_or_else(|| format!("unterminated RUSTFLAGS in `{line}`"))?;
    expanded(value, inherited).map(Some)
}

/// Returns whether a dry-run line runs cargo or Whitaker, as opposed to
/// merely naming one, as an `echo` of the Whitaker path does.
fn runs_cargo_or_whitaker(line: &str) -> bool {
    let first = line.split_whitespace().next().unwrap_or_default();
    !matches!(first, "echo" | "printf") && (line.contains("cargo") || line.contains("whitaker"))
}

/// Returns, for each cargo or whitaker command `make -n TARGET` would run on
/// the host, the `RUSTFLAGS` it assigns (expanded under `inherited`), or
/// `None` when it assigns none.
fn make_rustflags(target: &str, host: Host, inherited: Option<&str>) -> Read<Vec<Option<Flags>>> {
    let commands = dry_run(target, host, inherited)?
        .lines()
        .filter(|line| runs_cargo_or_whitaker(line))
        .map(|line| assignment(line, inherited))
        .collect::<Read<Vec<_>>>()?;
    if commands.is_empty() {
        return Err(format!("`make -n {target}` runs no cargo command").into());
    }
    Ok(commands)
}

/// Checks every development target on one host: an assigned `RUSTFLAGS`
/// carries the frontend flag, carries mold exactly when the host and target
/// are Linux, and keeps an inherited `RUSTFLAGS`. Under an inherited
/// `RUSTFLAGS` every command must assign, because the caller's value displaces
/// the configuration's sources; setup-rust exports one in CI.
fn check_development_targets(host: Host, inherited: Option<&str>) -> Read<Vec<String>> {
    let mut problems = Vec::new();
    for target in DEVELOPMENT_TARGETS {
        let commands = make_rustflags(target, host, inherited)?;
        if inherited.is_some() && commands.iter().any(Option::is_none) {
            problems.push(format!(
                "`make {target}` runs a command that takes only the caller's RUSTFLAGS"
            ));
        }
        // An empty assignment is `Some(Flags(vec![]))` and is checked like any other.
        for flags in commands.into_iter().flatten() {
            if !flags.names(THREADS_FLAG) {
                problems.push(format!(
                    "`make {target}` on {host:?} drops {THREADS_FLAG}: {flags:?}"
                ));
            }
            if flags.names(MOLD_FLAG) != host.expects_mold() {
                problems.push(format!(
                    "`make {target}` on {host:?} gets mold wrong: {flags:?}"
                ));
            }
            if inherited.is_some_and(|caller| !flags.carries_run(caller)) {
                problems.push(format!(
                    "`make {target}` drops the caller's RUSTFLAGS: {flags:?}"
                ));
            }
        }
    }
    Ok(problems)
}

#[test]
fn every_rustflags_source_carries_the_parallel_frontend() {
    let found = sources().expect("read the configuration sources");
    assert!(
        found.iter().any(|(key, _)| key == "build"),
        "no [build] rustflags for non-Linux hosts"
    );
    let missing: Vec<&str> = found
        .iter()
        .filter(|(_, flags)| !flags.names(THREADS_FLAG))
        .map(|(key, _)| key.as_str())
        .collect();
    assert!(
        missing.is_empty(),
        "{THREADS_FLAG} missing from {missing:?}"
    );
}

#[test]
fn mold_is_confined_to_linux() {
    let found = sources().expect("read the configuration sources");
    let linux: Vec<_> = found
        .iter()
        .filter(|(key, _)| LINUX_TABLES.contains(&key.as_str()))
        .collect();
    assert!(!linux.is_empty(), "no Linux target table carries rustflags");
    assert!(
        linux.iter().all(|(_, flags)| flags.names(MOLD_FLAG)),
        "a Linux table lost mold"
    );
    let wider: Vec<&str> = found
        .iter()
        .filter(|(key, flags)| !LINUX_TABLES.contains(&key.as_str()) && flags.names(MOLD_FLAG))
        .map(|(key, _)| key.as_str())
        .collect();
    assert!(wider.is_empty(), "mold named beyond Linux in {wider:?}");
}

#[test]
fn sources_differ_only_by_the_linker() {
    let mut stripped: Vec<Vec<String>> = sources()
        .expect("read the configuration sources")
        .into_iter()
        .map(|(_, flags)| flags.without_mold())
        .collect();
    stripped.dedup();
    assert_eq!(
        stripped.len(),
        1,
        "rustflags sources disagree: {stripped:?}"
    );
}

#[test]
fn development_targets_restate_both_flags_on_linux() {
    let problems = check_development_targets(Host::Linux, None).expect("read `make -n` output");
    assert!(problems.is_empty(), "{problems:#?}");
    for target in ASSIGNING_TARGETS {
        let assigned = make_rustflags(target, Host::Linux, None)
            .expect("read `make -n` output")
            .into_iter()
            .flatten()
            .count();
        assert!(assigned > 0, "`make {target}` assigns no RUSTFLAGS");
    }
}

#[test]
fn development_targets_keep_the_standard_under_inherited_rustflags() {
    let problems =
        check_development_targets(Host::Linux, Some(INHERITED)).expect("read `make -n` output");
    assert!(problems.is_empty(), "{problems:#?}");
}

#[test]
fn development_targets_keep_the_frontend_but_not_mold_elsewhere() {
    let problems = check_development_targets(Host::Darwin, None).expect("read `make -n` output");
    assert!(problems.is_empty(), "{problems:#?}");
}

/// A Linux host building for another platform through `CARGO_BUILD_TARGET`
/// must not be handed mold, while a Linux target keeps it.
#[test]
fn development_targets_leave_mold_off_a_non_linux_target() {
    let mut problems =
        check_development_targets(Host::LinuxBuildingFor("aarch64-apple-darwin"), None)
            .expect("read `make -n` output");
    problems.extend(
        check_development_targets(Host::LinuxBuildingFor("aarch64-unknown-linux-gnu"), None)
            .expect("read `make -n` output"),
    );
    assert!(problems.is_empty(), "{problems:#?}");
}

/// Coverage measures and release ships, so both stay on the default flags.
/// Every command must assign `RUSTFLAGS`, since only an assignment displaces
/// the configuration's sources.
#[test]
fn coverage_and_release_take_neither_flag() {
    for target in HELD_OUT_TARGETS {
        for assigned in make_rustflags(target, Host::Linux, None).expect("read `make -n` output") {
            let flags = assigned.unwrap_or_else(|| {
                panic!("`make {target}` runs a command that takes the configuration's flags")
            });
            assert!(
                !flags.names(THREADS_FLAG),
                "`make {target}` takes {THREADS_FLAG}"
            );
            assert!(!flags.names(MOLD_FLAG), "`make {target}` takes {MOLD_FLAG}");
        }
    }
}
