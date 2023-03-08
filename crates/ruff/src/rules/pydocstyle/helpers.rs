use std::collections::BTreeSet;
use std::str::Lines;

use ruff_python_ast::cast;
use ruff_python_ast::helpers::{map_callable, to_call_path};

use crate::checkers::ast::Checker;
use crate::docstrings::definition::{Definition, DefinitionKind};

pub struct LinesWhenConsiderLineContinuation<'a> {
    underlying: Lines<'a>,
}

impl<'a> Iterator for LinesWhenConsiderLineContinuation<'a> {
    type Item = (String, usize);

    fn next(&mut self) -> Option<Self::Item> {
        let mut ret = String::new();
        let mut actual_lines = 0_usize;

        // given a valid utf-8 string
        // new_line_start always starts at a valid code point
        for line in self.underlying.by_ref() {
            actual_lines += 1;

            // 0x5c is \ in ASCII
            // for utf-8 encoding str, only \'s last byte is equal to 0x0a
            if !line.as_bytes().ends_with(&[0x5c]) {
                ret.push_str(line);
                break;
            }

            // we know line_end - 1 is \
            // so, it is a valid code point
            // we ignore "\\\n" here because we want to correctly reflect the number of blank lines
            ret.push_str(&line[..line.len() - 1]);
        }

        // no more lines to consume
        if actual_lines == 0 {
            return None;
        }

        Some((ret, actual_lines))
    }
}

impl<'a> LinesWhenConsiderLineContinuation<'a> {
    pub fn from(input: &'a str) -> LinesWhenConsiderLineContinuation<'a> {
        LinesWhenConsiderLineContinuation {
            underlying: input.lines(),
        }
    }
}

/// Return the index of the first logical line in a string.
pub fn logical_line(content: &str) -> Option<usize> {
    // Find the first logical line.
    let mut logical_line = None;
    for (i, line) in content.lines().enumerate() {
        if line.trim().is_empty() {
            // Empty line. If this is the line _after_ the first logical line, stop.
            if logical_line.is_some() {
                break;
            }
        } else {
            // Non-empty line. Store the index.
            logical_line = Some(i);
        }
    }
    logical_line
}

/// Normalize a word by removing all non-alphanumeric characters
/// and converting it to lowercase.
pub fn normalize_word(first_word: &str) -> String {
    first_word
        .replace(|c: char| !c.is_alphanumeric(), "")
        .to_lowercase()
}

/// Check decorator list to see if function should be ignored.
pub fn should_ignore_definition(
    checker: &Checker,
    definition: &Definition,
    ignore_decorators: &BTreeSet<String>,
) -> bool {
    if ignore_decorators.is_empty() {
        return false;
    }

    if let DefinitionKind::Function(parent)
    | DefinitionKind::NestedFunction(parent)
    | DefinitionKind::Method(parent) = definition.kind
    {
        for decorator in cast::decorator_list(parent) {
            if let Some(call_path) = checker.ctx.resolve_call_path(map_callable(decorator)) {
                if ignore_decorators
                    .iter()
                    .any(|decorator| to_call_path(decorator) == call_path)
                {
                    return true;
                }
            }
        }
    }
    false
}
