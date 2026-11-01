use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::helpers::map_subscript;
use ruff_python_ast::{self as ast, Expr, ExprKind, Stmt, StmtKind};
use ruff_python_semantic::{Scope, SemanticModel};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::codes::Category;
use crate::fix;
use crate::{Fix, FixAvailability, Violation};

/// ## What it does
/// Checks for the presence of unused private `TypeVar`, `ParamSpec` or
/// `TypeVarTuple` declarations.
///
/// ## Why is this bad?
/// A private `TypeVar` that is defined but not used is likely a mistake. It
/// should either be used, made public, or removed to avoid confusion. A type
/// variable is considered "private" if its name starts with an underscore.
///
/// ## Example
/// ```pyi
/// import typing
/// import typing_extensions
///
/// _T = typing.TypeVar("_T")
/// _Ts = typing_extensions.TypeVarTuple("_Ts")
/// ```
///
/// ## Fix safety
/// The fix is always marked as unsafe, as it would break your code if the type
/// variable is imported by another module.
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "v0.0.281", category = Category::Suspicious)]
pub(crate) struct UnusedPrivateTypeVar {
    type_var_like_name: String,
    type_var_like_kind: String,
}

impl Violation for UnusedPrivateTypeVar {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Always;

    #[derive_message_formats]
    fn message(&self) -> String {
        let UnusedPrivateTypeVar {
            type_var_like_name,
            type_var_like_kind,
        } = self;
        format!("Private {type_var_like_kind} `{type_var_like_name}` is never used")
    }

    fn fix_title(&self) -> Option<String> {
        let UnusedPrivateTypeVar {
            type_var_like_name,
            type_var_like_kind,
        } = self;
        Some(format!(
            "Remove unused private {type_var_like_kind} `{type_var_like_name}`"
        ))
    }
}

/// ## What it does
/// Checks for the presence of unused private `typing.Protocol` definitions.
///
/// ## Why is this bad?
/// A private `typing.Protocol` that is defined but not used is likely a
/// mistake. It should either be used, made public, or removed to avoid
/// confusion.
///
/// ## Example
///
/// ```pyi
/// import typing
///
/// class _PrivateProtocol(typing.Protocol):
///     foo: int
/// ```
///
/// Use instead:
///
/// ```pyi
/// import typing
///
/// class _PrivateProtocol(typing.Protocol):
///     foo: int
///
/// def func(arg: _PrivateProtocol) -> None: ...
/// ```
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "v0.0.281", category = Category::Suspicious)]
pub(crate) struct UnusedPrivateProtocol {
    name: String,
}

impl Violation for UnusedPrivateProtocol {
    #[derive_message_formats]
    fn message(&self) -> String {
        let UnusedPrivateProtocol { name } = self;
        format!("Private protocol `{name}` is never used")
    }
}

/// ## What it does
/// Checks for the presence of unused private type aliases.
///
/// ## Why is this bad?
/// A private type alias that is defined but not used is likely a
/// mistake. It should either be used, made public, or removed to avoid
/// confusion.
///
/// ## Example
///
/// ```pyi
/// import typing
///
/// _UnusedTypeAlias: typing.TypeAlias = int
/// ```
///
/// Use instead:
///
/// ```pyi
/// import typing
///
/// _UsedTypeAlias: typing.TypeAlias = int
///
/// def func(arg: _UsedTypeAlias) -> _UsedTypeAlias: ...
/// ```
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "v0.0.281", category = Category::Suspicious)]
pub(crate) struct UnusedPrivateTypeAlias {
    name: String,
}

impl Violation for UnusedPrivateTypeAlias {
    #[derive_message_formats]
    fn message(&self) -> String {
        let UnusedPrivateTypeAlias { name } = self;
        format!("Private TypeAlias `{name}` is never used")
    }
}

/// ## What it does
/// Checks for the presence of unused private `typing.TypedDict` definitions.
///
/// ## Why is this bad?
/// A private `typing.TypedDict` that is defined but not used is likely a
/// mistake. It should either be used, made public, or removed to avoid
/// confusion.
///
/// ## Example
///
/// ```pyi
/// import typing
///
/// class _UnusedPrivateTypedDict(typing.TypedDict):
///     foo: list[int]
/// ```
///
/// Use instead:
///
/// ```pyi
/// import typing
///
/// class _UsedPrivateTypedDict(typing.TypedDict):
///     foo: set[str]
///
/// def func(arg: _UsedPrivateTypedDict) -> _UsedPrivateTypedDict: ...
/// ```
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "v0.0.281", category = Category::Suspicious)]
pub(crate) struct UnusedPrivateTypedDict {
    name: String,
}

impl Violation for UnusedPrivateTypedDict {
    #[derive_message_formats]
    fn message(&self) -> String {
        let UnusedPrivateTypedDict { name } = self;
        format!("Private TypedDict `{name}` is never used")
    }
}

