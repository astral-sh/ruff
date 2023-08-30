use anyhow::{bail, Result};
use ruff_diagnostics::{AutofixKind, Diagnostic, Edit, Fix, Violation};
use ruff_macros::{derive_message_formats, violation};
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

/// A abstract node that can be considered a candidate for replacement with `starmap`.
pub(crate) trait StarmapEquivalent: Ranged {
    /// Get generated element.
    fn element(&self) -> &Expr;
    /// Get generator comprehensions.
    fn generators(&self) -> &[ast::Comprehension];
    /// Try to produce a fix suggestion transforming this node into a call to `starmap`.
    fn try_make_suggestion(
        name: String,
        iter: &Expr,
        func: &Expr,
        checker: &Checker,
    ) -> Result<String>;
}

// FURB140
pub(crate) fn reimplemented_starmap<T: StarmapEquivalent>(checker: &mut Checker, generator: &T) {
    // Generator should have exactly one comprehension.
    let [comprehension @ ast::Comprehension { .. }] = generator.generators() else {
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
    let Some((args, func)) = match_call(generator.element()) else {
        return;
    };

    // Here we want to check that `args` and `elts` are the same (same length, same elements,
    // same order).
    if elts.len() != args.len()
        || !std::iter::zip(elts, args)
            // We intentionally do not use ComparableExpr here because it will compare expression
            // contexts and in `elts` it's definitely `Load`, while in `args` it's always `Store`.
            //
            // For this reason, we compare names directly.
            .all(|(x, y)| get_name_id(x) == get_name_id(y))
    {
        return;
    }

    let mut diagnostic = Diagnostic::new(ReimplementedStarmap, generator.range());
    if checker.patch(diagnostic.kind.rule()) {
        diagnostic.try_set_fix(|| {
            // Try importing `starmap` from `itertools`.
            //
            // It is not required to be `itertools.starmap`, though. The user might've already
            // imported it. Maybe even under a different name. So, we should use that name
            // for fix construction.
            let (import_edit, starmap_name) = checker.importer().get_or_import_symbol(
                &ImportRequest::import_from("itertools", "starmap"),
                generator.start(),
                checker.semantic(),
            )?;
            // The actual fix suggestion depends on what type of expression we were looking at.
            //
            // - For generator expressions, we use `starmap` call directly.
            // - For list and set comprehensions, we'd want to wrap it with `list` and `set`
            //   correspondingly.
            let main_edit = Edit::range_replacement(
                T::try_make_suggestion(starmap_name, iter, func, checker)?,
                generator.range(),
            );
            Ok(Fix::suggested_edits(import_edit, [main_edit]))
        });
    }
    checker.diagnostics.push(diagnostic);
}

#[inline]
fn get_name_id(expr: &Expr) -> Option<&str> {
    Some(&expr.as_name_expr()?.id)
}

impl StarmapEquivalent for ast::ExprGeneratorExp {
    fn element(&self) -> &Expr {
        self.elt.as_ref()
    }
    fn generators(&self) -> &[ast::Comprehension] {
        self.generators.as_slice()
    }
    fn try_make_suggestion(
        name: String,
        iter: &Expr,
        func: &Expr,
        checker: &Checker,
    ) -> Result<String> {
        // For generator expressions, we replace
        // ```python
        // (foo(...) for ... in iter)
        // ```
        //
        // with
        // ```python
        // itertools.starmap(foo, iter)
        // ```
        let call = construct_starmap_call(name, iter, func);
        Ok(checker.generator().expr(&call.into()))
    }
}

impl StarmapEquivalent for ast::ExprListComp {
    fn element(&self) -> &Expr {
        self.elt.as_ref()
    }
    fn generators(&self) -> &[ast::Comprehension] {
        self.generators.as_slice()
    }
    fn try_make_suggestion(
        name: String,
        iter: &Expr,
        func: &Expr,
        checker: &Checker,
    ) -> Result<String> {
        // For list comprehensions, we replace
        // ```python
        // [foo(...) for ... in iter]
        // ```
        //
        // with
        // ```python
        // list(itertools.starmap(foo, iter))
        // ```
        try_construct_call(name, iter, func, "list", checker)
    }
}

impl StarmapEquivalent for ast::ExprSetComp {
    fn element(&self) -> &Expr {
        self.elt.as_ref()
    }
    fn generators(&self) -> &[ast::Comprehension] {
        self.generators.as_slice()
    }
    fn try_make_suggestion(
        name: String,
        iter: &Expr,
        func: &Expr,
        checker: &Checker,
    ) -> Result<String> {
        // For set comprehensions, we replace
        // ```python
        // {foo(...) for ... in iter}
        // ```
        //
        // with
        // ```python
        // set(itertools.starmap(foo, iter))
        // ```
        try_construct_call(name, iter, func, "set", checker)
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

    // In general, we replace
    // ```python
    // foo(...) for ... in iter
    // ```
    //
    // with
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
