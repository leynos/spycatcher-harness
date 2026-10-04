//! The shell-syntax half of the build-standard contract readers.
//!
//! It reads the lines `make -n` prints: splitting a line into the simple
//! commands it chains, outside quotes; finding which one runs cargo or
//! Whitaker; and locating where a quoted assignment ends. It models only the
//! syntax the gate recipes use, and fails rather than guess at anything else.

use super::Read;

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

/// Where a scan of a recipe line stands: the quote it is inside, if any, and
/// whether the previous character was a backslash.
#[derive(Default)]
struct Scan {
    /// The open quote character, if the scan is inside one.
    quote: Option<char>,
    /// Whether the previous character escaped this one.
    escaped: bool,
}

impl Scan {
    /// Advances over one character and reports whether it sits outside any
    /// quote or escape, so that a separator there counts.
    fn is_unquoted(&mut self, c: char) -> bool {
        if self.escaped {
            self.escaped = false;
            return false;
        }
        if c == '\\' && self.quote != Some('\'') {
            self.escaped = true;
            return false;
        }
        match (self.quote, c) {
            (Some(open), _) if open == c => {
                self.quote = None;
                false
            }
            (Some(_), _) => false,
            (None, '"' | '\'') => {
                self.quote = Some(c);
                false
            }
            (None, _) => true,
        }
    }
}

/// Returns the `(offset, length)` of every command separator outside quotes.
///
/// # Errors
///
/// An unterminated quote means the reader cannot tell where a command ends, so
/// it fails rather than guessing.
fn split_points(line: &str) -> Read<Vec<(usize, usize)>> {
    let mut scan = Scan::default();
    let mut points = Vec::new();
    let mut skip = 0;
    for (at, c) in line.char_indices() {
        if skip > 0 {
            skip -= 1;
            continue;
        }
        let separator = scan
            .is_unquoted(c)
            .then(|| line.get(at..).and_then(separator_len))
            .flatten();
        if let Some(len) = separator {
            points.push((at, len));
            skip = len - 1;
        }
    }
    if scan.quote.is_some() {
        return Err(format!("unterminated quote in `{line}`").into());
    }
    Ok(points)
}

/// Splits one recipe line into the simple commands it chains.
///
/// A line can hold several commands joined by `&&`, `||` or `;`, or an
/// `if ... then ... else ... fi` block, and each takes its own `RUSTFLAGS`. A
/// separator inside single or double quotes, or after a backslash, does not
/// split.
pub fn commands(line: &str) -> Read<Vec<&str>> {
    let mut parts = Vec::new();
    let mut start = 0;
    for (at, len) in split_points(line)? {
        parts.push(line.get(start..at).unwrap_or_default());
        start = at + len;
    }
    parts.push(line.get(start..).unwrap_or_default());
    Ok(parts
        .into_iter()
        .map(str::trim)
        .map(without_keyword)
        .filter(|part| !part.is_empty())
        .collect())
}

/// Returns the offset of the first double quote in `text` that no backslash
/// escapes.
pub fn closing_quote(text: &str) -> Option<usize> {
    let mut escaped = false;
    for (at, c) in text.char_indices() {
        if escaped {
            escaped = false;
        } else if c == '\\' {
            escaped = true;
        } else if c == '"' {
            return Some(at);
        }
    }
    None
}

/// One leading `NAME=value` assignment of a simple command.
pub struct Assigned<'a> {
    /// The variable name.
    pub name: &'a str,
    /// The value, without its surrounding quotes.
    pub value: &'a str,
    /// Whether the shell would expand the value: false only inside single quotes.
    pub expands: bool,
}

/// Returns whether `text` is a shell variable name.
fn is_name(text: &str) -> bool {
    !text.is_empty() && text.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Takes a double-quoted value off the front of `quoted`, which follows the
/// opening quote: the value, that it expands, and the text after the closing quote.
fn take_double(quoted: &str) -> Option<(&str, bool, &str)> {
    let at = closing_quote(quoted)?;
    Some((quoted.get(..at)?, true, quoted.get(at + 1..)?))
}

/// Takes a single-quoted value off the front of `quoted`, which follows the
/// opening quote. The shell does not expand it.
fn take_single(quoted: &str) -> Option<(&str, bool, &str)> {
    let at = quoted.find('\'')?;
    Some((quoted.get(..at)?, false, quoted.get(at + 1..)?))
}

/// Takes a bare word off the front of `value`, which the shell expands.
fn take_word(value: &str) -> (&str, bool, &str) {
    value
        .split_once(char::is_whitespace)
        .map_or((value, true, ""), |(word, rest)| (word, true, rest))
}

/// Takes one assigned value off the front of `value`: a double-quoted value, a
/// single-quoted one, or a bare word. Returns it, whether it expands, and the
/// text after it.
fn take_value(value: &str) -> Option<(&str, bool, &str)> {
    match value.chars().next() {
        Some('"') => take_double(value.get(1..)?),
        Some('\'') => take_single(value.get(1..)?),
        _ => Some(take_word(value)),
    }
}

/// Splits the leading `NAME=value` assignments off a simple command, returning
/// them and the rest, which starts at the command word. An assignment later in
/// the command, such as one after `--`, is an argument and is not read.
///
/// # Errors
///
/// An unterminated quote means the reader cannot tell where the value ends.
pub fn leading_assignments(command: &str) -> Read<(Vec<Assigned<'_>>, &str)> {
    let mut found = Vec::new();
    let mut rest = command.trim_start();
    while let Some((name, value)) = rest.split_once('=') {
        if !is_name(name) {
            break;
        }
        let (text, expands, tail) = take_value(value).ok_or_else(|| {
            format!("unterminated quote in the assignment of `{name}` in `{command}`")
        })?;
        found.push(Assigned {
            name,
            value: text,
            expands,
        });
        rest = tail.trim_start();
    }
    Ok((found, rest))
}

/// Returns whether a command runs cargo or Whitaker, as opposed to naming one,
/// as `command -v whitaker` or an `echo` of its path does.
///
/// # Errors
///
/// An unreadable leading assignment fails rather than hiding the command.
pub fn runs_cargo_or_whitaker(command: &str) -> Read<bool> {
    let (_, rest) = leading_assignments(command)?;
    let word = rest.split_whitespace().next().unwrap_or_default();
    let name = word.rsplit('/').next().unwrap_or(word);
    Ok(matches!(name, "cargo" | "whitaker"))
}
