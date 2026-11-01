//! Isolated executable fixtures whose writable descriptors stay out of the test process.

use std::{
    io,
    process::{Command, Output},
};

use camino::{Utf8Path, Utf8PathBuf};

/// Owns an executable and its unique scratch directory until execution finishes.
pub(super) struct FakeProgram {
    /// Executable path passed to the stand-in process.
    path: Utf8PathBuf,
    /// Keeps the unique scratch directory alive while the stand-in executes.
    _directory: tempfile::TempDir,
}

impl FakeProgram {
    /// Returns the executable path; a fixture can pass `path()` to `Command::new`.
    pub(super) fn path(&self) -> &Utf8Path { &self.path }
}

/// Creates an isolated executable without opening it for writing in this process.
/// For example, `create_fake_program("failure", "#!/bin/sh\nexit 4\n")`
/// returns an owned fixture whose executable exits with status 4.
pub(super) fn create_fake_program(label: &str, script: &str) -> io::Result<FakeProgram> {
    let directory = tempfile::Builder::new()
        .prefix(&format!("{label}-"))
        .tempdir_in(env!("CARGO_TARGET_TMPDIR"))
        .map_err(|error| fixture_error("create the scratch directory", label, &error))?;
    let path = Utf8PathBuf::from_path_buf(directory.path().join("make")).map_err(|path| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "executable path for {label} is not UTF-8: {}",
                path.display()
            ),
        )
    })?;
    // A sibling test can fork while this helper runs. Keeping the script's
    // writable descriptor in a separate child prevents that fork inheriting it.
    // Waiting for the writer also ensures it has closed the file before use.
    let output = Command::new("/bin/sh")
        .args([
            "-c",
            "umask 077; printf '%s' \"$2\" > \"$1\" && chmod 755 \"$1\"",
            "fake-program-writer",
        ])
        .arg(path.as_str())
        .arg(script)
        .output()
        .map_err(|error| fixture_error("run the script writer", label, &error))?;
    require_writer_success(&output, label)?;
    Ok(FakeProgram {
        path,
        _directory: directory,
    })
}

/// Adds operation context while preserving the original I/O error kind.
/// For example, a denied write remains `PermissionDenied` with the fixture label.
fn fixture_error(operation: &str, label: &str, error: &io::Error) -> io::Error {
    io::Error::new(error.kind(), format!("{operation} for {label}: {error}"))
}

/// Reports a writer's unsuccessful status and stderr as a setup error.
/// For example, a failed `chmod` produces an error identifying the writer status.
fn require_writer_success(output: &Output, label: &str) -> io::Result<()> {
    if output.status.success() {
        Ok(())
    } else {
        Err(io::Error::other(format!(
            "write executable for {label}: {}; stderr: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr),
        )))
    }
}

/// Same-label fixtures remain independent while both child executions are pending.
#[test]
fn same_label_programs_preserve_distinct_paths_and_outputs() -> io::Result<()> {
    let first = create_fake_program(
        "concurrent-failure",
        "#!/bin/sh\nprintf 'FIRST-OUT\\n'\nprintf 'FIRST-ERR\\n' >&2\nexit 4\n",
    )?;
    let second = create_fake_program(
        "concurrent-failure",
        "#!/bin/sh\nprintf 'SECOND-OUT\\n'\nprintf 'SECOND-ERR\\n' >&2\nexit 7\n",
    )?;
    let first_child = spawn_program(&first)?;
    let second_child = match spawn_program(&second) {
        Ok(child) => child,
        Err(error) => {
            first_child.wait_with_output()?;
            return Err(error);
        }
    };
    let first_result = first_child.wait_with_output();
    let second_result = second_child.wait_with_output();
    let first_output = first_result?;
    let second_output = second_result?;
    assert_distinct_program_results(&first, &second, &first_output, &second_output);
    Ok(())
}

/// Starts a stand-in with captured streams; callers wait before dropping its guard.
/// For example, an exit-4 fixture produces a child whose captured status is 4.
fn spawn_program(program: &FakeProgram) -> io::Result<std::process::Child> {
    Command::new(program.path())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
}

/// Checks independence and externally observed output without performing setup.
#[track_caller]
fn assert_distinct_program_results(
    first: &FakeProgram,
    second: &FakeProgram,
    first_output: &Output,
    second_output: &Output,
) {
    assert_ne!(
        first.path(),
        second.path(),
        "same-label fixtures need unique paths"
    );
    assert_eq!(
        first_output.status.code(),
        Some(4),
        "first script should exit 4"
    );
    assert_eq!(
        first_output.stdout, b"FIRST-OUT\n",
        "first stdout should stay distinct"
    );
    assert_eq!(
        first_output.stderr, b"FIRST-ERR\n",
        "first stderr should stay distinct"
    );
    assert_eq!(
        second_output.status.code(),
        Some(7),
        "second script should exit 7"
    );
    assert_eq!(
        second_output.stdout, b"SECOND-OUT\n",
        "second stdout should stay distinct"
    );
    assert_eq!(
        second_output.stderr, b"SECOND-ERR\n",
        "second stderr should stay distinct"
    );
}
