use anyhow::{Context, Result};
use ruff_python_ast::{self as ast, Arguments, Expr};
use rustc_hash::{FxBuildHasher, FxHashSet};

use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_trivia::{SimpleTokenKind, SimpleTokenizer};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for duplicate base classes in class definitions.
///
/// ## Why is this bad?
/// Including duplicate base classes will raise a `TypeError` at runtime.
///
/// ## Example
/// ```python
/// class Foo:
///     pass
///
///
/// class Bar(Foo, Foo):
///     pass
/// ```
///
/// Use instead:
/// ```python
/// class Foo:
///     pass
///
///
/// class Bar(Foo):
///     pass
/// ```
///
/// ## References
/// - [Python documentation: Class definitions](https://docs.python.org/3/reference/compound_stmts.html#class-definitions)
#[violation]
pub struct DuplicateBases {
    base: String,
    class: String,
}

impl Violation for DuplicateBases {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        let DuplicateBases { base, class } = self;
        format!("Duplicate base `{base}` for class `{class}`")
    }

    fn fix_title(&self) -> Option<String> {
        Some(format!("Remove duplicate base"))
    }
}

/// PLE0241
pub(crate) fn duplicate_bases(checker: &mut Checker, name: &str, arguments: Option<&Arguments>) {
    let Some(Arguments { args: bases, .. }) = arguments else {
        return;
    };

    let mut seen: FxHashSet<&str> = FxHashSet::with_capacity_and_hasher(bases.len(), FxBuildHasher);
    let mut prev: Option<&Expr> = bases.iter().next();
    let len: usize = bases.iter().count();
    for (index, base) in bases.iter().enumerate() {
        if let Expr::Name(ast::ExprName { id, .. }) = base {
            if !seen.insert(id) {
                let mut diagnostic = Diagnostic::new(
                    DuplicateBases {
                        base: id.to_string(),
                        class: name.to_string(),
                    },
                    base.range(),
                );
                // diagnostic.set_fix(Fix::safe_edit(Edit::range_deletion(base.range())));
                diagnostic.try_set_fix(|| {
                    remove_base(
                        base,
                        prev.unwrap(),
                        index == len - 1,
                        checker.locator().contents(),
                    )
                    .map(Fix::safe_edit)
                });
                checker.diagnostics.push(diagnostic);
            }
        }
        prev = Some(&base);
    }
}

/// Remove the base at the given index.
fn remove_base(base: &Expr, prev: &Expr, is_last: bool, source: &str) -> Result<Edit> {
    if !is_last {
        // Case 1: the base class is _not_ the last one, so delete from the start of the
        // expression to the end of the subsequent comma.
        // Ex) Delete `A` in `class Foo(A, B)`.
        let mut tokenizer = SimpleTokenizer::starts_at(base.end(), source);

        // Find the trailing comma.
        tokenizer
            .find(|token| token.kind == SimpleTokenKind::Comma)
            .context("Unable to find trailing comma")?;

        // Find the next non-whitespace token.
        let next = tokenizer
            .find(|token| {
                token.kind != SimpleTokenKind::Whitespace && token.kind != SimpleTokenKind::Newline
            })
            .context("Unable to find next token")?;

        Ok(Edit::deletion(base.start(), next.start()))
    } else {
        // Case 2: the expression is the last node, but not the _only_ node, so delete from the
        // start of the previous comma to the end of the expression.
        // Ex) Delete `B` in `class Foo(A, B)`.
        let mut tokenizer = SimpleTokenizer::starts_at(prev.end(), source);

        // Find the trailing comma.
        let comma = tokenizer
            .find(|token| token.kind == SimpleTokenKind::Comma)
            .context("Unable to find trailing comma")?;

        Ok(Edit::deletion(comma.start(), base.end()))
    }
}