/// PYI018
pub(crate) fn unused_private_type_var(checker: &Checker, scope: &Scope) {
    for binding in scope
        .binding_ids()
        .map(|binding_id| checker.semantic().binding(binding_id))
    {
        if !(binding.kind.is_assignment() && binding.is_private_declaration()) {
            continue;
        }
        if binding.is_used() {
            continue;
        }

        let Some(source) = binding.source else {
            continue;
        };
        let stmt = checker.semantic().statement(source);
        let StmtKind::Assign(assign) = stmt.classify() else {
            continue;
        };
        let targets = assign.targets();
        let value = assign.value();
        let Some([ast::ExprFields::Name(ast::ExprNameFields { id, .. })]) =
            targets.as_array().map(|[target]| [target.fields()])
        else {
            continue;
        };
        let ExprKind::Call(call) = value.classify() else {
            continue;
        };
        let func = call.func();

        let semantic = checker.semantic();
        let Some(type_var_like_kind) =
            semantic
                .resolve_qualified_name(func)
                .and_then(|qualified_name| {
                    if semantic.match_typing_qualified_name(&qualified_name, "TypeVar") {
                        Some("TypeVar")
                    } else if semantic.match_typing_qualified_name(&qualified_name, "ParamSpec") {
                        Some("ParamSpec")
                    } else if semantic.match_typing_qualified_name(&qualified_name, "TypeVarTuple")
                    {
                        Some("TypeVarTuple")
                    } else {
                        None
                    }
                })
        else {
            continue;
        };

        checker
            .report_diagnostic(
                UnusedPrivateTypeVar {
                    type_var_like_name: id.to_string(),
                    type_var_like_kind: type_var_like_kind.to_string(),
                },
                binding.range(),
            )
            .set_fix(Fix::unsafe_edit(fix::edits::delete_stmt(
                stmt,
                None,
                checker.locator(),
                checker.indexer(),
            )));
    }
}

/// PYI046
pub(crate) fn unused_private_protocol(checker: &Checker, scope: &Scope) {
    for binding in scope
        .binding_ids()
        .map(|binding_id| checker.semantic().binding(binding_id))
    {
        if !(binding.kind.is_class_definition() && binding.is_private_declaration()) {
            continue;
        }
        if binding.is_used() {
            continue;
        }

        let Some(source) = binding.source else {
            continue;
        };

        let StmtKind::ClassDef(class_def) = checker.semantic().statement(source).classify() else {
            continue;
        };

        if !class_def.bases().iter().any(|base| {
            checker
                .semantic()
                .match_typing_expr(map_subscript(base), "Protocol")
        }) {
            continue;
        }

        checker.report_diagnostic(
            UnusedPrivateProtocol {
                name: class_def.name().to_string(),
            },
            binding.range(),
        );
    }
}

/// PYI047
pub(crate) fn unused_private_type_alias(checker: &Checker, scope: &Scope) {
    let semantic = checker.semantic();

    for binding in scope
        .binding_ids()
        .map(|binding_id| semantic.binding(binding_id))
    {
        if !(binding.kind.is_assignment() && binding.is_private_declaration()) {
            continue;
        }
        if binding.is_used() {
            continue;
        }

        let Some(source) = binding.source else {
            continue;
        };

        let Some(alias_name) = extract_type_alias_name(semantic.statement(source), semantic) else {
            continue;
        };

        checker.report_diagnostic(
            UnusedPrivateTypeAlias {
                name: alias_name.to_string(),
            },
            binding.range(),
        );
    }
}

fn extract_type_alias_name<'a>(stmt: Stmt<'a>, semantic: &SemanticModel) -> Option<&'a str> {
    match stmt.classify() {
        StmtKind::AnnAssign(stmt_ann_assign) => {
            let target = stmt_ann_assign.target();
            let annotation = stmt_ann_assign.annotation();

            let expr_name = target.as_name_expr()?;
            let id = expr_name.id();
            if semantic.match_typing_expr(annotation, "TypeAlias") {
                Some(id)
            } else {
                None
            }
        }
        StmtKind::TypeAlias(stmt_type_alias) => {
            let name = stmt_type_alias.name();

            let expr_name = name.as_name_expr()?;
            let id = expr_name.id();
            Some(id)
        }
        _ => None,
    }
}

/// PYI049
pub(crate) fn unused_private_typed_dict(checker: &Checker, scope: &Scope) {
    let semantic = checker.semantic();

    for binding in scope
        .binding_ids()
        .map(|binding_id| semantic.binding(binding_id))
    {
        if !binding.is_private_declaration() {
            continue;
        }
        if !(binding.kind.is_class_definition() || binding.kind.is_assignment()) {
            continue;
        }
        if binding.is_used() {
            continue;
        }

        let Some(source) = binding.source else {
            continue;
        };

        let Some(class_name) = extract_typeddict_name(semantic.statement(source), semantic) else {
            continue;
        };

        checker.report_diagnostic(
            UnusedPrivateTypedDict {
                name: class_name.to_string(),
            },
            binding.range(),
        );
    }
}

fn extract_typeddict_name<'a>(stmt: Stmt<'a>, semantic: &SemanticModel) -> Option<&'a str> {
    let is_typeddict = |expr: Expr<'_>| semantic.match_typing_expr(expr, "TypedDict");
    match stmt.classify() {
        // E.g. return `Some("Foo")` for the first one of these classes,
        // and `Some("Bar")` for the second:
        //
        // ```python
        // import typing
        // from typing import TypedDict
        //
        // class Foo(TypedDict):
        //     x: int
        //
        // T = typing.TypeVar("T")
        //
        // class Bar(typing.TypedDict, typing.Generic[T]):
        //     y: T
        // ```
        StmtKind::ClassDef(class_def) => {
            let name = class_def.name();

            if class_def.bases().iter().any(is_typeddict) {
                Some(name.as_str())
            } else {
                None
            }
        }
        // E.g. return `Some("Baz")` for this assignment,
        // which is an accepted alternative way of creating a TypedDict type:
        //
        // ```python
        // import typing
        // Baz = typing.TypedDict("Baz", {"z": bytes})
        // ```
        StmtKind::Assign(stmt_assign) => {
            let targets = stmt_assign.targets();
            let value = stmt_assign.value();

            let [target] = targets.as_array()?;
            let expr_name = target.as_name_expr()?;
            let id = expr_name.id();
            let expr_call = value.as_call_expr()?;
            let func = expr_call.func();
            if is_typeddict(func) { Some(id) } else { None }
        }
        _ => None,
    }
}
