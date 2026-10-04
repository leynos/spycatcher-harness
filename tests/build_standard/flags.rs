//! The Cargo-configuration half of the build-standard contract readers.
//!
//! It holds the flag constants, the normalised flag list, and the reader for the
//! `rustflags` sources in `.cargo/config.toml`. File access goes through a
//! `cap_std` directory handle rooted at the crate manifest directory.

use std::error::Error;

use cap_std::{ambient_authority, fs::Dir};

/// The parallel-frontend flag every `rustflags` source must carry.
pub const THREADS_FLAG: &str = "-Zthreads=8";

/// The linker flag the Linux source must add, normalized to one token.
pub const MOLD_FLAG: &str = "-Clink-arg=-fuse-ld=mold";

/// Target table keys that apply on Linux alone.
pub const LINUX_TABLES: [&str; 2] = ["x86_64-unknown-linux-gnu", "cfg(target_os = \"linux\")"];

/// The table that gives every Linux architecture mold. A table keyed on one
/// triple would leave the other Linux targets without it.
pub const LINUX_SELECTOR: &str = "cfg(target_os = \"linux\")";

/// The result of a reader, which the tests unwrap.
pub type Read<T> = Result<T, Box<dyn Error>>;

/// One `rustflags` list, with `-C value` pairs joined into `-Cvalue` so both
/// spellings compare equal.
#[derive(Debug)]
pub struct Flags(Vec<String>);

impl Flags {
    /// Normalizes a word list into flags.
    pub(super) fn from_words<S: AsRef<str>>(words: &[S]) -> Self {
        let mut joined: Vec<String> = Vec::new();
        for word in words.iter().map(AsRef::as_ref) {
            match joined.last_mut() {
                Some(last) if last == "-C" => *last = format!("-C{word}"),
                _ => joined.push(word.to_owned()),
            }
        }
        Self(joined)
    }

    /// Returns whether the list is exactly the caller's words, in order and
    /// with nothing added.
    pub fn is_exactly(&self, caller: &str) -> bool {
        self.0 == Self::from_words(&caller.split_whitespace().collect::<Vec<_>>()).0
    }

    /// Returns whether the list names one flag.
    pub fn names(&self, flag: &str) -> bool {
        self.0.iter().any(|candidate| candidate == flag)
    }

    /// Returns whether the list holds the caller's words as one unbroken run.
    pub fn carries_run(&self, caller: &str) -> bool {
        // The caller's words are normalised like the stored ones, so `-C x` and
        // `-Cx` compare equal.
        let wanted = Self::from_words(&caller.split_whitespace().collect::<Vec<_>>()).0;
        // An empty caller value is carried by any list; `windows(0)` would panic.
        if wanted.is_empty() {
            return true;
        }
        self.0
            .windows(wanted.len())
            .any(|run| run == wanted.as_slice())
    }

    /// Returns the list without the linker flag, for comparing sources.
    pub fn without_mold(self) -> Vec<String> {
        self.0
            .into_iter()
            .filter(|flag| flag != MOLD_FLAG)
            .collect()
    }
}

/// The Cargo configuration the readers parse, named in their errors.
const CONFIG: &str = ".cargo/config.toml";

/// Reads one table's `rustflags`, which Cargo accepts as an array of strings or
/// as one whitespace-separated string.
///
/// # Errors
///
/// Any other shape, or an array member that is not a string, would be skipped
/// by a lenient reader and so hide a flag from every check, so it is an error.
pub fn table_flags(key: &str, table: &toml::Value) -> Read<Option<Flags>> {
    let Some(raw) = table.get("rustflags") else {
        return Ok(None);
    };
    let words: Vec<String> = if let Some(text) = raw.as_str() {
        text.split_whitespace().map(str::to_owned).collect()
    } else if let Some(items) = raw.as_array() {
        items
            .iter()
            .map(|item| {
                item.as_str().map(str::to_owned).ok_or_else(|| {
                    format!("{CONFIG}: `[{key}] rustflags` has a non-string member {item}")
                })
            })
            .collect::<Result<_, _>>()?
    } else {
        return Err(format!(
            "{CONFIG}: `[{key}] rustflags` must be a string or an array, found {raw}"
        )
        .into());
    };
    Ok(Some(Flags::from_words(&words)))
}

/// Reads a file relative to the crate manifest directory.
///
/// # Errors
///
/// A failure to open the directory or read the file names the operation and the
/// path.
pub fn read(path: &str) -> Read<String> {
    let root = Dir::open_ambient_dir(env!("CARGO_MANIFEST_DIR"), ambient_authority())
        .map_err(|error| format!("opening the crate directory to read `{path}`: {error}"))?;
    Ok(root
        .read_to_string(path)
        .map_err(|error| format!("reading `{path}`: {error}"))?)
}

/// Parses the Cargo configuration text.
///
/// # Errors
///
/// A parse failure names the file.
pub fn parse(text: &str) -> Read<toml::Value> {
    Ok(toml::from_str(text).map_err(|error| format!("parsing {CONFIG}: {error}"))?)
}

/// Returns every `rustflags` source in the configuration, by table name.
pub fn sources() -> Read<Vec<(String, Flags)>> {
    let config = parse(&read(CONFIG)?)?;
    let mut found = Vec::new();
    if let Some(flags) = config
        .get("build")
        .map(|table| table_flags("build", table))
        .transpose()?
        .flatten()
    {
        found.push(("build".to_owned(), flags));
    }
    if let Some(targets) = config.get("target").and_then(toml::Value::as_table) {
        for (key, table) in targets {
            if let Some(flags) = table_flags(key, table)? {
                found.push((key.clone(), flags));
            }
        }
    }
    Ok(found)
}
