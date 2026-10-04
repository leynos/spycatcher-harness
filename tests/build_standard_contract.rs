//! Contract tests for the Rust build standard.
//!
//! The standard makes the parallel `rustc` frontend the default for every
//! development build and mold the default linker on Linux. Cargo reads both
//! from `.cargo/config.toml`, but it applies a single `rustflags` source rather
//! than merging them, and an assigned `RUSTFLAGS` replaces every source. So the
//! flags must be repeated in each configuration source, restated wherever the
//! Makefile assigns `RUSTFLAGS` for a development target, and kept out of the
//! release recipe, which ships and so stays on the default flags.
//!
//! The Makefile clauses run `make -n` and read the commands it would run,
//! rather than the Makefile's text, so a flag lost through a variable or a
//! recipe edit fails here. Each assigned value is expanded by a small model of
//! the forms the recipes use, not by a shell, with and without an inherited
//! `RUSTFLAGS`, and a line chaining several commands is read command by
//! command. The readers live in `build_standard/support.rs`; the expectations
//! below state, per host and target, whether mold applies, independently of the
//! Makefile's own rule.

#[path = "build_standard/support.rs"]
mod support;

use rstest::rstest;
use support::{
    Host, LINUX_SELECTOR, LINUX_TABLES, MOLD_FLAG, THREADS_FLAG, check_development_targets,
    make_rustflags, sources,
};

/// Makefile targets that build for development. A command in one either
/// assigns `RUSTFLAGS` with the standard flags or assigns none and so takes
/// the configuration's.
const DEVELOPMENT_TARGETS: [&str; 4] = ["test", "typecheck", "lint", "build"];

/// Makefile targets that ship, so every command assigns `RUSTFLAGS` and none
/// carries a standard flag. Coverage runs in CI, outwith the Makefile.
const HELD_OUT_TARGETS: [&str; 1] = ["release"];

/// Development targets that must assign `RUSTFLAGS` in at least one command,
/// so the restatement checks above cannot pass by finding nothing to check.
const ASSIGNING_TARGETS: [&str; 2] = ["test", "build"];

/// A caller's own flags, distinct from anything a recipe adds, to prove a
/// recipe composes an exported `RUSTFLAGS` with the standard flags rather than
/// replacing either.
const INHERITED: &str = "--cfg inherited_from_caller";

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
        linux.iter().any(|(key, _)| key == LINUX_SELECTOR),
        "mold must sit under `{LINUX_SELECTOR}` so every Linux architecture gets it"
    );
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

/// The flags every development target assigns, per host, target and caller.
///
/// Mold applies exactly when a Linux host builds for a Linux target: Cargo
/// matches `cfg(target_os = "linux")` against the compilation target, so a
/// macOS host, a foreign target and an Android triple (whose `target_os` is
/// `android`, though it contains `-linux-`) must not be handed it. Cargo
/// resolves `host-tuple` to the host's own triple.
#[rstest]
#[case::linux(Host::Linux, None, true)]
#[case::linux_with_a_caller(Host::Linux, Some(INHERITED), true)]
#[case::linux_with_an_empty_caller(Host::Linux, Some(""), true)]
#[case::linux_with_a_blank_caller(Host::Linux, Some("   "), true)]
#[case::linux_with_a_split_codegen_flag(Host::Linux, Some("-C debuginfo=1"), true)]
#[case::macos(Host::Darwin, None, false)]
#[case::linux_to_macos(Host::LinuxBuildingFor("aarch64-apple-darwin"), None, false)]
#[case::linux_to_windows(Host::LinuxBuildingFor("x86_64-pc-windows-msvc"), None, false)]
#[case::linux_to_android(Host::LinuxBuildingFor("aarch64-linux-android"), None, false)]
#[case::linux_to_androideabi(Host::LinuxBuildingFor("armv7-linux-androideabi"), None, false)]
#[case::linux_to_linux(Host::LinuxBuildingFor("aarch64-unknown-linux-gnu"), None, true)]
#[case::linux_to_host_tuple(Host::LinuxBuildingFor("host-tuple"), None, true)]
fn development_targets_route_the_flags(
    #[case] host: Host,
    #[case] inherited: Option<&str>,
    #[case] expects_mold: bool,
) {
    let problems = check_development_targets(&DEVELOPMENT_TARGETS, host, inherited, expects_mold)
        .expect("read `make -n` output");
    assert!(problems.is_empty(), "{problems:#?}");
}

#[test]
fn the_assigning_targets_assign_rustflags() {
    for target in ASSIGNING_TARGETS {
        let assigned = make_rustflags(target, Host::Linux, None)
            .expect("read `make -n` output")
            .into_iter()
            .flatten()
            .count();
        assert!(assigned > 0, "`make {target}` assigns no RUSTFLAGS");
    }
}

/// Release ships, so it adds neither standard flag. Every command must assign
/// `RUSTFLAGS`, since only an assignment displaces the configuration's
/// sources. It forwards the caller's own value untouched, even one that names a
/// standard flag, and an empty one when the caller exports none. Coverage runs
/// in CI, outwith the Makefile, and is not checked here.
#[rstest]
#[case::no_caller(None)]
#[case::with_a_caller(Some(INHERITED))]
#[case::a_caller_naming_a_standard_flag(Some("-Zthreads=8"))]
fn release_adds_no_standard_flag(#[case] inherited: Option<&str>) {
    for target in HELD_OUT_TARGETS {
        for assigned in
            make_rustflags(target, Host::Linux, inherited).expect("read `make -n` output")
        {
            let flags = assigned.unwrap_or_else(|| {
                panic!("`make {target}` runs a command that takes the configuration's flags")
            });
            assert!(
                flags.is_exactly(inherited.unwrap_or_default()),
                "`make {target}` assigns {flags:?}, not the caller's {inherited:?}"
            );
        }
    }
}
