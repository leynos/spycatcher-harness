//! Source-injected configuration fixtures shared by CLI layering suites.

use std::io::Write as _;
use std::path::Path;
use std::sync::Arc;

use cap_std::{
    ambient_authority,
    fs_utf8::{Dir, camino::Utf8Path},
};
use eyre::{Result, WrapErr as _, eyre};
use ortho_config::{
    MapEnv, NoOpLocalizer, SharedEnvSource, SharedScanEnvSource, SubcommandFileContext,
};

use spycatcher_harness::HarnessConfig;
use spycatcher_harness::cli::load_subcommand_config_from_iter_with_sources;

/// Loads a [`HarnessConfig`] from explicit file and environment fixtures.
///
/// Each call gets a new directory and a closed in-memory environment source,
/// so parallel tests cannot observe one another's configuration.
pub fn load_with_sources(
    argv: &[&str],
    config_file: Option<&str>,
    env_vars: &[(&str, &str)],
) -> Result<HarnessConfig> {
    let temp_dir = tempfile::tempdir().wrap_err("create configuration fixture directory")?;
    if let Some(content) = config_file {
        let directory = open_fixture_directory(temp_dir.path())?;
        let mut file = directory
            .create(".spycatcher_harness.toml")
            .wrap_err("create configuration fixture file")?;
        file.write_all(content.as_bytes())
            .wrap_err("write configuration fixture file")?;
    }

    load_from_base(argv, temp_dir.path(), env_vars)
}

/// Runs the application loader against the supplied base and closed sources.
pub fn load_from_base(
    argv: &[&str],
    base: &Path,
    env_vars: &[(&str, &str)],
) -> Result<HarnessConfig> {
    let xdg_base = base
        .to_str()
        .ok_or_else(|| eyre!("configuration fixture path is not UTF-8"))?;
    let source = Arc::new(
        env_vars.iter().fold(
            MapEnv::new()
                .with_var("XDG_CONFIG_HOME", xdg_base)
                .with_var("XDG_CONFIG_DIRS", xdg_base),
            |source, (key, value)| source.with_var(*key, *value),
        ),
    );
    let discovery: SharedEnvSource = source.clone();
    let merge: SharedScanEnvSource = source;
    let files = SubcommandFileContext::new(base, discovery.as_ref());
    let localizer = NoOpLocalizer::new();

    Ok(load_subcommand_config_from_iter_with_sources(
        argv, &localizer, files, merge,
    )?)
}

/// Opens a `cap_std` directory after validating the temporary path as UTF-8.
fn open_fixture_directory(system_path: &Path) -> Result<Dir> {
    let utf8_path =
        Utf8Path::from_path(system_path).ok_or_else(|| eyre!("fixture path is not UTF-8"))?;
    Dir::open_ambient_dir(utf8_path, ambient_authority())
        .wrap_err("open configuration fixture directory")
}
