use anyhow::{Result, bail};
use ruff_diagnostics::Applicability;
use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::comparable::ComparableExpr;
use ruff_python_ast::helpers::any_over_expr;
use ruff_python_ast::{self as ast, Builder, Expr, ExprKind, NodeList};
use ruff_text_size::{Ranged, TextRange};

use crate::checkers::ast::Checker;
use crate::codes::Category;
use crate::fix::syntax;
use crate::importer::ImportRequest;
use crate::{Edit, Fix, FixAvailability, Violation};

/// ## What it does
/// Checks for generator expressions, list and set comprehensions that can
/// be replaced with `itertools.starmap`.
///
/// ## Why is this bad?
/// When unpacking values from iterators to pass them directly to
/// a function, prefer `itertools.starmap`.
///
/// Using `itertools.starmap` is more concise and readable. Furthermore, it is
/// more efficient than generator expressions, and in some versions of Python,
/// it is more efficient than comprehensions.
///
/// ## Known problems
/// Since Python 3.12, `itertools.starmap` is less efficient than
/// comprehensions ([#7771]). This is due to [PEP 709], which made
/// comprehensions faster.
///
/// ## Example
/// ```python
/// all(predicate(a, b) for a, b in some_iterable)
/// ```
///
/// Use instead:
/// ```python
/// from itertools import starmap
///
///
/// all(starmap(predicate, some_iterable))
/// ```
///
/// ## Fix safety
/// This rule's fix is marked as safe, unless the expression contains comments.
///
/// ## References
/// - [Python documentation: `itertools.starmap`](https://docs.python.org/3/library/itertools.html#itertools.starmap)
///
/// [PEP 709]: https://peps.python.org/pep-0709/
/// [#7771]: https://github.com/astral-sh/ruff/issues/7771
#[derive(ViolationMetadata)]
#[violation_metadata(preview_since = "v0.0.291", category = Category::Pedantic)]
pub(crate) struct ReimplementedStarmap;

impl Violation for ReimplementedStarmap {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        "Use `itertools.starmap` instead of the generator".to_string()
    }

    fn fix_title(&self) -> Option<String> {
        Some("Replace with `itertools.starmap`".to_string())
    }
}

/// FURB140
pub(crate) fn reimplemented_starmap(checker: &Checker, target: &StarmapCandidate) {
    // Generator should have exactly one comprehension.
    let Some([comprehension]) = target.generators().as_array() else {
        return;
    };

    // This comprehension should have a form:
    // ```python
    // (x, y, z, ...) in iter
    // ```
    //
    // `x, y, z, ...` are what we call `elts` for short.
    let Some(value) = match_comprehension_target(comprehension) else {
        return;
    };

    // Generator should produce one element that should look like:
    // ```python
    // func(a, b, c, ...)
    // ```
    //
    // here we refer to `a, b, c, ...` as `args`.
    //
    // NOTE: `func` is not necessarily just a function name, it can be an attribute access,
    //       or even a call itself.
    let Some((args, func)) = match_call(target.element()) else {
        return;
    };

    match value {
        // Ex) `f(*x) for x in iter`
        ComprehensionTarget::Name(name) => {
            let Some([arg]) = args.as_array() else {
                return;
            };

            let ExprKind::Starred(starred) = arg.classify() else {
                return;
            };
            let value = starred.value();

            if ComparableExpr::from(value) != ComparableExpr::from(name) {
                return;
            }

            // If the argument is used outside the function call, we can't replace it.
            if any_over_expr(func, |expr| {
                expr.as_name_expr()
                    .is_some_and(|expr| expr.id() == name.id())
            }) {
                return;
            }
        }
        // Ex) `f(x, y, z) for x, y, z in iter`
        ComprehensionTarget::Tuple(tuple) => {
            if tuple.elts().len() != args.len()
                || std::iter::zip(tuple, args)
                    .any(|(x, y)| ComparableExpr::from(x) != ComparableExpr::from(y))
            {
                return;
            }

            // If any of the members are used outside the function call, we can't replace it.
            if any_over_expr(func, |expr| {
                tuple
                    .iter()
                    .any(|elem| ComparableExpr::from(expr) == ComparableExpr::from(elem))
            }) {
                return;
            }
        }
    }

    let mut diagnostic = checker.report_diagnostic(ReimplementedStarmap, target.range());
    diagnostic.try_set_fix(|| {
        // Import `starmap` from `itertools`.
        let (import_edit, starmap_name) = checker.importer().get_or_import_symbol(
            &ImportRequest::import_from("itertools", "starmap"),
            target.start(),
            checker.semantic(),
        )?;
        // The actual fix suggestion depends on what type of expression we were looking at:
        // - For generator expressions, we use `starmap` call directly.
        // - For list and set comprehensions, we'd want to wrap it with `list` and `set`
        //   correspondingly.
        let main_edit = Edit::range_replacement(
            target.try_make_suggestion(&starmap_name, comprehension.iter(), func, checker)?,
            target.range(),
        );

        let applicability = if checker.comment_ranges().intersects(target.range()) {
            Applicability::Unsafe
        } else {
            Applicability::Safe
        };

        Ok(Fix::applicable_edits(
            import_edit,
            [main_edit],
            applicability,
        ))
    });
}

