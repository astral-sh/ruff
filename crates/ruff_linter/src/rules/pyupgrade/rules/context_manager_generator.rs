use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::token::parenthesized_range;
use ruff_python_ast::visitor::{Visitor, walk_expr, walk_stmt};
use ruff_python_ast::{self as ast, Expr, PythonVersion, Stmt};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::codes::Category;
use crate::importer::ImportRequest;
use crate::{Edit, Fix, FixAvailability, Violation};

/// ## What it does
/// Checks for generator-based context managers annotated with `Iterator` or
/// `AsyncIterator` instead of `Generator` or `AsyncGenerator`.
///
/// ## Why is this bad?
/// `contextlib.contextmanager` and `contextlib.asynccontextmanager` require
/// generators, which support methods such as `throw` that are not guaranteed
/// by the iterator protocols. The iterator overloads of these decorators are
/// deprecated in typeshed.
///
/// ## Example
///
/// ```python
/// from collections.abc import Iterator
/// from contextlib import contextmanager
///
///
/// @contextmanager
/// def example() -> Iterator[int]:
///     yield 1
/// ```
///
/// Use instead:
///
/// ```python
/// from collections.abc import Generator
/// from contextlib import contextmanager
///
///
/// @contextmanager
/// def example() -> Generator[int, None, None]:
///     yield 1
/// ```
///
/// ## Fix safety
/// The fix is unsafe because it changes the function's runtime annotations
/// and may add an import, introducing a new name in the module. For a generator
/// that returns a value, the fix uses `object` as the return type to accommodate
/// any return value.
///
/// ## Known limitations
/// The rule checks functions whose innermost decorator is a contextlib
/// context manager and whose bodies contain a `yield` or `yield from`.
///
/// ## References
/// - [Python documentation: `contextlib.contextmanager`](https://docs.python.org/3/library/contextlib.html#contextlib.contextmanager)
/// - [Python documentation: `contextlib.asynccontextmanager`](https://docs.python.org/3/library/contextlib.html#contextlib.asynccontextmanager)
#[derive(ViolationMetadata)]
#[violation_metadata(preview_since = "NEXT_RUFF_VERSION", category = Category::Suspicious)]
pub(crate) struct ContextManagerGenerator {
    iterator: &'static str,
    generator: &'static str,
}

impl Violation for ContextManagerGenerator {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        let Self {
            iterator,
            generator,
        } = self;
        format!("Use `{generator}` instead of `{iterator}` for a context manager")
    }

    fn fix_title(&self) -> Option<String> {
        Some(format!("Replace with `{}`", self.generator))
    }
}

/// UP052
pub(crate) fn context_manager_generator(checker: &Checker, function: &ast::StmtFunctionDef) {
    let Some(decorator) = function.decorator_list.last() else {
        return;
    };
    let Some(qualified_name) = checker
        .semantic()
        .resolve_qualified_name(&decorator.expression)
    else {
        return;
    };
    let (iterator, generator) = match (function.is_async, qualified_name.segments()) {
        (false, ["contextlib", "contextmanager"]) => ("Iterator", "Generator"),
        (true, ["contextlib", "asynccontextmanager"]) => ("AsyncIterator", "AsyncGenerator"),
        _ => return,
    };
    let Some(annotation) = function.returns.as_deref() else {
        return;
    };
    let (expression, simple, tokens) = if let Expr::StringLiteral(string) = annotation {
        let Ok(parsed) = checker.parse_type_annotation(string) else {
            return;
        };
        (
            parsed.expression(),
            parsed.kind().is_simple(),
            parsed.parsed().tokens(),
        )
    } else {
        (annotation, true, checker.tokens())
    };
    let Expr::Subscript(ast::ExprSubscript { value, slice, .. }) = expression else {
        return;
    };
    let Some(name) = checker.semantic().resolve_qualified_name(value) else {
        return;
    };
    let module = match name.segments() {
        ["typing", name] if *name == iterator => "typing",
        ["collections", "abc", name] if *name == iterator => "collections.abc",
        _ => return,
    };
    let (item, parent) = match slice.as_ref() {
        Expr::Tuple(tuple) => match tuple.elts.as_slice() {
            [item] => (item, slice.as_ref()),
            _ => return,
        },
        item => (item, expression),
    };

    let mut visitor = GeneratorBody::default();
    visitor.visit_body(&function.body);
    if !visitor.has_yield {
        return;
    }

    let mut diagnostic = checker.report_diagnostic(
        ContextManagerGenerator {
            iterator,
            generator,
        },
        if simple {
            expression.range()
        } else {
            annotation.range()
        },
    );
    if !simple {
        return;
    }
    diagnostic.try_set_fix(|| {
        let item_range =
            parenthesized_range(item.into(), parent.into(), tokens).unwrap_or_else(|| item.range());
        let import = checker.importer().get_or_import_symbol(
            &ImportRequest::import_from(module, generator),
            function.start(),
            checker.semantic(),
        );
        // Reuse an existing import from the other module if it conflicts with
        // adding the generator next to the iterator. The collections.abc
        // generics cannot be subscripted at runtime before Python 3.9.
        let (import_edit, binding) = if module == "collections.abc" {
            import.or_else(|_| {
                checker.importer().get_or_import_symbol(
                    &ImportRequest::import_from("typing", generator),
                    function.start(),
                    checker.semantic(),
                )
            })?
        } else if checker.target_version() >= PythonVersion::PY39 {
            import.or_else(|_| {
                checker.importer().get_or_import_symbol(
                    &ImportRequest::import_from("collections.abc", generator),
                    function.start(),
                    checker.semantic(),
                )
            })?
        } else {
            import?
        };
        let return_type = if visitor.returns_value {
            "object"
        } else {
            "None"
        };
        let (object_edit, return_type) = if visitor.returns_value {
            checker.importer().get_or_import_builtin_symbol(
                return_type,
                function.start(),
                checker.semantic(),
            )?
        } else {
            (None, return_type.to_string())
        };
        let arguments = if function.is_async {
            ", None".to_string()
        } else {
            format!(", None, {return_type}")
        };
        Ok(Fix::unsafe_edits(
            Edit::range_replacement(binding, value.range()),
            [import_edit, Edit::insertion(arguments, item_range.end())]
                .into_iter()
                .chain(object_edit),
        ))
    });
}

#[derive(Default)]
struct GeneratorBody {
    has_yield: bool,
    returns_value: bool,
}

impl<'a> Visitor<'a> for GeneratorBody {
    fn visit_stmt(&mut self, stmt: &'a Stmt) {
        match stmt {
            Stmt::FunctionDef(_) | Stmt::ClassDef(_) => {}
            Stmt::Return(return_stmt) => {
                if return_stmt
                    .value
                    .as_deref()
                    .is_some_and(|value| !value.is_none_literal_expr())
                {
                    self.returns_value = true;
                }
                walk_stmt(self, stmt);
            }
            _ => walk_stmt(self, stmt),
        }
    }

    fn visit_expr(&mut self, expr: &'a Expr) {
        match expr {
            Expr::Lambda(_) => {}
            Expr::Yield(_) | Expr::YieldFrom(_) => {
                self.has_yield = true;
                walk_expr(self, expr);
            }
            _ => walk_expr(self, expr),
        }
    }
}
