//! A reader for the lines of a GitHub Actions workflow job.
//!
//! It models only what the CI-order contract needs: finding the job that holds a
//! step, the executable `run` commands of that job, whether a step is
//! conditional, and where mold is installed. It judges this repository's own
//! workflow forms and fails closed: a form it does not recognise is a named
//! error, not a guess. Text that is not a runnable command
//! (a comment, a name, a description) is never evidence.

/// One line of workflow text.
#[derive(Clone, Copy)]
struct Text<'a>(&'a str);

impl<'a> Text<'a> {
    /// Returns the number of leading spaces.
    fn indent(self) -> usize {
        self.0.len() - self.0.trim_start().len()
    }

    /// Returns whether the line is blank or a comment, and so evidence of nothing.
    fn is_inert(self) -> bool {
        let trimmed = self.0.trim_start();
        trimmed.is_empty() || trimmed.starts_with('#')
    }

    /// Returns whether the line begins a YAML sequence item, a workflow step.
    fn starts_step(self) -> bool {
        self.0.trim_start().starts_with("- ")
    }

    /// Returns the line without its indent or a leading `- `.
    fn key_text(self) -> &'a str {
        self.0.trim().trim_start_matches("- ")
    }

    /// Returns whether the line, as a one-line command, runs `apt` or `apt-get`
    /// to install mold: the command itself, not an `echo` of one.
    fn installs_mold(self) -> bool {
        let words: Vec<&str> = self
            .0
            .split_whitespace()
            .skip_while(|word| *word == "sudo")
            .collect();
        words
            .first()
            .is_some_and(|first| matches!(*first, "apt" | "apt-get"))
            && words.contains(&"install")
            && words.contains(&"mold")
    }

    /// Splits a shell command line at every `&&` that sits outside quotes.
    fn chain(self) -> Vec<Self> {
        let mut parts = Vec::new();
        let (mut start, mut quote) = (0, None);
        let bytes = self.0.as_bytes();
        for (at, c) in self.0.char_indices() {
            match (quote, c) {
                (Some(open), _) if open == c => quote = None,
                (None, '"' | '\'') => quote = Some(c),
                (None, '&') if bytes.get(at + 1) == Some(&b'&') && at >= start => {
                    parts.push(Self(self.0.get(start..at).unwrap_or_default()));
                    start = at + 2;
                }
                _ => {}
            }
        }
        parts.push(Self(self.0.get(start..).unwrap_or_default()));
        parts
    }
}

/// The result of a read that fails closed: a form the reader does not recognise
/// is a named error, never a guess.
pub type Checked<T> = Result<T, String>;

/// Where a scan for `run` commands stands: inside a `run: |` block (holding the
/// key's indent) or not, and what it has found.
///
/// It recognises a one-line `run: cmd` and a literal `run: |` block, the forms
/// this repository's workflows use. A folded `run: >` block is an error, since
/// YAML would join its lines into one command and the reader does not model that.
#[derive(Default)]
struct RunScan {
    /// The indent of the `run:` key whose block the scan is inside, if any.
    block: Option<usize>,
    /// The commands found, with their line offsets.
    found: Vec<(usize, String)>,
}

impl RunScan {
    /// Takes a line that sits inside the open block, ending the block otherwise.
    /// Returns whether the line was consumed.
    fn take_block_line(&mut self, at: usize, line: Text<'_>) -> bool {
        let Some(key_indent) = self.block else {
            return false;
        };
        if line.indent() > key_indent {
            self.found.push((at, line.0.trim().to_owned()));
            return true;
        }
        self.block = None;
        false
    }

    /// Starts a `run:` command from a line that holds the key, if it does.
    fn take_key_line(&mut self, at: usize, line: Text<'_>) -> Checked<()> {
        let Some(rest) = line.key_text().strip_prefix("run:") else {
            return Ok(());
        };
        let dash = if line.starts_step() { 2 } else { 0 };
        let inline = rest.trim();
        if inline.starts_with('>') {
            return Err(format!(
                "folded scalar `run: >` at line {at} is not recognised; write `run: |` or one line"
            ));
        }
        if inline.starts_with('|') {
            self.block = Some(line.indent() + dash);
        } else if !inline.is_empty() {
            self.found.push((at, inline.to_owned()));
        }
        Ok(())
    }

    /// Reads one line of the job.
    fn feed(&mut self, at: usize, line: Text<'_>) -> Checked<()> {
        if line.is_inert() || self.take_block_line(at, line) {
            return Ok(());
        }
        self.take_key_line(at, line)
    }
}

/// The lines of one workflow job.
pub struct Job<'a> {
    /// The job's lines, from its key to the next job.
    lines: Vec<&'a str>,
}

impl<'a> Job<'a> {
    /// Returns the job that contains the first line holding `needle`, or an
    /// empty job when none does. A job is a two-space-indented key under `jobs:`.
    pub fn containing(workflow: &'a str, needle: &str) -> Self {
        let lines: Vec<&str> = workflow.lines().collect();
        let starts_job = |line: &str| Text(line).indent() == 2 && line.trim_end().ends_with(':');
        let bounds = lines
            .iter()
            .position(|line| !Text(line).is_inert() && line.contains(needle))
            .and_then(|found| {
                let start =
                    (0..=found).rfind(|&i| lines.get(i).copied().is_some_and(starts_job))?;
                let end = (found + 1..lines.len())
                    .find(|&i| lines.get(i).copied().is_some_and(starts_job))
                    .unwrap_or(lines.len());
                Some(start..end)
            });
        Self {
            lines: bounds
                .and_then(|range| lines.get(range))
                .map(<[&str]>::to_vec)
                .unwrap_or_default(),
        }
    }

