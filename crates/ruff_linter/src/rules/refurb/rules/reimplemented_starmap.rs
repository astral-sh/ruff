use anyhow::{bail, Result};
use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::comparable::ComparableExpr;
use ruff_python_ast::helpers::any_over_expr;
use ruff_python_ast::name::Name;
use ruff_python_ast::{self as ast, Expr};
use ruff_text_size::{Ranged, TextRange};

use crate::checkers::ast::Checker;
use crate::importer::ImportRequest;

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
/// ## References
/// - [Python documentation: `itertools.starmap`](https://docs.python.org/3/library/itertools.html#itertools.starmap)
///
/// [PEP 709]: https://peps.python.org/pep-0709/
/// [#7771]: https://github.com/astral-sh/ruff/issues/7771
#[derive(ViolationMetadata)]
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
    let [comprehension] = target.generators() else {
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
            let [arg] = args else {
                return;
            };

            let Expr::Starred(ast::ExprStarred { value, .. }) = arg else {
                return;
            };

            if ComparableExpr::from(value.as_ref()) != ComparableExpr::from(name) {
                return;
            }

            // If the argument is used outside the function call, we can't replace it.
            if any_over_expr(func, &|expr| {
                expr.as_name_expr().is_some_and(|expr| expr.id == name.id)
            }) {
                return;
            }
        }
        // Ex) `f(x, y, z) for x, y, z in iter`
        ComprehensionTarget::Tuple(tuple) => {
            if tuple.len() != args.len()
                || std::iter::zip(tuple, args)
                    .any(|(x, y)| ComparableExpr::from(x) != ComparableExpr::from(y))
            {
                return;
            }

            // If any of the members are used outside the function call, we can't replace it.
            if any_over_expr(func, &|expr| {
                tuple
                    .iter()
                    .any(|elem| ComparableExpr::from(expr) == ComparableExpr::from(elem))
            }) {
                return;
            }
        }
    }

    let mut diagnostic = Diagnostic::new(ReimplementedStarmap, target.range());
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
            target.try_make_suggestion(
                Name::from(starmap_name),
                &comprehension.iter,
                func,
                checker,
            )?,
            target.range(),
        );
        Ok(Fix::safe_edits(import_edit, [main_edit]))
    });
    checker.report_diagnostic(diagnostic);
}

/// An enum for a node that can be considered a candidate for replacement with `starmap`.
#[derive(Debug)]
pub(crate) enum StarmapCandidate<'a> {
    Generator(&'a ast::ExprGenerator),
    ListComp(&'a ast::ExprListComp),
    SetComp(&'a ast::ExprSetComp),
}

impl<'a> From<&'a ast::ExprGenerator> for StarmapCandidate<'a> {
    fn from(generator: &'a ast::ExprGenerator) -> Self {
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
        name: Name,
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
                try_construct_call(name, iter, func, Name::new_static("list"), checker)
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
                try_construct_call(name, iter, func, Name::new_static("set"), checker)
            }
        }
    }
}

/// Try constructing the call to `itertools.starmap` and wrapping it with the given builtin.
fn try_construct_call(
    name: Name,
    iter: &Expr,
    func: &Expr,
    builtin: Name,
    checker: &Checker,
) -> Result<String> {
    // We can only do our fix if `builtin` identifier is still bound to
    // the built-in type.
    if !checker.semantic().has_builtin_binding(&builtin) {
        bail!("Can't use built-in `{builtin}` constructor")
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
fn construct_starmap_call(starmap_binding: Name, iter: &Expr, func: &Expr) -> ast::ExprCall {
    let starmap = ast::ExprName {
        id: starmap_binding,
        ctx: ast::ExprContext::Load,
        range: TextRange::default(),
    };
    ast::ExprCall {
        func: Box::new(starmap.into()),
        arguments: ast::Arguments {
            args: Box::from([func.clone(), iter.clone()]),
            keywords: Box::from([]),
            range: TextRange::default(),
        },
        range: TextRange::default(),
    }
}

/// Wrap given function call with yet another call.
fn wrap_with_call_to(call: ast::ExprCall, func_name: Name) -> ast::ExprCall {
    let name = ast::ExprName {
        id: func_name,
        ctx: ast::ExprContext::Load,
        range: TextRange::default(),
    };
    ast::ExprCall {
        func: Box::new(name.into()),
        arguments: ast::Arguments {
            args: Box::from([call.into()]),
            keywords: Box::from([]),
            range: TextRange::default(),
        },
        range: TextRange::default(),
    }
}

#[derive(Debug)]
enum ComprehensionTarget<'a> {
    /// E.g., `(x, y, z, ...)` in `(x, y, z, ...) in iter`.
    Tuple(&'a ast::ExprTuple),
    /// E.g., `x` in `x in iter`.
    Name(&'a ast::ExprName),
}

/// Extract the target from the comprehension (e.g., `(x, y, z)` in `(x, y, z, ...) in iter`).
fn match_comprehension_target(comprehension: &ast::Comprehension) -> Option<ComprehensionTarget> {
    if comprehension.is_async || !comprehension.ifs.is_empty() {
        return None;
    }
    match &comprehension.target {
        Expr::Tuple(tuple) => Some(ComprehensionTarget::Tuple(tuple)),
        Expr::Name(name) => Some(ComprehensionTarget::Name(name)),
        _ => None,
    }
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
