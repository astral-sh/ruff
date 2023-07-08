use std::ops::Add;

use ruff_text_size::{TextRange, TextSize};
use rustpython_parser::ast::{self, Ranged};

use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};

use crate::checkers::ast::Checker;
use crate::registry::AsRule;

/// ## What it does
/// Checks for class definitions that include unnecessary parentheses after
/// the class name.
///
/// ## Why is this bad?
/// If a class definition doesn't have any bases, the parentheses are
/// unnecessary.
///
/// ## Examples
/// ```python
/// class Foo():
///     ...
/// ```
///
/// Use instead:
/// ```python
/// class Foo:
///     ...
/// ```
#[violation]
pub struct UnnecessaryClassParentheses;

impl AlwaysAutofixableViolation for UnnecessaryClassParentheses {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Unnecessary parentheses after class definition")
    }

    fn autofix_title(&self) -> String {
        "Remove parentheses".to_string()
    }
}

/// UP039
pub(crate) fn unnecessary_class_parentheses(checker: &mut Checker, class_def: &ast::StmtClassDef) {
    if !class_def.bases.is_empty() || !class_def.keywords.is_empty() {
        return;
    }

    let offset = class_def.name.end();
    let contents = checker.locator.after(offset);

    // Find the open and closing parentheses between the class name and the colon, if they exist.
    let mut depth = 0u32;
    let mut start = None;
    let mut end = None;
    for (i, c) in contents.char_indices() {
        match c {
            '(' => {
                if depth == 0 {
                    start = Some(i);
                }
                depth = depth.saturating_add(1);
            }
            ')' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    end = Some(i + c.len_utf8());
                }
            }
            ':' => {
                if depth == 0 {
                    break;
                }
            }
            _ => {}
        }
    }
    let (Some(start), Some(end)) = (start, end) else {
        return;
    };

    // Convert to `TextSize`.
    let start = TextSize::try_from(start).unwrap();
    let end = TextSize::try_from(end).unwrap();

    // Add initial offset.
    let start = offset.add(start);
    let end = offset.add(end);

    let mut diagnostic = Diagnostic::new(UnnecessaryClassParentheses, TextRange::new(start, end));
    if checker.patch(diagnostic.kind.rule()) {
        diagnostic.set_fix(Fix::automatic(Edit::deletion(start, end)));
    }
    checker.diagnostics.push(diagnostic);
}