    /// Returns the offset of the first non-comment line holding `needle`.
    pub fn offset_of(&self, needle: &str) -> Option<usize> {
        self.lines
            .iter()
            .position(|line| !Text(line).is_inert() && line.contains(needle))
    }

    /// Returns the lines of the step that holds line `at`: from its `- ` line to
    /// the next step at the same or a shallower indent.
    fn step_range(&self, at: usize) -> std::ops::Range<usize> {
        let start = (0..=at)
            .rfind(|&i| self.lines.get(i).is_some_and(|l| Text(l).starts_step()))
            .unwrap_or(at);
        let start_indent = self.lines.get(start).map_or(0, |l| Text(l).indent());
        let end = (start + 1..self.lines.len())
            .find(|&i| {
                self.lines
                    .get(i)
                    .is_some_and(|l| Text(l).starts_step() && Text(l).indent() <= start_indent)
            })
            .unwrap_or(self.lines.len());
        start..end
    }

    /// Returns the lines of the step that holds line `at`.
    fn step(&self, at: usize) -> &[&'a str] {
        self.lines.get(self.step_range(at)).unwrap_or_default()
    }

    /// Returns whether any key of the step holding line `at` satisfies `wanted`.
    fn step_has(&self, at: usize, wanted: fn(&str) -> bool) -> bool {
        self.step(at)
            .iter()
            .any(|line| wanted(Text(line).key_text()))
    }

    /// Returns whether the step carries an `if:` condition, so it may be skipped.
    fn is_conditional(&self, at: usize) -> bool {
        self.step_has(at, |key| key.starts_with("if:"))
    }

    /// Returns whether the step runs the shared setup-rust action: its `uses:`
    /// value names that action's path exactly, not any text that contains it.
    fn uses_setup_rust(&self, at: usize) -> bool {
        self.step_has(at, |key| {
            key.strip_prefix("uses:").is_some_and(|value| {
                value
                    .trim()
                    .starts_with("leynos/shared-actions/.github/actions/setup-rust@")
            })
        })
    }

    /// Returns every executable `run` command, with its line offset: the inline
    /// text of `run: cmd` and each line of a `run: |` block.
    ///
    /// # Errors
    ///
    /// A folded `run: >` block, which this reader does not model.
    fn run_commands(&self) -> Checked<Vec<(usize, String)>> {
        let mut scan = RunScan::default();
        for (at, line) in self.lines.iter().enumerate() {
            scan.feed(at, Text(line))?;
        }
        Ok(scan.found)
    }

    /// Reads line `at` as an `install-mold` input: `None` when it is not one,
    /// otherwise whether its value is exactly `true` (quotes removed).
    ///
    /// # Errors
    ///
    /// A value with a trailing comment, which this reader does not model.
    fn mold_input(&self, at: usize) -> Checked<Option<bool>> {
        let Some(line) = self.lines.get(at).map(|line| Text(line)) else {
            return Ok(None);
        };
        let Some(raw) = line.key_text().strip_prefix("install-mold:") else {
            return Ok(None);
        };
        if line.is_inert() {
            return Ok(None);
        }
        if raw.contains(" #") {
            return Err(format!(
                "trailing comment in the `install-mold` value at line {at} is not recognised"
            ));
        }
        Ok(Some(raw.trim().trim_matches(['"', '\'']) == "true"))
    }

    /// Returns whether line `at` sits directly inside a `with:` mapping of its
    /// step: the nearest shallower line of the step is a `with:` key.
    fn is_under_with(&self, at: usize) -> bool {
        let Some(indent) = self.lines.get(at).map(|line| Text(line).indent()) else {
            return false;
        };
        let before = self.step_range(at).start..at;
        self.lines
            .get(before)
            .unwrap_or_default()
            .iter()
            .map(|line| Text(line))
            .rfind(|line| !line.is_inert() && line.indent() < indent)
            .is_some_and(|line| line.key_text() == "with:")
    }

    /// Returns whether line `at` is under `with:` of an unconditional step that
    /// runs the shared setup-rust action, so it takes effect.
    fn is_active_setup_rust_input(&self, at: usize) -> bool {
        self.is_under_with(at) && self.uses_setup_rust(at) && !self.is_conditional(at)
    }

    /// Returns the offset of the first installation of mold: an `apt` install
    /// command the job runs, or setup-rust's `install-mold` input set to true,
    /// in a step with no `if:` condition.
    ///
    /// # Errors
    ///
    /// A form the reader does not recognise: a folded `run: >` block, or a
    /// comment after an `install-mold` value.
    pub fn mold_install_offset(&self) -> Checked<Option<usize>> {
        let commands = self
            .run_commands()?
            .into_iter()
            .filter(|(at, text)| {
                !self.is_conditional(*at) && Text(text).chain().into_iter().any(Text::installs_mold)
            })
            .map(|(at, _)| at);
        let mut inputs = Vec::new();
        for at in 0..self.lines.len() {
            let on = self.mold_input(at)?.unwrap_or(false);
            if on && self.is_active_setup_rust_input(at) {
                inputs.push(at);
            }
        }
        Ok(commands.chain(inputs).min())
    }
}
