use rustpython_parser::ast;
use rustpython_parser::ast::{Ranged, Stmt};

use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_semantic::analyze::visibility::is_abstract;

use crate::autofix::edits::delete_stmt;
use crate::checkers::ast::Checker;
use crate::registry::AsRule;

/// ## What it does
/// Checks for redundant definitions of `__str__` or `__repr__` in stubs.
///
/// ## Why is this bad?
/// These definitions are redundant with `object.__str__` or `object.__repr__`.
///
/// ## Example
/// ```python
/// class Foo:
///    def __repr__(self) -> str: ...
/// ```
#[violation]
pub struct StrOrReprDefinedInStub {
    name: String,
}

impl AlwaysAutofixableViolation for StrOrReprDefinedInStub {
    #[derive_message_formats]
    fn message(&self) -> String {
        let StrOrReprDefinedInStub { name } = self;
        format!("Defining `{name}` in a stub is almost always redundant")
    }

    fn autofix_title(&self) -> String {
        let StrOrReprDefinedInStub { name } = self;
        format!("Remove definition of `{name}`")
    }
}

/// PYI029
pub(crate) fn str_or_repr_defined_in_stub(checker: &mut Checker, stmt: &Stmt) {
    let Stmt::FunctionDef(ast::StmtFunctionDef {
                              name,
                              decorator_list,
                              returns,
                              args,
                              ..
                          }) = stmt else {
        return
    };

    if !matches!(name.as_str(), "__str__" | "__repr__") {
        return;
    }

    if !checker.semantic_model().scope().kind.is_class() {
        return;
    }

    // It is a violation only if the method signature matches that of `object.__str__`
    // or `object.__repr__` exactly and the method is not decorated as abstract.
    if !args.kwonlyargs.is_empty() || (args.args.len() + args.posonlyargs.len()) > 1 {
        return;
    }

    if is_abstract(checker.semantic_model(), decorator_list) {
        return;
    }

    let Some(returns) = returns else {
        return;
    };

    if checker
        .semantic_model()
        .resolve_call_path(returns)
        .map_or(true, |call_path| {
            !matches!(call_path.as_slice(), ["" | "builtins", "str"])
        })
    {
        return;
    }

    let mut diagnostic = Diagnostic::new(
        StrOrReprDefinedInStub {
            name: name.to_string(),
        },
        stmt.range(),
    );

    if checker.patch(diagnostic.kind.rule()) {
        let mut edit = delete_stmt(
            stmt,
            checker.semantic_model().stmt_parent(),
            checker.locator,
            checker.indexer,
            checker.stylist,
        );

        // If we removed the last statement, replace it with `...` instead of `pass` since we're
        // editing a stub.
        if edit.content() == Some("pass") {
            edit = Edit::range_replacement("...".to_string(), edit.range());
        }

        diagnostic.set_fix(
            Fix::automatic(edit).isolate(checker.isolation(checker.semantic_model().stmt_parent())),
        );
    }

    checker.diagnostics.push(diagnostic);
}
