//! Reads a parsed workflow as text, for the clauses that search values.
//!
//! The walk turns every key and scalar into one line, and the computed-secret
//! reader judges that text; the rules decide what each finding means.

use serde_yaml::{Mapping, Value};

/// Returns every key and scalar in a parsed value, one per line.
///
/// Reading the parse rather than the file is deliberate: comments are gone by
/// this point, so prose explaining why the token is absent does not read as
/// the token being present, while a block scalar's content, which Actions
/// expands, is read in full. It also reaches every place a value can sit, at
/// workflow, job or step scope, in a `run` body, an action input, an `env`
/// value, an `if:`, a `defaults.run.shell` or a `secrets:` forwarding,
/// without a clause having to be told about each. Keys carry their `:` so the
/// computed-secret clause can tell a `secrets:` key from an expression.
pub fn rendered(value: &Value) -> String {
    let mut text = String::new();
    collect_scalars(value, &mut text);
    text
}

/// Appends the keys and scalars of `value` to `text`, depth first.
fn collect_scalars(value: &Value, text: &mut String) {
    match value {
        Value::Null => {}
        Value::Bool(flag) => push_line(text, &flag.to_string()),
        Value::Number(number) => push_line(text, &number.to_string()),
        Value::String(string) => push_line(text, string),
        Value::Sequence(items) => items.iter().for_each(|item| collect_scalars(item, text)),
        Value::Mapping(mapping) => collect_mapping(mapping, text),
        Value::Tagged(tagged) => collect_scalars(&tagged.value, text),
    }
}

/// Appends each key of `mapping`, with its `:`, followed by its value.
fn collect_mapping(mapping: &Mapping, text: &mut String) {
    for (key, item) in mapping {
        let mut key_text = String::new();
        collect_scalars(key, &mut key_text);
        push_line(text, &format!("{}:", key_text.trim_end()));
        collect_scalars(item, text);
    }
}

/// Appends one line to `text`.
fn push_line(text: &mut String, line: &str) {
    text.push_str(line);
    text.push('\n');
}

/// Walks one step or job mapping, as [`rendered`] does a whole value.
pub fn rendered_mapping(mapping: &Mapping) -> String {
    let mut text = String::new();
    collect_mapping(mapping, &mut text);
    text
}

/// Returns whether `character` can continue an identifier or a property path.
const fn continues_a_name(character: char) -> bool {
    character.is_ascii_alphanumeric() || character == '_' || character == '.'
}

/// Returns whether `text` reaches the `secrets` context other than by name.
///
/// `secrets['CS_' + ...]`, `secrets[format('CS_{0}', 'ACCESS_TOKEN')]` and
/// `toJSON(secrets)` all hand a step the token without spelling it, so a
/// search for the name alone passes them. Every `secrets` word is therefore
/// judged by what follows it: `.` is a named reference, which the name search
/// already judges, and `:` is a YAML key (`secrets: inherit` or a named
/// forwarding), which the job clauses judge. Anything else is a computed or
/// whole-context access and is refused.
pub fn computes_a_secret(text: &str) -> bool {
    text.match_indices("secrets").any(|(start, word)| {
        let before = text.get(..start).and_then(|head| head.chars().next_back());
        let next = text
            .get(start + word.len()..)
            .and_then(|tail| tail.trim_start().chars().next());
        let is_word_start = !before.is_some_and(continues_a_name);
        // A word following `secrets` makes it prose ("keeps secrets out of
        // logs"), not an expression.
        let is_prose = next.is_some_and(|c| c.is_ascii_alphanumeric() || c == '_');
        is_word_start && !is_prose && !matches!(next, Some('.' | ':') | None)
    })
}
