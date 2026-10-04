//! The Cargo-configuration half of the build-standard contract readers.
//!
//! It holds the flag constants, the normalised flag list, and the reader for the
//! `rustflags` sources in `.cargo/config.toml`. File access goes through a
//! `cap_std` directory handle rooted at the crate manifest directory.

use std::{error::Error, path::Path};

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

    /// Parses whitespace-separated flags, as a caller exports them.
    pub fn from_text(text: &str) -> Self {
        Self::from_words(&text.split_whitespace().collect::<Vec<_>>())
    }

    /// Returns whether the list is exactly `caller`'s flags, in order and with
    /// nothing added.
    pub fn equals(&self, caller: &Self) -> bool {
        self.0 == caller.0
    }

    /// Returns whether the list names one flag.
    pub fn names(&self, flag: &str) -> bool {
        self.0.iter().any(|candidate| candidate == flag)
    }

    /// Returns whether the list holds `caller`'s flags as one unbroken run.
    pub fn carries(&self, caller: &Self) -> bool {
        // An empty caller value is carried by any list; `windows(0)` would panic.
        if caller.0.is_empty() {
            return true;
        }
        self.0
            .windows(caller.0.len())
            .any(|run| run == caller.0.as_slice())
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

/// One table of the Cargo configuration, named by its key, whose `rustflags`
/// the readers read.
pub struct Source<'a> {
    /// The table's key, named in errors.
    pub key: &'a str,
    /// The table itself.
    pub table: &'a toml::Value,
}

impl Source<'_> {
    /// Reads the table's `rustflags`, which Cargo accepts as an array of strings
    /// or as one whitespace-separated string.
    ///
    /// # Errors
    ///
    /// Any other shape, or an array member that is not a string, would be
    /// skipped by a lenient reader and so hide a flag from every check, so it is
    /// an error naming the file and the table.
    pub fn flags(&self) -> Read<Option<Flags>> {
        let Some(raw) = self.table.get("rustflags") else {
            return Ok(None);
        };
        let words = match (raw.as_str(), raw.as_array()) {
            (Some(text), _) => text.split_whitespace().map(str::to_owned).collect(),
            (None, Some(items)) => self.array_words(items)?,
            (None, None) => {
                return Err(format!(
                    "{CONFIG}: `[{}] rustflags` must be a string or an array, found {raw}",
                    self.key
                )
                .into());
            }
        };
        Ok(Some(Flags::from_words(&words)))
    }

    /// Reads the members of an array-form `rustflags`, which must all be strings.
    fn array_words(&self, items: &[toml::Value]) -> Read<Vec<String>> {
        items
            .iter()
            .map(|item| {
                item.as_str().map(str::to_owned).ok_or_else(|| {
                    format!(
                        "{CONFIG}: `[{}] rustflags` has a non-string member {item}",
                        self.key
                    )
                    .into()
                })
            })
            .collect()
    }
}

/// The text of the Cargo configuration, before it is parsed.
pub struct ConfigText<'a>(pub &'a str);

impl ConfigText<'_> {
    /// Parses the configuration text.
    ///
    /// # Errors
    ///
    /// A parse failure names the file.
    pub fn parse(&self) -> Read<toml::Value> {
        Ok(toml::from_str(self.0).map_err(|error| format!("parsing {CONFIG}: {error}"))?)
    }
}

/// Reads a file relative to the crate manifest directory.
///
/// # Errors
///
/// A failure to open the directory or read the file names the operation and the
/// path.
pub fn read(path: &Path) -> Read<String> {
    let root = Dir::open_ambient_dir(env!("CARGO_MANIFEST_DIR"), ambient_authority()).map_err(
        |error| {
            format!(
                "opening the crate directory to read `{}`: {error}",
                path.display()
            )
        },
    )?;
    Ok(root
        .read_to_string(path)
        .map_err(|error| format!("reading `{}`: {error}", path.display()))?)
}

/// Reads and parses the Cargo configuration.
///
/// # Errors
///
/// A read or parse failure names the file.
pub fn config() -> Read<toml::Value> {
    ConfigText(&read(Path::new(CONFIG))?).parse()
}

/// Returns every `rustflags` source in the configuration, by table name.
pub fn sources() -> Read<Vec<(String, Flags)>> {
    let config = config()?;
    let mut found = Vec::new();
    if let Some(table) = config.get("build") {
        let source = Source {
            key: "build",
            table,
        };
        if let Some(flags) = source.flags()? {
            found.push(("build".to_owned(), flags));
        }
    }
    if let Some(targets) = config.get("target").and_then(toml::Value::as_table) {
        for (key, table) in targets {
            if let Some(flags) = (Source { key, table }).flags()? {
                found.push((key.clone(), flags));
            }
        }
    }
    Ok(found)
}
