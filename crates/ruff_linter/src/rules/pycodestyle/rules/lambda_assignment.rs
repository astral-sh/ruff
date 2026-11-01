use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::token::parenthesized_range;
use ruff_python_ast::{
    self as ast, BuildId, Builder, Expr, ExprKind, ExprLambda, ListId, NodeList,
    ParameterWithDefault, Parameters, Stmt, build,
};
use ruff_python_semantic::SemanticModel;
use ruff_python_trivia::{has_leading_content, has_trailing_content, leading_indentation};
use ruff_source_file::UniversalNewlines;
use ruff_text_size::{Ranged, TextRange};

use crate::checkers::ast::Checker;
use crate::codes::Category;
use crate::fix::syntax;
use crate::{Applicability, Edit, Fix, FixAvailability, Violation};

/// ## What it does
/// Checks for lambda expressions which are assigned to a variable.
///
/// ## Why is this bad?
/// Per PEP 8, you should "Always use a def statement instead of an assignment
/// statement that binds a lambda expression directly to an identifier."
///
/// Using a `def` statement leads to better tracebacks, and the assignment
/// itself negates the primary benefit of using a `lambda` expression (i.e.,
/// that it can be embedded inside another expression).
///
/// ## Example
/// ```python
/// f = lambda x: 2 * x
/// ```
///
/// Use instead:
/// ```python
/// def f(x):
///     return 2 * x
/// ```
///
/// ## Fix safety
/// This fix is marked as unsafe because converting a lambda assignment into a
/// function definition changes observable properties of the callable.
///
/// In particular, a lambda function has the name `"<lambda>"`, while the
/// generated function uses the name of the assigned variable. Code that relies
/// on function metadata, such as logging, registration, or introspection, may
/// therefore behave differently after the fix.
///
/// [PEP 8]: https://peps.python.org/pep-0008/#programming-recommendations
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "v0.0.28", category = Category::Pedantic)]
pub(crate) struct LambdaAssignment {
    name: String,
}

impl Violation for LambdaAssignment {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        "Do not assign a `lambda` expression, use a `def`".to_string()
    }

    fn fix_title(&self) -> Option<String> {
        let LambdaAssignment { name } = self;
        Some(format!("Rewrite `{name}` as a `def`"))
    }
}

/// E731
pub(crate) fn lambda_assignment(
    checker: &Checker,
    target: Expr<'_>,
    value: Expr<'_>,
    annotation: Option<Expr<'_>>,
    stmt: Stmt<'_>,
) {
    let ExprKind::Name(name_node) = target.classify() else {
        return;
    };
    let id = name_node.id();

    let ExprKind::Lambda(lambda) = value.classify() else {
        return;
    };

    // If the assignment is a class attribute (with an annotation), ignore it.
    //
    // This is most common for, e.g., dataclasses and Pydantic models. Those libraries will
    // treat the lambda as an assignable field, and the use of a lambda is almost certainly
    // intentional.
    if annotation.is_some() && checker.semantic().current_scope().kind.is_class() {
        return;
    }

    let mut diagnostic = checker.report_diagnostic(
        LambdaAssignment {
            name: id.to_string(),
        },
        stmt.range(),
    );

    if !has_leading_content(stmt.start(), checker.source())
        && !has_trailing_content(stmt.end(), checker.source())
    {
        let first_line = checker.locator().line_str(stmt.start());
        let indentation = leading_indentation(first_line);
        let mut indented = String::new();
        for (idx, line) in function(id, lambda, annotation, stmt, checker)
            .universal_newlines()
            .enumerate()
        {
            if idx == 0 {
                indented.push_str(&line);
            } else {
                indented.push_str(checker.stylist().line_ending().as_str());
                indented.push_str(indentation);
                indented.push_str(&line);
            }
        }

        // If the lambda is shadowing a variable in the current scope,
        // rewriting it as a function declaration may break type-checking.
        // See: https://github.com/astral-sh/ruff/issues/5421
        let applicability = if checker
            .semantic()
            .current_scope()
            .get_all(id)
            .any(|binding_id| checker.semantic().binding(binding_id).kind.is_annotation())
        {
            Applicability::DisplayOnly
        } else {
            Applicability::Unsafe
        };

        diagnostic.set_fix(Fix::applicable_edit(
            Edit::range_replacement(indented, stmt.range()),
            applicability,
        ));
    }
}

struct CallableTypes<'a> {
    arg_types: Option<NodeList<'a, ast::kind::Expr>>,
    return_type: Expr<'a>,
}

