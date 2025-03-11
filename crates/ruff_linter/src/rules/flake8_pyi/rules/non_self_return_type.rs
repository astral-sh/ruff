use crate::checkers::ast::Checker;
use crate::importer::ImportRequest;
use ruff_diagnostics::{Applicability, Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast as ast;
use ruff_python_ast::helpers::map_subscript;
use ruff_python_ast::identifier::Identifier;
use ruff_python_ast::PythonVersion;
use ruff_python_semantic::analyze;
use ruff_python_semantic::analyze::class::might_be_generic;
use ruff_python_semantic::analyze::visibility::{is_abstract, is_final, is_overload};
use ruff_python_semantic::{ScopeKind, SemanticModel};
use ruff_text_size::Ranged;

/// ## What it does
/// Checks for methods that are annotated with a fixed return type which
/// should instead be returning `Self`.
///
/// ## Why is this bad?
/// If methods that generally return `self` at runtime are annotated with a
/// fixed return type, and the class is subclassed, type checkers will not be
/// able to infer the correct return type.
///
/// For example:
/// ```python
/// class Shape:
///     def set_scale(self, scale: float) -> Shape:
///         self.scale = scale
///         return self
///
/// class Circle(Shape):
///     def set_radius(self, radius: float) -> Circle:
///         self.radius = radius
///         return self
///
/// # Type checker infers return type as `Shape`, not `Circle`.
/// Circle().set_scale(0.5)
///
/// # Thus, this expression is invalid, as `Shape` has no attribute `set_radius`.
/// Circle().set_scale(0.5).set_radius(2.7)
/// ```
///
/// Specifically, this check enforces that the return type of the following
/// methods is `Self`:
///
/// 1. In-place binary-operation dunder methods, like `__iadd__`, `__imul__`, etc.
/// 1. `__new__`, `__enter__`, and `__aenter__`, if those methods return the
///    class name.
/// 1. `__iter__` methods that return `Iterator`, despite the class inheriting
///    directly from `Iterator`.
/// 1. `__aiter__` methods that return `AsyncIterator`, despite the class
///    inheriting directly from `AsyncIterator`.
///
/// ## Example
///
/// ```pyi
/// class Foo:
///     def __new__(cls, *args: Any, **kwargs: Any) -> Foo: ...
///     def __enter__(self) -> Foo: ...
///     async def __aenter__(self) -> Foo: ...
///     def __iadd__(self, other: Foo) -> Foo: ...
/// ```
///
/// Use instead:
///
/// ```pyi
/// from typing_extensions import Self
///
/// class Foo:
///     def __new__(cls, *args: Any, **kwargs: Any) -> Self: ...
///     def __enter__(self) -> Self: ...
///     async def __aenter__(self) -> Self: ...
///     def __iadd__(self, other: Foo) -> Self: ...
/// ```
///
/// ## Fix safety
/// This rule's fix is marked as unsafe as it changes the meaning of your type annotations.
///
/// ## References
/// - [Python documentation: `typing.Self`](https://docs.python.org/3/library/typing.html#typing.Self)
#[derive(ViolationMetadata)]
pub(crate) struct NonSelfReturnType {
    class_name: String,
    method_name: String,
}

impl Violation for NonSelfReturnType {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        let NonSelfReturnType {
            class_name,
            method_name,
        } = self;

        if matches!(class_name.as_str(), "__new__") {
            "`__new__` methods usually return `self` at runtime".to_string()
        } else {
            format!("`{method_name}` methods in classes like `{class_name}` usually return `self` at runtime")
        }
    }

    fn fix_title(&self) -> Option<String> {
        Some("Use `Self` as return type".to_string())
    }
}

/// PYI034
pub(crate) fn non_self_return_type(
    checker: &Checker,
    stmt: &ast::Stmt,
    is_async: bool,
    name: &str,
    decorator_list: &[ast::Decorator],
    returns: Option<&ast::Expr>,
    parameters: &ast::Parameters,
) {
    let semantic = checker.semantic();

    let ScopeKind::Class(class_def) = semantic.current_scope().kind else {
        return;
    };

    if parameters.args.is_empty() && parameters.posonlyargs.is_empty() {
        return;
    }

    let Some(returns) = returns else {
        return;
    };

    // PEP 673 forbids the use of `typing(_extensions).Self` in metaclasses.
    if analyze::class::is_metaclass(class_def, semantic).is_yes() {
        return;
    }

    // Skip any abstract or overloaded methods.
    if is_abstract(decorator_list, semantic) || is_overload(decorator_list, semantic) {
        return;
    }

    if is_async {
        if name == "__aenter__"
            && is_name(returns, &class_def.name)
            && !is_final(&class_def.decorator_list, semantic)
        {
            add_diagnostic(checker, stmt, returns, class_def, name);
        }
        return;
    }

    // In-place methods that are expected to return `Self`.
    if is_inplace_bin_op(name) {
        if !is_self(returns, checker) {
            add_diagnostic(checker, stmt, returns, class_def, name);
        }
        return;
    }

    if is_name(returns, &class_def.name) {
        if matches!(name, "__enter__" | "__new__") && !is_final(&class_def.decorator_list, semantic)
        {
            add_diagnostic(checker, stmt, returns, class_def, name);
        }
        return;
    }

    match name {
        "__iter__" => {
            if is_iterable_or_iterator(returns, semantic)
                && subclasses_iterator(class_def, semantic)
            {
                add_diagnostic(checker, stmt, returns, class_def, name);
            }
        }
        "__aiter__" => {
            if is_async_iterable_or_iterator(returns, semantic)
                && subclasses_async_iterator(class_def, semantic)
            {
                add_diagnostic(checker, stmt, returns, class_def, name);
            }
        }
        _ => {}
    }
}

