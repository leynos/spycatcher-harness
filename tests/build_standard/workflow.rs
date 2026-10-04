//! A reader for the lines of a GitHub Actions workflow job.
//!
//! It models only what the CI-order contract needs: finding the job that holds a
//! step, the executable `run` commands of that job, whether a step is
//! conditional, and where mold is installed. Text that is not a runnable command
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

/// Where a scan for `run` commands stands: inside a `run: |` block (holding the
/// key's indent) or not, and what it has found.
#[derive(Default)]
struct RunScan<'a> {
    /// The indent of the `run:` key whose block the scan is inside, if any.
    block: Option<usize>,
    /// The commands found, with their line offsets.
    found: Vec<(usize, &'a str)>,
}

impl<'a> RunScan<'a> {
    /// Takes a line that sits inside the open block, ending the block otherwise.
    /// Returns whether the line was consumed.
    fn take_block_line(&mut self, at: usize, line: Text<'a>) -> bool {
        let Some(key_indent) = self.block else {
            return false;
        };
        if line.indent() > key_indent {
            self.found.push((at, line.0.trim()));
            return true;
        }
        self.block = None;
        false
    }

    /// Starts a `run:` command from a line that holds the key, if it does.
    fn take_key_line(&mut self, at: usize, line: Text<'a>) {
        let Some(rest) = line.key_text().strip_prefix("run:") else {
            return;
        };
        let dash = if line.starts_step() { 2 } else { 0 };
        let inline = rest.trim();
        if inline.starts_with(['|', '>']) {
            self.block = Some(line.indent() + dash);
        } else if !inline.is_empty() {
            self.found.push((at, inline));
        }
    }

    /// Reads one line of the job.
    fn feed(&mut self, at: usize, line: Text<'a>) {
        if !line.is_inert() && !self.take_block_line(at, line) {
            self.take_key_line(at, line);
        }
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
    fn step(&self, at: usize) -> &[&'a str] {
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
        self.lines.get(start..end).unwrap_or_default()
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

    /// Returns whether the step runs the setup-rust action.
    fn uses_setup_rust(&self, at: usize) -> bool {
        self.step_has(at, |key| {
            key.starts_with("uses:") && key.contains("setup-rust")
        })
    }

    /// Returns every executable `run` command, with its line offset: the inline
    /// text of `run: cmd` and each line of a `run: |` or `run: >` block.
    fn run_commands(&self) -> Vec<(usize, &'a str)> {
        let mut scan = RunScan::default();
        for (at, line) in self.lines.iter().enumerate() {
            scan.feed(at, Text(line));
        }
        scan.found
    }

    /// Returns whether line `at` is setup-rust's `install-mold` input set to true.
    fn is_mold_input(&self, at: usize) -> bool {
        let text = self.lines.get(at).copied().map_or("", |line| line.trim());
        !Text(text).is_inert() && text.starts_with("install-mold:") && text.contains("true")
    }

    /// Returns the offset of the first installation of mold: an `apt` install
    /// command the job runs, or setup-rust's `install-mold` input set to true,
    /// in a step with no `if:` condition.
    pub fn mold_install_offset(&self) -> Option<usize> {
        let commands = self
            .run_commands()
            .into_iter()
            .filter(|&(at, text)| {
                !self.is_conditional(at) && Text(text).chain().into_iter().any(Text::installs_mold)
            })
            .map(|(at, _)| at);
        let inputs = (0..self.lines.len()).filter(|&at| {
            self.is_mold_input(at) && self.uses_setup_rust(at) && !self.is_conditional(at)
        });
        commands.chain(inputs).min()
    }
}
