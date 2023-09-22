use anyhow::{bail, Result};
use ruff_diagnostics::{AutofixKind, Diagnostic, Edit, Fix, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::comparable::ComparableExpr;
use ruff_python_ast::{self as ast, Expr};
use ruff_text_size::{Ranged, TextRange};

use crate::checkers::ast::Checker;
use crate::importer::ImportRequest;
use crate::registry::AsRule;

/// ## What it does
/// Checks for generator expressions, list and set comprehensions that can
/// be replaced with `itertools.starmap`.
///
/// ## Why is this bad?
/// When unpacking values from iterators to pass them directly to
/// a function, prefer `itertools.starmap`.
///
/// Using `itertools.starmap` is more concise and readable.
///
/// ## Example
/// ```python
/// scores = [85, 100, 60]
/// passing_scores = [60, 80, 70]
///
///
/// def passed_test(score: int, passing_score: int) -> bool:
///     return score >= passing_score
///
///
/// passed_all_tests = all(
///     passed_test(score, passing_score)
///     for score, passing_score in zip(scores, passing_scores)
/// )
/// ```
///
/// Use instead:
/// ```python
/// from itertools import starmap
///
///
/// scores = [85, 100, 60]
/// passing_scores = [60, 80, 70]
///
///
/// def passed_test(score: int, passing_score: int) -> bool:
///     return score >= passing_score
///
///
/// passed_all_tests = all(starmap(passed_test, zip(scores, passing_scores)))
/// ```
///
/// ## References
/// - [Python documentation: `itertools.starmap`](https://docs.python.org/3/library/itertools.html#itertools.starmap)
#[violation]
pub struct ReimplementedStarmap;

impl Violation for ReimplementedStarmap {
    const AUTOFIX: AutofixKind = AutofixKind::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Use `itertools.starmap` instead of the generator")
    }

    fn autofix_title(&self) -> Option<String> {
        Some(format!("Replace with `itertools.starmap`"))
    }
}