/// An enum for a node that can be considered a candidate for replacement with `starmap`.
#[derive(Debug)]
pub(crate) enum StarmapCandidate<'a> {
    Generator(ast::ExprGenerator<'a>),
    ListComp(ast::ExprListComp<'a>),
    SetComp(ast::ExprSetComp<'a>),
}

impl<'a> From<ast::ExprGenerator<'a>> for StarmapCandidate<'a> {
    fn from(generator: ast::ExprGenerator<'a>) -> Self {
        Self::Generator(generator)
    }
}

impl<'a> From<ast::ExprListComp<'a>> for StarmapCandidate<'a> {
    fn from(list_comp: ast::ExprListComp<'a>) -> Self {
        Self::ListComp(list_comp)
    }
}

impl<'a> From<ast::ExprSetComp<'a>> for StarmapCandidate<'a> {
    fn from(set_comp: ast::ExprSetComp<'a>) -> Self {
        Self::SetComp(set_comp)
    }
}

impl Ranged for StarmapCandidate<'_> {
    fn range(&self) -> TextRange {
        match self {
            Self::Generator(generator) => generator.range(),
            Self::ListComp(list_comp) => list_comp.range(),
            Self::SetComp(set_comp) => set_comp.range(),
        }
    }
}

impl StarmapCandidate<'_> {
    /// Return the generated element for the candidate.
    fn element(&self) -> Expr<'_> {
        match self {
            Self::Generator(generator) => generator.elt(),
            Self::ListComp(list_comp) => list_comp.elt(),
            Self::SetComp(set_comp) => set_comp.elt(),
        }
    }

    /// Return the generator comprehensions for the candidate.
    fn generators(&self) -> NodeList<'_, ast::kind::Comprehension> {
        match self {
            Self::Generator(generator) => generator.generators(),
            Self::ListComp(list_comp) => list_comp.generators(),
            Self::SetComp(set_comp) => set_comp.generators(),
        }
    }

    /// Generator expressions become `starmap(func, iter)`. List and set comprehensions wrap the call
    /// in `list(...)` or `set(...)` to preserve the result type.
    fn try_make_suggestion(
        &self,
        starmap_name: &str,
        iter: Expr<'_>,
        func: Expr<'_>,
        checker: &Checker,
    ) -> Result<String> {
        let wrapper = match self {
            Self::Generator(_) => None,
            Self::ListComp(_) => Some("list"),
            Self::SetComp(_) => Some("set"),
        };
        if let Some(builtin) = wrapper
            && !checker.semantic().has_builtin_binding(builtin)
        {
            bail!("Can't use built-in `{builtin}` constructor");
        }

        let mut builder = Builder::default();
        let args = builder.copy_nodes([func, iter]);
        let starmap = syntax::call_name(&mut builder, starmap_name, args);
        let replacement = if let Some(wrapper) = wrapper {
            let args = builder.list([starmap]);
            syntax::call_name(&mut builder, wrapper, args)
        } else {
            starmap
        };
        Ok(checker.generator().expr(builder.get(replacement)))
    }
}
#[derive(Debug)]
enum ComprehensionTarget<'a> {
    /// E.g., `(x, y, z, ...)` in `(x, y, z, ...) in iter`.
    Tuple(ast::ExprTuple<'a>),
    /// E.g., `x` in `x in iter`.
    Name(ast::ExprName<'a>),
}

/// Extract the target from the comprehension (e.g., `(x, y, z)` in `(x, y, z, ...) in iter`).
fn match_comprehension_target(
    comprehension: ast::Comprehension<'_>,
) -> Option<ComprehensionTarget<'_>> {
    if comprehension.is_async() || !comprehension.ifs().is_empty() {
        return None;
    }
    match comprehension.target().classify() {
        ExprKind::Tuple(tuple) => Some(ComprehensionTarget::Tuple(tuple)),
        ExprKind::Name(name) => Some(ComprehensionTarget::Name(name)),
        _ => None,
    }
}

/// Match that the given expression is `func(x, y, z, ...)`.
fn match_call(element: Expr<'_>) -> Option<(NodeList<'_, ast::kind::Expr>, Expr<'_>)> {
    let expr_call = element.as_call_expr()?;
    let func = expr_call.func();
    let arguments = expr_call.arguments();
    let args = arguments.args();
    let keywords = arguments.keywords();

    if !keywords.is_empty() {
        return None;
    }

    Some((args, func))
}