/// Extract the argument types and return type from a `Callable` annotation.
/// The `Callable` import can be from either `collections.abc` or `typing`.
/// A literal parameter list supplies the individual argument annotations.
fn extract_types<'a>(annotation: Expr<'a>, semantic: &SemanticModel) -> Option<CallableTypes<'a>> {
    let ExprKind::Subscript(subscript) = annotation.classify() else {
        return None;
    };
    let value = subscript.value();
    let slice = subscript.slice();
    let ExprKind::Tuple(tuple) = slice.classify() else {
        return None;
    };
    let elts = tuple.elts();
    let [param_types, return_type] = elts.as_array()?;

    if !semantic
        .resolve_qualified_name(value)
        .is_some_and(|qualified_name| {
            matches!(
                qualified_name.segments(),
                ["collections", "abc", "Callable"]
            ) || semantic.match_typing_qualified_name(&qualified_name, "Callable")
        })
    {
        return None;
    }

    // The first argument to `Callable` must be a list of types, parameter
    // specification (e.g., a `ParamSpec`), or ellipsis.
    // For parameter specifications, we cannot assign per-parameter annotations,
    // but we can still preserve the return type annotation.
    let params = param_types.as_list_expr().map(ast::ExprList::elts);
    Some(CallableTypes {
        arg_types: params,
        return_type,
    })
}

/// Generate a function definition from a `lambda` expression.
fn function(
    name: &str,
    lambda: ExprLambda<'_>,
    annotation: Option<Expr<'_>>,
    stmt: Stmt<'_>,
    checker: &Checker,
) -> String {
    let mut builder = Builder::default();
    let callable = annotation.and_then(|annotation| extract_types(annotation, checker.semantic()));
    let parameters = match (lambda.parameters(), callable.as_ref()) {
        (Some(parameters), Some(types)) => {
            annotated_parameters(&mut builder, parameters, types.arg_types)
        }
        (Some(parameters), None) => builder.copy_node(parameters),
        (None, _) => builder.add(build::Parameters::default()),
    };
    let returns = callable.map(|types| builder.copy_node(types.return_type));

    // A placeholder lets us preserve the source formatting of the lambda body.
    let ellipsis = builder.add(build::ExprEllipsisLiteral {
        range: TextRange::default(),
    });
    let return_stmt = builder.add(build::StmtReturn {
        range: TextRange::default(),
        value: Some(ellipsis.into()),
    });
    let body = builder.list([BuildId::<ast::kind::Stmt>::from(return_stmt)]);
    let name = syntax::identifier(&mut builder, name);
    let function = builder.add(build::StmtFunctionDef {
        is_async: false,
        name,
        parameters,
        body,
        decorator_list: ListId::default(),
        returns,
        type_params: None,
        range: TextRange::default(),
    });
    let generated = checker.generator().stmt(builder.get(function).into());
    replace_trailing_ellipsis_with_original_expr(generated, lambda, stmt, checker)
}

fn annotated_parameters(
    builder: &mut Builder,
    parameters: Parameters<'_>,
    arg_types: Option<NodeList<'_, ast::kind::Expr>>,
) -> BuildId<ast::kind::Parameters> {
    // Callable lists positional-only annotations first, followed by positional-or-keyword annotations.
    let mut annotations = arg_types.into_iter().flatten();
    let mut annotated = |builder: &mut Builder, arg: ParameterWithDefault<'_>| {
        let param = arg.parameter();
        let name = builder.copy_node(param.name());
        let annotation = annotations.next().map(|ty| builder.copy_node(ty));
        let parameter = builder.add(build::Parameter {
            range: param.range(),
            name,
            annotation,
        });
        let default = arg.default().map(|expr| builder.copy_node(expr));
        builder.add(build::ParameterWithDefault {
            range: arg.range(),
            parameter,
            default,
        })
    };
    let posonlyargs = builder.map_list(parameters.posonlyargs(), &mut annotated);
    let args = builder.map_list(parameters.args(), annotated);
    let vararg = parameters.vararg().map(|arg| builder.copy_node(arg));
    let kwonlyargs = builder.copy_nodes(parameters.kwonlyargs());
    let kwarg = parameters.kwarg().map(|arg| builder.copy_node(arg));
    builder.add(build::Parameters {
        range: parameters.range(),
        posonlyargs,
        args,
        vararg,
        kwonlyargs,
        kwarg,
    })
}
fn replace_trailing_ellipsis_with_original_expr(
    mut generated: String,
    lambda: ExprLambda<'_>,
    stmt: Stmt<'_>,
    checker: &Checker,
) -> String {
    let original_expr_range = parenthesized_range(lambda.body(), lambda.into(), checker.tokens())
        .unwrap_or(lambda.body().range());

    // This prevents the autofix of introducing a syntax error if the lambda's body is an
    // expression spanned across multiple lines. To avoid the syntax error we preserve
    // the parenthesis around the body.
    let original_expr_in_source =
        if parenthesized_range(lambda.into(), stmt.into(), checker.tokens()).is_some() {
            format!("({})", checker.locator().slice(original_expr_range))
        } else {
            checker.locator().slice(original_expr_range).to_string()
        };

    let placeholder_ellipsis_start = generated.rfind("...").unwrap();
    let placeholder_ellipsis_end = placeholder_ellipsis_start + "...".len();

    generated.replace_range(
        placeholder_ellipsis_start..placeholder_ellipsis_end,
        &original_expr_in_source,
    );
    generated
}
