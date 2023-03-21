use itertools::Either::{Left, Right};
use std::collections::BTreeMap;
use std::iter;

use log::error;
use rustc_hash::FxHashSet;
use rustpython_parser::ast::{
    Boolop, Constant, Expr, ExprContext, ExprKind, Keyword, Stmt, StmtKind,
};

use ruff_diagnostics::{AlwaysAutofixableViolation, Violation};
use ruff_diagnostics::{Diagnostic, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::comparable::ComparableExpr;
use ruff_python_ast::helpers::{create_expr, match_trailing_comment, unparse_expr};
use ruff_python_ast::types::{Range, RefEquality};
use ruff_python_stdlib::identifiers::is_identifier;
use ruff_python_stdlib::keyword::KWLIST;

use crate::autofix::helpers::delete_stmt;
use crate::checkers::ast::Checker;
use crate::message::Location;
use crate::registry::AsRule;

use super::fixes;

#[violation]
pub struct UnnecessaryPass;

impl AlwaysAutofixableViolation for UnnecessaryPass {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Unnecessary `pass` statement")
    }

    fn autofix_title(&self) -> String {
        "Remove unnecessary `pass`".to_string()
    }
}

#[violation]
pub struct DuplicateClassFieldDefinition(pub String);

impl AlwaysAutofixableViolation for DuplicateClassFieldDefinition {
    #[derive_message_formats]
    fn message(&self) -> String {
        let DuplicateClassFieldDefinition(name) = self;
        format!("Class field `{name}` is defined multiple times")
    }

    fn autofix_title(&self) -> String {
        let DuplicateClassFieldDefinition(name) = self;
        format!("Remove duplicate field definition for `{name}`")
    }
}

#[violation]
pub struct NonUniqueEnums {
    pub value: String,
}

impl Violation for NonUniqueEnums {
    #[derive_message_formats]
    fn message(&self) -> String {
        let NonUniqueEnums { value } = self;
        format!("Enum contains duplicate value: `{value}`")
    }
}

/// ## What it does
/// Checks for unnecessary list comprehensions passed to `any` and `all`.
///
/// ## Why is this bad?
/// `any` and `all` take any iterators, including generators. Converting a generator to a list
/// by way of a list comprehension is unnecessary and reduces performance due to the
/// overhead of creating the list.
///
/// For example, compare the performance of `all` with a list comprehension against that
/// of a generator (~40x faster here):
///
/// ```console
/// In [1]: %timeit all([i for i in range(1000)])
/// 8.14 µs ± 25.4 ns per loop (mean ± std. dev. of 7 runs, 100,000 loops each)
///
/// In [2]: %timeit all(i for i in range(1000))
/// 212 ns ± 0.892 ns per loop (mean ± std. dev. of 7 runs, 1,000,000 loops each)
/// ```
///
/// ## Examples
/// ```python
/// any([x.id for x in bar])
/// all([x.id for x in bar])
/// ```
///
/// Use instead:
/// ```python
/// any(x.id for x in bar)
/// all(x.id for x in bar)
/// ```
#[violation]
pub struct UnnecessaryComprehensionAnyAll;

impl AlwaysAutofixableViolation for UnnecessaryComprehensionAnyAll {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Unnecessary list comprehension.")
    }

    fn autofix_title(&self) -> String {
        "Remove unnecessary list comprehension".to_string()
    }
}

#[violation]
pub struct UnnecessarySpread;

impl Violation for UnnecessarySpread {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Unnecessary spread `**`")
    }
}

#[violation]
pub struct MultipleStartsEndsWith {
    pub attr: String,
}

impl AlwaysAutofixableViolation for MultipleStartsEndsWith {
    #[derive_message_formats]
    fn message(&self) -> String {
        let MultipleStartsEndsWith { attr } = self;
        format!("Call `{attr}` once with a `tuple`")
    }

    fn autofix_title(&self) -> String {
        let MultipleStartsEndsWith { attr } = self;
        format!("Merge into a single `{attr}` call")
    }
}