/// Add a diagnostic for the given method.
fn add_diagnostic(
    checker: &Checker,
    stmt: &ast::Stmt,
    returns: &ast::Expr,
    class_def: &ast::StmtClassDef,
    method_name: &str,
) {
    let mut diagnostic = Diagnostic::new(
        NonSelfReturnType {
            class_name: class_def.name.to_string(),
            method_name: method_name.to_string(),
        },
        stmt.identifier(),
    );

    diagnostic.try_set_fix(|| replace_with_self_fix(checker, stmt, returns, class_def));

    checker.report_diagnostic(diagnostic);
}

fn replace_with_self_fix(
    checker: &Checker,
    stmt: &ast::Stmt,
    returns: &ast::Expr,
    class_def: &ast::StmtClassDef,
) -> anyhow::Result<Fix> {
    let semantic = checker.semantic();

    let (self_import, self_binding) = {
        let source_module = if checker.target_version() >= PythonVersion::PY311 {
            "typing"
        } else {
            "typing_extensions"
        };

        let (importer, semantic) = (checker.importer(), checker.semantic());
        let request = ImportRequest::import_from(source_module, "Self");
        importer.get_or_import_symbol(&request, returns.start(), semantic)?
    };

    let mut others = Vec::with_capacity(2);

    let remove_first_argument_type_hint = || -> Option<Edit> {
        let ast::StmtFunctionDef { parameters, .. } = stmt.as_function_def_stmt()?;
        let first = parameters.iter().next()?;
        let annotation = first.annotation()?;

        is_class_reference(semantic, annotation, &class_def.name)
            .then(|| Edit::deletion(first.name().end(), annotation.end()))
    };

    others.extend(remove_first_argument_type_hint());
    others.push(Edit::range_replacement(self_binding, returns.range()));

    let applicability = if might_be_generic(class_def, checker.semantic()) {
        Applicability::DisplayOnly
    } else {
        Applicability::Unsafe
    };

    Ok(Fix::applicable_edits(self_import, others, applicability))
}

/// Return true if `annotation` is either `ClassName` or `type[ClassName]`
fn is_class_reference(semantic: &SemanticModel, annotation: &ast::Expr, expected: &str) -> bool {
    if is_name(annotation, expected) {
        return true;
    }

    let ast::Expr::Subscript(ast::ExprSubscript { value, slice, .. }) = annotation else {
        return false;
    };

    if !semantic.match_builtin_expr(value, "type") && !semantic.match_typing_expr(value, "Type") {
        return false;
    }

    is_name(slice, expected)
}

/// Returns `true` if the method is an in-place binary operator.
fn is_inplace_bin_op(name: &str) -> bool {
    matches!(
        name,
        "__iadd__"
            | "__isub__"
            | "__imul__"
            | "__imatmul__"
            | "__itruediv__"
            | "__ifloordiv__"
            | "__imod__"
            | "__ipow__"
            | "__ilshift__"
            | "__irshift__"
            | "__iand__"
            | "__ixor__"
            | "__ior__"
    )
}

/// Return `true` if the given expression resolves to the given name.
fn is_name(expr: &ast::Expr, name: &str) -> bool {
    let ast::Expr::Name(ast::ExprName { id, .. }) = expr else {
        return false;
    };
    id.as_str() == name
}

/// Return `true` if the given expression resolves to `typing.Self`.
fn is_self(expr: &ast::Expr, checker: &Checker) -> bool {
    checker.match_maybe_stringized_annotation(expr, |expr| {
        checker.semantic().match_typing_expr(expr, "Self")
    })
}

/// Return `true` if the given class extends `collections.abc.Iterator`.
fn subclasses_iterator(class_def: &ast::StmtClassDef, semantic: &SemanticModel) -> bool {
    analyze::class::any_qualified_base_class(class_def, semantic, &|qualified_name| {
        matches!(
            qualified_name.segments(),
            ["typing", "Iterator"] | ["collections", "abc", "Iterator"]
        )
    })
}

/// Return `true` if the given expression resolves to `collections.abc.Iterable` or `collections.abc.Iterator`.
fn is_iterable_or_iterator(expr: &ast::Expr, semantic: &SemanticModel) -> bool {
    semantic
        .resolve_qualified_name(map_subscript(expr))
        .is_some_and(|qualified_name| {
            matches!(
                qualified_name.segments(),
                ["typing", "Iterable" | "Iterator"]
                    | ["collections", "abc", "Iterable" | "Iterator"]
            )
        })
}

/// Return `true` if the given class extends `collections.abc.AsyncIterator`.
fn subclasses_async_iterator(class_def: &ast::StmtClassDef, semantic: &SemanticModel) -> bool {
    analyze::class::any_qualified_base_class(class_def, semantic, &|qualified_name| {
        matches!(
            qualified_name.segments(),
            ["typing", "AsyncIterator"] | ["collections", "abc", "AsyncIterator"]
        )
    })
}

/// Return `true` if the given expression resolves to `collections.abc.AsyncIterable` or `collections.abc.AsyncIterator`.
fn is_async_iterable_or_iterator(expr: &ast::Expr, semantic: &SemanticModel) -> bool {
    semantic
        .resolve_qualified_name(map_subscript(expr))
        .is_some_and(|qualified_name| {
            matches!(
                qualified_name.segments(),
                ["typing", "AsyncIterable" | "AsyncIterator"]
                    | ["collections", "abc", "AsyncIterable" | "AsyncIterator"]
            )
        })
}
