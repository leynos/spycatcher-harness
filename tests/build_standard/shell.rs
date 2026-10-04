//! The shell-syntax half of the build-standard contract readers.
//!
//! It reads the lines `make -n` prints: splitting a line into the simple
//! commands it chains, outside quotes; finding which one runs cargo or
//! Whitaker; and parsing the assignments that lead a command. It models only
//! the syntax the gate recipes use, and fails rather than guess at anything
//! else.

use super::Read;

/// One line of `make -n` output, or one simple command cut from it.
#[derive(Clone, Copy)]
pub struct Line<'a>(pub &'a str);

/// One leading `NAME=value` assignment of a simple command.
pub struct Assigned<'a> {
    /// The variable name.
    pub name: &'a str,
    /// The value, without its surrounding quotes.
    pub value: &'a str,
    /// Whether the shell would expand the value: false only inside single quotes.
    pub expands: bool,
}

/// A value taken off the front of an assignment, with what follows it.
struct Taken<'a> {
    /// The value, without its surrounding quotes.
    value: &'a str,
    /// Whether the shell would expand it.
    expands: bool,
    /// The text after the value, which must start a new word.
    rest: &'a str,
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

impl<'a> Line<'a> {
    /// Returns the length of the command separator that starts the text, if
    /// any: `&&`, `||` or `;`.
    fn separator_len(self) -> Option<usize> {
        if self.0.starts_with("&&") || self.0.starts_with("||") {
            Some(2)
        } else {
            self.0.starts_with(';').then_some(1)
        }
    }

    /// Strips a leading shell keyword that introduces a command, not a command.
    fn without_keyword(self) -> &'a str {
        ["then ", "else ", "do ", "if "]
            .iter()
            .fold(self.0, |text, keyword| {
                text.strip_prefix(keyword).unwrap_or(text)
            })
            .trim()
    }

    /// Returns the `(offset, length)` of every command separator outside quotes.
    ///
    /// # Errors
    ///
    /// An unterminated quote means the reader cannot tell where a command ends,
    /// so it fails rather than guessing.
    fn split_points(self) -> Read<Vec<(usize, usize)>> {
        let mut scan = Scan::default();
        let mut points = Vec::new();
        let mut skip = 0;
        for (at, c) in self.0.char_indices() {
            if skip > 0 {
                skip -= 1;
                continue;
            }
            let separator = scan
                .is_unquoted(c)
                .then(|| self.0.get(at..).and_then(|rest| Line(rest).separator_len()))
                .flatten();
            if let Some(len) = separator {
                points.push((at, len));
                skip = len - 1;
            }
        }
        if scan.quote.is_some() {
            return Err(format!("unterminated quote in `{}`", self.0).into());
        }
        Ok(points)
    }

    /// Splits the line into the simple commands it chains.
    ///
    /// A line can hold several commands joined by `&&`, `||` or `;`, or an
    /// `if ... then ... else ... fi` block, and each takes its own `RUSTFLAGS`. A
    /// separator inside single or double quotes, or after a backslash, does not
    /// split.
    ///
    /// # Errors
    ///
    /// An unterminated quote fails the split.
    pub fn commands(self) -> Read<Vec<&'a str>> {
        let mut parts = Vec::new();
        let mut start = 0;
        for (at, len) in self.split_points()? {
            parts.push(self.0.get(start..at).unwrap_or_default());
            start = at + len;
        }
        parts.push(self.0.get(start..).unwrap_or_default());
        Ok(parts
            .into_iter()
            .map(|part| Line(part.trim()).without_keyword())
            .filter(|part| !part.is_empty())
            .collect())
    }

    /// Returns the offset of the first double quote that no backslash escapes.
    fn closing_quote(self) -> Option<usize> {
        let mut escaped = false;
        for (at, c) in self.0.char_indices() {
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

    /// Checks that a taken value ends its word, and returns it.
    fn ended(self, value: &'a str, expands: bool, rest: &'a str) -> Read<Taken<'a>> {
        if rest.chars().next().is_some_and(|c| !c.is_whitespace()) {
            return Err(format!("text adjoins a closing quote in `{}`", self.0).into());
        }
        Ok(Taken {
            value,
            expands,
            rest,
        })
    }

    /// Takes a double-quoted value; the text starts after the opening quote.
    fn take_double(self) -> Read<Taken<'a>> {
        let at = self
            .closing_quote()
            .ok_or_else(|| format!("unterminated quote in `{}`", self.0))?;
        let (value, rest) = (self.0.get(..at), self.0.get(at + 1..));
        self.ended(value.unwrap_or_default(), true, rest.unwrap_or_default())
    }

    /// Takes a single-quoted value, which the shell does not expand; the text
    /// starts after the opening quote.
    fn take_single(self) -> Read<Taken<'a>> {
        let at = self
            .0
            .find('\'')
            .ok_or_else(|| format!("unterminated quote in `{}`", self.0))?;
        let (value, rest) = (self.0.get(..at), self.0.get(at + 1..));
        self.ended(value.unwrap_or_default(), false, rest.unwrap_or_default())
    }

    /// Takes a bare word, which the shell expands.
    fn take_word(self) -> Taken<'a> {
        let (value, rest) = self
            .0
            .split_once(char::is_whitespace)
            .unwrap_or((self.0, ""));
        Taken {
            value,
            expands: true,
            rest,
        }
    }

    /// Takes one assigned value off the front: a double-quoted value, a
    /// single-quoted one, or a bare word.
    fn take_value(self) -> Read<Taken<'a>> {
        let after_quote = Self(self.0.get(1..).unwrap_or_default());
        match self.0.chars().next() {
            Some('"') => after_quote.take_double(),
            Some('\'') => after_quote.take_single(),
            _ => Ok(self.take_word()),
        }
    }

    /// Splits the leading `NAME=value` assignments off a simple command,
    /// returning them and the rest, which starts at the command word. An
    /// assignment later in the command, such as one after `--`, is an argument
    /// and is not read.
    ///
    /// # Errors
    ///
    /// An unterminated quote, or text stuck to a closing quote, means the reader
    /// cannot tell where the value ends.
    pub fn assignments(self) -> Read<(Vec<Assigned<'a>>, Self)> {
        let mut found = Vec::new();
        let mut rest = self.0.trim_start();
        while let Some((name, value)) = rest.split_once('=') {
            let is_name =
                !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
            if !is_name {
                break;
            }
            let taken = Line(value).take_value()?;
            found.push(Assigned {
                name,
                value: taken.value,
                expands: taken.expands,
            });
            rest = taken.rest.trim_start();
        }
        Ok((found, Self(rest)))
    }

    /// Returns whether the command runs cargo or Whitaker, as opposed to naming
    /// one, as `command -v whitaker` or an `echo` of its path does.
    ///
    /// # Errors
    ///
    /// An unreadable leading assignment fails rather than hiding the command.
    pub fn runs_cargo_or_whitaker(self) -> Read<bool> {
        let (_, Line(rest)) = self.assignments()?;
        let word = rest.split_whitespace().next().unwrap_or_default();
        let name = word.rsplit('/').next().unwrap_or(word);
        Ok(matches!(name, "cargo" | "whitaker"))
    }
}