#[violation]
pub struct UnnecessaryDictKwargs;

impl Violation for UnnecessaryDictKwargs {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Unnecessary `dict` kwargs")
    }
}

#[violation]
pub struct ReimplementedListBuiltin;

impl AlwaysAutofixableViolation for ReimplementedListBuiltin {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Prefer `list` over useless lambda")
    }

    fn autofix_title(&self) -> String {
        "Replace with `list`".to_string()
    }
}

/// PIE790
pub fn no_unnecessary_pass(checker: &mut Checker, body: &[Stmt]) {
    if body.len() > 1 {
        // This only catches the case in which a docstring makes a `pass` statement
        // redundant. Consider removing all `pass` statements instead.
        let docstring_stmt = &body[0];
        let pass_stmt = &body[1];
        let StmtKind::Expr { value } = &docstring_stmt.node else {
            return;
        };
        if matches!(
            value.node,
            ExprKind::Constant {
                value: Constant::Str(..),
                ..
            }
        ) {
            if matches!(pass_stmt.node, StmtKind::Pass) {
                let mut diagnostic = Diagnostic::new(UnnecessaryPass, Range::from(pass_stmt));
                if checker.patch(diagnostic.kind.rule()) {
                    if let Some(index) = match_trailing_comment(pass_stmt, checker.locator) {
                        diagnostic.amend(Fix::deletion(
                            pass_stmt.location,
                            Location::new(
                                pass_stmt.end_location.unwrap().row(),
                                pass_stmt.end_location.unwrap().column() + index,
                            ),
                        ));
                    } else {
                        match delete_stmt(
                            pass_stmt,
                            None,
                            &[],
                            checker.locator,
                            checker.indexer,
                            checker.stylist,
                        ) {
                            Ok(fix) => {
                                diagnostic.amend(fix);
                            }
                            Err(e) => {
                                error!("Failed to delete `pass` statement: {}", e);
                            }
                        }
                    }
                }
                checker.diagnostics.push(diagnostic);
            }
        }
    }
}

/// PIE794
pub fn duplicate_class_field_definition<'a, 'b>(
    checker: &mut Checker<'a>,
    parent: &'b Stmt,
    body: &'b [Stmt],
) where
    'b: 'a,
{
    let mut seen_targets: FxHashSet<&str> = FxHashSet::default();
    for stmt in body {
        // Extract the property name from the assignment statement.
        let target = match &stmt.node {
            StmtKind::Assign { targets, .. } => {
                if targets.len() != 1 {
                    continue;
                }
                if let ExprKind::Name { id, .. } = &targets[0].node {
                    id
                } else {
                    continue;
                }
            }
            StmtKind::AnnAssign { target, .. } => {
                if let ExprKind::Name { id, .. } = &target.node {
                    id
                } else {
                    continue;
                }
            }
            _ => continue,
        };

        if !seen_targets.insert(target) {
            let mut diagnostic = Diagnostic::new(
                DuplicateClassFieldDefinition(target.to_string()),
                Range::from(stmt),
            );
            if checker.patch(diagnostic.kind.rule()) {
                let deleted: Vec<&Stmt> = checker.deletions.iter().map(Into::into).collect();
                let locator = checker.locator;
                match delete_stmt(
                    stmt,
                    Some(parent),
                    &deleted,
                    locator,
                    checker.indexer,
                    checker.stylist,
                ) {
                    Ok(fix) => {
                        checker.deletions.insert(RefEquality(stmt));
                        diagnostic.amend(fix);
                    }
                    Err(err) => {
                        error!("Failed to remove duplicate class definition: {}", err);
                    }
                }
            }
            checker.diagnostics.push(diagnostic);
        }
    }
}

