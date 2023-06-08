use rustpython_parser::ast;
use rustpython_parser::ast::Stmt;

use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::identifier_range;
use ruff_python_semantic::analyze::visibility::is_abstract;

use crate::autofix::edits::delete_stmt;
use crate::checkers::ast::Checker;
use crate::registry::AsRule;

/// ## What it does
/// Checks for redundant definitions of `__str__` or `__repr__` in stubs.
///
/// ## Why is this bad?
/// Defining `__str__` or `__repr__` in a stub is almost always redundant,
/// as the signatures are almost always identical to those of the default
/// equivalent, `object.__str__` and `object.__repr__`, respectively.
///
/// ## Example
/// ```python
/// class Foo:
///     def __repr__(self) -> str:
///         ...
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

    let Some(returns) = returns else {
        return;
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
        identifier_range(stmt, checker.locator),
    );
    if checker.patch(diagnostic.kind.rule()) {
        let stmt = checker.semantic_model().stmt();
        let parent = checker.semantic_model().stmt_parent();
        let edit = delete_stmt(
            stmt,
            parent,
            checker.locator,
            checker.indexer,
            checker.stylist,
        );
        diagnostic.set_fix(
            Fix::automatic(edit).isolate(checker.isolation(checker.semantic_model().stmt_parent())),
        );
    }
    checker.diagnostics.push(diagnostic);
}