/// FURB140
pub(crate) fn reimplemented_starmap(checker: &mut Checker, target: &StarmapCandidate) {
    // Generator should have exactly one comprehension.
    let [comprehension @ ast::Comprehension { .. }] = target.generators() else {
        return;
    };

    // This comprehension should have a form:
    // ```python
    // (x, y, z, ...) in iter
    // ```
    //
    // `x, y, z, ...` are what we call `elts` for short.
    let Some((elts, iter)) = match_comprehension(comprehension) else {
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

    // Here we want to check that `args` and `elts` are the same (same length, same elements,
    // same order).
    if elts.len() != args.len()
        || !std::iter::zip(elts, args)
            .all(|(x, y)| ComparableExpr::from(x) == ComparableExpr::from(y))
    {
        return;
    }

    let mut diagnostic = Diagnostic::new(ReimplementedStarmap, target.range());
    if checker.patch(diagnostic.kind.rule()) {
        diagnostic.try_set_fix(|| {
            // Try importing `starmap` from `itertools`.
            //
            // It is not required to be `itertools.starmap`, though. The user might've already
            // imported it. Maybe even under a different name. So, we should use that name
            // for fix construction.
            let (import_edit, starmap_name) = checker.importer().get_or_import_symbol(
                &ImportRequest::import_from("itertools", "starmap"),
                target.start(),
                checker.semantic(),
            )?;
            // The actual fix suggestion depends on what type of expression we were looking at.
            //
            // - For generator expressions, we use `starmap` call directly.
            // - For list and set comprehensions, we'd want to wrap it with `list` and `set`
            //   correspondingly.
            let main_edit = Edit::range_replacement(
                target.try_make_suggestion(starmap_name, iter, func, checker)?,
                target.range(),
            );
            Ok(Fix::suggested_edits(import_edit, [main_edit]))
        });
    }
    checker.diagnostics.push(diagnostic);
}

/// An enum for a node that can be considered a candidate for replacement with `starmap`.
#[derive(Debug)]
pub(crate) enum StarmapCandidate<'a> {
    Generator(&'a ast::ExprGeneratorExp),
    ListComp(&'a ast::ExprListComp),
    SetComp(&'a ast::ExprSetComp),
}

impl<'a> From<&'a ast::ExprGeneratorExp> for StarmapCandidate<'a> {
    fn from(generator: &'a ast::ExprGeneratorExp) -> Self {
        Self::Generator(generator)
    }
}

impl<'a> From<&'a ast::ExprListComp> for StarmapCandidate<'a> {
    fn from(list_comp: &'a ast::ExprListComp) -> Self {
        Self::ListComp(list_comp)
    }
}

impl<'a> From<&'a ast::ExprSetComp> for StarmapCandidate<'a> {
    fn from(set_comp: &'a ast::ExprSetComp) -> Self {
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
    pub(crate) fn element(&self) -> &Expr {
        match self {
            Self::Generator(generator) => generator.elt.as_ref(),
            Self::ListComp(list_comp) => list_comp.elt.as_ref(),
            Self::SetComp(set_comp) => set_comp.elt.as_ref(),
        }
    }

    /// Return the generator comprehensions for the candidate.
    pub(crate) fn generators(&self) -> &[ast::Comprehension] {
        match self {
            Self::Generator(generator) => generator.generators.as_slice(),
            Self::ListComp(list_comp) => list_comp.generators.as_slice(),
            Self::SetComp(set_comp) => set_comp.generators.as_slice(),
        }
    }

    /// Try to produce a fix suggestion transforming this node into a call to `starmap`.
    pub(crate) fn try_make_suggestion(
        &self,
        name: String,
        iter: &Expr,
        func: &Expr,
        checker: &Checker,
    ) -> Result<String> {
        match self {
            Self::Generator(_) => {
                // For generator expressions, we replace:
                // ```python
                // (foo(...) for ... in iter)
                // ```
                //
                // with:
                // ```python
                // itertools.starmap(foo, iter)
                // ```
                let call = construct_starmap_call(name, iter, func);
                Ok(checker.generator().expr(&call.into()))
            }
            Self::ListComp(_) => {
                // For list comprehensions, we replace:
                // ```python
                // [foo(...) for ... in iter]
                // ```
                //
                // with:
                // ```python
                // list(itertools.starmap(foo, iter))
                // ```
                try_construct_call(name, iter, func, "list", checker)
            }
            Self::SetComp(_) => {
                // For set comprehensions, we replace:
                // ```python
                // {foo(...) for ... in iter}
                // ```
                //
                // with:
                // ```python
                // set(itertools.starmap(foo, iter))
                // ```
                try_construct_call(name, iter, func, "set", checker)
            }
        }
    }
}

/// Try constructing the call to `itertools.starmap` and wrapping it with the given builtin.
fn try_construct_call(
    name: String,
    iter: &Expr,
    func: &Expr,
    builtin: &str,
    checker: &Checker,
) -> Result<String> {
    // We can only do our fix if `builtin` identifier is still bound to
    // the built-in type.
    if !checker.semantic().is_builtin(builtin) {
        bail!(format!("Can't use built-in `{builtin}` constructor"))
    }

    // In general, we replace:
    // ```python
    // foo(...) for ... in iter
    // ```
    //
    // with:
    // ```python
    // builtin(itertools.starmap(foo, iter))
    // ```
    // where `builtin` is a constructor for a target collection.
    let call = construct_starmap_call(name, iter, func);
    let wrapped = wrap_with_call_to(call, builtin);
    Ok(checker.generator().expr(&wrapped.into()))
}

/// Construct the call to `itertools.starmap` for suggestion.
fn construct_starmap_call(starmap_binding: String, iter: &Expr, func: &Expr) -> ast::ExprCall {
    let starmap = ast::ExprName {
        id: starmap_binding,
        ctx: ast::ExprContext::Load,
        range: TextRange::default(),
    };
    ast::ExprCall {
        func: Box::new(starmap.into()),
        arguments: ast::Arguments {
            args: vec![func.clone(), iter.clone()],
            keywords: vec![],
            range: TextRange::default(),
        },
        range: TextRange::default(),
    }
}

/// Wrap given function call with yet another call.
fn wrap_with_call_to(call: ast::ExprCall, func_name: &str) -> ast::ExprCall {
    let name = ast::ExprName {
        id: func_name.to_string(),
        ctx: ast::ExprContext::Load,
        range: TextRange::default(),
    };
    ast::ExprCall {
        func: Box::new(name.into()),
        arguments: ast::Arguments {
            args: vec![call.into()],
            keywords: vec![],
            range: TextRange::default(),
        },
        range: TextRange::default(),
    }
}

/// Match that the given comprehension is `(x, y, z, ...) in iter`.
fn match_comprehension(comprehension: &ast::Comprehension) -> Option<(&[Expr], &Expr)> {
    if comprehension.is_async || !comprehension.ifs.is_empty() {
        return None;
    }

    let ast::ExprTuple { elts, .. } = comprehension.target.as_tuple_expr()?;
    Some((elts, &comprehension.iter))
}

/// Match that the given expression is `func(x, y, z, ...)`.
fn match_call(element: &Expr) -> Option<(&[Expr], &Expr)> {
    let ast::ExprCall {
        func,
        arguments: ast::Arguments { args, keywords, .. },
        ..
    } = element.as_call_expr()?;

    if !keywords.is_empty() {
        return None;
    }

    Some((args, func))
}