/// PIE796
pub fn non_unique_enums<'a, 'b>(checker: &mut Checker<'a>, parent: &'b Stmt, body: &'b [Stmt])
where
    'b: 'a,
{
    let StmtKind::ClassDef { bases, .. } = &parent.node else {
        return;
    };

    if !bases.iter().any(|expr| {
        checker
            .ctx
            .resolve_call_path(expr)
            .map_or(false, |call_path| call_path.as_slice() == ["enum", "Enum"])
    }) {
        return;
    }

    let mut seen_targets: FxHashSet<ComparableExpr> = FxHashSet::default();
    for stmt in body {
        let StmtKind::Assign { value, .. } = &stmt.node else {
            continue;
        };

        if let ExprKind::Call { func, .. } = &value.node {
            if checker
                .ctx
                .resolve_call_path(func)
                .map_or(false, |call_path| call_path.as_slice() == ["enum", "auto"])
            {
                continue;
            }
        }

        if !seen_targets.insert(ComparableExpr::from(value)) {
            let diagnostic = Diagnostic::new(
                NonUniqueEnums {
                    value: unparse_expr(value, checker.stylist),
                },
                Range::from(stmt),
            );
            checker.diagnostics.push(diagnostic);
        }
    }
}

/// PIE800
pub fn unnecessary_spread(checker: &mut Checker, keys: &[Option<Expr>], values: &[Expr]) {
    for item in keys.iter().zip(values.iter()) {
        if let (None, value) = item {
            // We only care about when the key is None which indicates a spread `**`
            // inside a dict.
            if let ExprKind::Dict { .. } = value.node {
                let diagnostic = Diagnostic::new(UnnecessarySpread, Range::from(value));
                checker.diagnostics.push(diagnostic);
            }
        }
    }
}

/// PIE802
pub fn unnecessary_comprehension_any_all(
    checker: &mut Checker,
    expr: &Expr,
    func: &Expr,
    args: &[Expr],
) {
    if let ExprKind::Name { id, .. } = &func.node {
        if (id == "all" || id == "any") && args.len() == 1 {
            if !checker.ctx.is_builtin(id) {
                return;
            }
            if let ExprKind::ListComp { .. } = args[0].node {
                let mut diagnostic =
                    Diagnostic::new(UnnecessaryComprehensionAnyAll, Range::from(&args[0]));
                if checker.patch(diagnostic.kind.rule()) {
                    match fixes::fix_unnecessary_comprehension_any_all(
                        checker.locator,
                        checker.stylist,
                        expr,
                    ) {
                        Ok(fix) => {
                            diagnostic.amend(fix);
                        }
                        Err(e) => error!("Failed to generate fix: {e}"),
                    }
                }
                checker.diagnostics.push(diagnostic);
            }
        }
    }
}

/// Return `true` if a key is a valid keyword argument name.
fn is_valid_kwarg_name(key: &Expr) -> bool {
    if let ExprKind::Constant {
        value: Constant::Str(value),
        ..
    } = &key.node
    {
        is_identifier(value) && !KWLIST.contains(&value.as_str())
    } else {
        false
    }
}

/// PIE804
pub fn unnecessary_dict_kwargs(checker: &mut Checker, expr: &Expr, kwargs: &[Keyword]) {
    for kw in kwargs {
        // keyword is a spread operator (indicated by None)
        if kw.node.arg.is_none() {
            if let ExprKind::Dict { keys, .. } = &kw.node.value.node {
                // ensure foo(**{"bar-bar": 1}) doesn't error
                if keys.iter().all(|expr| expr.as_ref().map_or(false, is_valid_kwarg_name)) ||
                    // handle case of foo(**{**bar})
                    (keys.len() == 1 && keys[0].is_none())
                {
                    let diagnostic = Diagnostic::new(UnnecessaryDictKwargs, Range::from(expr));
                    checker.diagnostics.push(diagnostic);
                }
            }
        }
    }
}

/// PIE810
pub fn multiple_starts_ends_with(checker: &mut Checker, expr: &Expr) {
    let ExprKind::BoolOp { op: Boolop::Or, values } = &expr.node else {
        return;
    };

    let mut duplicates = BTreeMap::new();
    for (index, call) in values.iter().enumerate() {
        let ExprKind::Call {
            func,
            args,
            keywords,
            ..
        } = &call.node else {
            continue
        };

        if !(args.len() == 1 && keywords.is_empty()) {
            continue;
        }

        let ExprKind::Attribute { value, attr, .. } = &func.node else {
            continue
        };

        if attr != "startswith" && attr != "endswith" {
            continue;
        }

        let ExprKind::Name { id: arg_name, .. } = &value.node else {
            continue
        };

        duplicates
            .entry((attr.as_str(), arg_name.as_str()))
            .or_insert_with(Vec::new)
            .push(index);
    }

    // Generate a `Diagnostic` for each duplicate.
    for ((attr_name, arg_name), indices) in duplicates {
        if indices.len() > 1 {
            let mut diagnostic = Diagnostic::new(
                MultipleStartsEndsWith {
                    attr: attr_name.to_string(),
                },
                Range::from(expr),
            );
            if checker.patch(diagnostic.kind.rule()) {
                let words: Vec<&Expr> = indices
                    .iter()
                    .map(|index| &values[*index])
                    .map(|expr| {
                        let ExprKind::Call { func: _, args, keywords: _} = &expr.node else {
                            unreachable!("{}", format!("Indices should only contain `{attr_name}` calls"))
                        };
                        args.get(0)
                            .unwrap_or_else(|| panic!("`{attr_name}` should have one argument"))
                    })
                    .collect();

                let call = create_expr(ExprKind::Call {
                    func: Box::new(create_expr(ExprKind::Attribute {
                        value: Box::new(create_expr(ExprKind::Name {
                            id: arg_name.to_string(),
                            ctx: ExprContext::Load,
                        })),
                        attr: attr_name.to_string(),
                        ctx: ExprContext::Load,
                    })),
                    args: vec![create_expr(ExprKind::Tuple {
                        elts: words
                            .iter()
                            .flat_map(|value| {
                                if let ExprKind::Tuple { elts, .. } = &value.node {
                                    Left(elts.iter())
                                } else {
                                    Right(iter::once(*value))
                                }
                            })
                            .map(Clone::clone)
                            .collect(),
                        ctx: ExprContext::Load,
                    })],
                    keywords: vec![],
                });

                // Generate the combined `BoolOp`.
                let mut call = Some(call);
                let bool_op = create_expr(ExprKind::BoolOp {
                    op: Boolop::Or,
                    values: values
                        .iter()
                        .enumerate()
                        .filter_map(|(index, elt)| {
                            if indices.contains(&index) {
                                std::mem::take(&mut call)
                            } else {
                                Some(elt.clone())
                            }
                        })
                        .collect(),
                });

                diagnostic.amend(Fix::replacement(
                    unparse_expr(&bool_op, checker.stylist),
                    expr.location,
                    expr.end_location.unwrap(),
                ));
            }
            checker.diagnostics.push(diagnostic);
        }
    }
}

/// PIE807
pub fn reimplemented_list_builtin(checker: &mut Checker, expr: &Expr) {
    let ExprKind::Lambda { args, body } = &expr.node else {
        unreachable!("Expected ExprKind::Lambda");
    };
    if args.args.is_empty()
        && args.kwonlyargs.is_empty()
        && args.posonlyargs.is_empty()
        && args.vararg.is_none()
        && args.kwarg.is_none()
    {
        if let ExprKind::List { elts, .. } = &body.node {
            if elts.is_empty() {
                let mut diagnostic = Diagnostic::new(ReimplementedListBuiltin, Range::from(expr));
                if checker.patch(diagnostic.kind.rule()) {
                    diagnostic.amend(Fix::replacement(
                        "list".to_string(),
                        expr.location,
                        expr.end_location.unwrap(),
                    ));
                }
                checker.diagnostics.push(diagnostic);
            }
        }
    }
}
