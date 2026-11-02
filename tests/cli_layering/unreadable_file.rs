//! Unix-only helper for preserving unreadable-file error coverage.

use cap_std::{
    ambient_authority,
    fs_utf8::{Dir, Permissions, PermissionsExt, camino::Utf8Path},
};
use eyre::{Result, WrapErr as _, eyre};
use spycatcher_harness::HarnessConfig;

/// Returns the loader result when the runner enforces mode-000 permissions.
///
/// `None` means the process can still open the file, so this runner cannot
/// exercise the permission-specific discovery failure.
pub(super) fn load_with_unreadable_file(argv: &[&str]) -> Result<Option<Result<HarnessConfig>>> {
    let temp_dir = tempfile::tempdir().wrap_err("create configuration fixture directory")?;
    let utf8_path =
        Utf8Path::from_path(temp_dir.path()).ok_or_else(|| eyre!("fixture path is not UTF-8"))?;
    let directory = Dir::open_ambient_dir(utf8_path, ambient_authority())
        .wrap_err("open configuration fixture directory")?;
    let file = directory
        .create(".spycatcher_harness.toml")
        .wrap_err("create configuration fixture file")?;
    file.set_permissions(Permissions::from_mode(0o000))
        .wrap_err("make configuration fixture unreadable")?;

    if directory.open(".spycatcher_harness.toml").is_ok() {
        Ok(None)
    } else {
        Ok(Some(super::support::load_from_base(
            argv,
            temp_dir.path(),
            &[],
        )))
    }
}
