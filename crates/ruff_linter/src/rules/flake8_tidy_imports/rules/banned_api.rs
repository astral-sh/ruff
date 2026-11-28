use rustc_hash::FxHashMap;

use ruff_python_ast::identifier::Identifier;
use ruff_python_ast::name::QualifiedName;
use ruff_python_ast::{self as ast, Expr};
use ruff_python_semantic::{BindingKind, SemanticModel};
use ruff_text_size::Ranged;

use ruff_macros::{ViolationMetadata, derive_message_formats};

use crate::Violation;
use crate::checkers::ast::Checker;
use crate::codes::Category;
use crate::rules::flake8_tidy_imports::matchers::NameMatchPolicy;
use crate::rules::flake8_tidy_imports::settings::ApiBan;

/// ## What it does
/// Checks for banned imports.
///
/// ## Why is this bad?
/// Projects may want to ensure that specific modules or module members are
/// not imported or accessed.
///
/// Security or other company policies may be a reason to impose
/// restrictions on importing external Python libraries. In some cases,
/// projects may adopt conventions around the use of certain modules or
/// module members that are not enforceable by the language itself.
///
/// This rule enforces certain import conventions project-wide automatically.
///
/// ## Options
/// - `lint.flake8-tidy-imports.banned-api`
/// - `lint.flake8-tidy-imports.extend-banned-api`
///
/// ## Preview
/// In preview mode, this rule also checks method calls on values whose type
/// is known from a type annotation. For example, with `pathlib.Path.unlink`
/// banned, `path.unlink()` is flagged when `path` is annotated as `Path`.
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "v0.0.201", category = Category::Restriction)]
pub(crate) struct BannedApi {
    name: String,
    message: String,
}

impl Violation for BannedApi {
    #[derive_message_formats]
    fn message(&self) -> String {
        let BannedApi { name, message } = self;
        format!("`{name}` is banned: {message}")
    }
}

/// TID251
pub(crate) fn banned_api<T: Ranged>(checker: &Checker, policy: &NameMatchPolicy, node: &T) {
    let banned_api = &checker.settings().flake8_tidy_imports.banned_api;
    if let Some(banned_module) = policy.find(banned_api.keys().map(AsRef::as_ref)) {
        if let Some(reason) = banned_api.get(&banned_module) {
            checker.report_diagnostic(
                BannedApi {
                    name: banned_module,
                    message: reason.msg.clone(),
                },
                node.range(),
            );
        }
    }
}

/// TID251
pub(crate) fn banned_attribute_access(checker: &Checker, expr: &Expr) {
    let banned_api = &checker.settings().flake8_tidy_imports.banned_api;
    if banned_api.is_empty() {
        return;
    }

    // Resolve the attribute access to a qualified name, e.g. `Path.unlink`
    // resolves to `pathlib.Path.unlink`. In preview mode, also resolve
    // method calls on values whose type is known from a type annotation,
    // e.g. `path.unlink()` where `path` is annotated as `pathlib.Path`.
    let banned = checker
        .semantic()
        .resolve_qualified_name(expr)
        .and_then(|qualified_name| {
            // Preserve the empty leading segment for attributes of implicit builtins.
            banned_api
                .get_key_value(&qualified_name.segments().join("."))
                .filter(|(banned_path, ..)| {
                    qualified_name == QualifiedName::from_dotted_name(banned_path)
                })
        })
        .or_else(|| {
            checker
                .settings()
                .preview
                .is_enabled()
                .then(|| find_annotated_receiver_ban(checker, banned_api, expr))
                .flatten()
        });

    if let Some((banned_path, ban)) = banned {
        checker.report_diagnostic(
            BannedApi {
                name: banned_path.clone(),
                message: ban.msg.clone(),
            },
            expr.range(),
        );
    }
}

/// In preview mode, resolve an attribute access on a value with a type
/// annotation to a banned API, e.g. `path.unlink()` where `path` is
/// annotated as `Path` matches a ban on `pathlib.Path.unlink`.
///
/// Only the annotation is consulted; the type is not inferred from the
/// value's initializer, and string annotations are not resolved.
fn find_annotated_receiver_ban<'a>(
    checker: &Checker,
    banned_api: &'a FxHashMap<String, ApiBan>,
    expr: &Expr,
) -> Option<(&'a String, &'a ApiBan)> {
    let Expr::Attribute(ast::ExprAttribute { value, attr, .. }) = expr else {
        return None;
    };
    let Expr::Name(name) = value.as_ref() else {
        return None;
    };

    let semantic = checker.semantic();
    let binding_id = semantic.resolve_name(name)?;
    let binding = semantic.binding(binding_id);

    // Find the annotation for the binding, e.g. the parameter annotation in
    // `def remove(path: Path)` or the annotation in `target: Path = ...`.
    let annotation = match binding.kind {
        BindingKind::Argument => {
            let ast::Stmt::FunctionDef(function) = binding.statement(semantic)? else {
                return None;
            };
            let parameter = function
                .parameters
                .iter_non_variadic_params()
                .find(|parameter| parameter.identifier() == binding.range())?;
            parameter.annotation()?
        }
        BindingKind::Annotation | BindingKind::Assignment => {
            let ast::Stmt::AnnAssign(ann_assign) = binding.statement(semantic)? else {
                return None;
            };
            &ann_assign.annotation
        }
        _ => return None,
    };

    let annotation = unwrap_annotated(annotation, semantic);
    let qualified_name = semantic
        .resolve_qualified_name(annotation)?
        .extend_members([attr.as_str()]);

    banned_api
        .get_key_value(&qualified_name.segments().join("."))
        .filter(|(banned_path, ..)| qualified_name == QualifiedName::from_dotted_name(banned_path))
}

/// Unwrap `Annotated[X, ...]` to `X`.
fn unwrap_annotated<'a>(expr: &'a Expr, semantic: &SemanticModel) -> &'a Expr {
    let Expr::Subscript(ast::ExprSubscript { value, slice, .. }) = expr else {
        return expr;
    };
    if !semantic.match_typing_expr(value, "Annotated") {
        return expr;
    }
    let Some(tuple) = slice.as_tuple_expr() else {
        return expr;
    };
    let [inner, ..] = &tuple.elts[..] else {
        return expr;
    };
    unwrap_annotated(inner, semantic)
}
