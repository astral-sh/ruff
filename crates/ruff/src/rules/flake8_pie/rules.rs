use log::error;
use ruff_macros::{define_violation, derive_message_formats};
use ruff_python::identifiers::is_identifier;
use ruff_python::keyword::KWLIST;
use rustc_hash::FxHashSet;
use rustpython_parser::ast::{Boolop, Constant, Expr, ExprKind, Keyword, Stmt, StmtKind};

use crate::ast::comparable::ComparableExpr;
use crate::ast::helpers::{match_trailing_comment, unparse_expr};
use crate::ast::types::{Range, RefEquality};
use crate::autofix::helpers::delete_stmt;
use crate::checkers::ast::Checker;
use crate::fix::Fix;
use crate::message::Location;
use crate::registry::Diagnostic;
use crate::violation::{AlwaysAutofixableViolation, Violation};

define_violation!(
    pub struct NoUnnecessaryPass;
);
impl AlwaysAutofixableViolation for NoUnnecessaryPass {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Unnecessary `pass` statement")
    }

    fn autofix_title(&self) -> String {
        "Remove unnecessary `pass`".to_string()
    }
}

define_violation!(
    pub struct DupeClassFieldDefinitions(pub String);
);
impl AlwaysAutofixableViolation for DupeClassFieldDefinitions {
    #[derive_message_formats]
    fn message(&self) -> String {
        let DupeClassFieldDefinitions(name) = self;
        format!("Class field `{name}` is defined multiple times")
    }

    fn autofix_title(&self) -> String {
        let DupeClassFieldDefinitions(name) = self;
        format!("Remove duplicate field definition for `{name}`")
    }
}

define_violation!(
    pub struct PreferUniqueEnums {
        pub value: String,
    }
);
impl Violation for PreferUniqueEnums {
    #[derive_message_formats]
    fn message(&self) -> String {
        let PreferUniqueEnums { value } = self;
        format!("Enum contains duplicate value: `{value}`")
    }
}

define_violation!(
    pub struct NoUnnecessarySpread;
);
impl Violation for NoUnnecessarySpread {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Unnecessary spread `**`")
    }
}

define_violation!(
    pub struct SingleStartsEndsWith {
        pub attr: String,
    }
);
impl Violation for SingleStartsEndsWith {
    #[derive_message_formats]
    fn message(&self) -> String {
        let SingleStartsEndsWith { attr } = self;
        format!("Call `{attr}` once with a `tuple`")
    }
}

define_violation!(
    pub struct NoUnnecessaryDictKwargs;
);
impl Violation for NoUnnecessaryDictKwargs {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Unnecessary `dict` kwargs")
    }
}

define_violation!(
    pub struct PreferListBuiltin;
);
impl AlwaysAutofixableViolation for PreferListBuiltin {
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
                let mut diagnostic =
                    Diagnostic::new(NoUnnecessaryPass, Range::from_located(pass_stmt));
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
pub fn dupe_class_field_definitions<'a, 'b>(
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
                DupeClassFieldDefinitions(target.to_string()),
                Range::from_located(stmt),
            );
            if checker.patch(diagnostic.kind.rule()) {
                let deleted: Vec<&Stmt> = checker
                    .deletions
                    .iter()
                    .map(std::convert::Into::into)
                    .collect();
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
pub fn prefer_unique_enums<'a, 'b>(checker: &mut Checker<'a>, parent: &'b Stmt, body: &'b [Stmt])
where
    'b: 'a,
{
    let StmtKind::ClassDef { bases, .. } = &parent.node else {
        return;
    };

    if !bases.iter().any(|expr| {
        checker
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
                .resolve_call_path(func)
                .map_or(false, |call_path| call_path.as_slice() == ["enum", "auto"])
            {
                continue;
            }
        }

        if !seen_targets.insert(ComparableExpr::from(value)) {
            let diagnostic = Diagnostic::new(
                PreferUniqueEnums {
                    value: unparse_expr(value, checker.stylist),
                },
                Range::from_located(stmt),
            );
            checker.diagnostics.push(diagnostic);
        }
    }
}

/// PIE800
pub fn no_unnecessary_spread(checker: &mut Checker, keys: &[Option<Expr>], values: &[Expr]) {
    for item in keys.iter().zip(values.iter()) {
        if let (None, value) = item {
            // We only care about when the key is None which indicates a spread `**`
            // inside a dict.
            if let ExprKind::Dict { .. } = value.node {
                let diagnostic = Diagnostic::new(NoUnnecessarySpread, Range::from_located(value));
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
pub fn no_unnecessary_dict_kwargs(checker: &mut Checker, expr: &Expr, kwargs: &[Keyword]) {
    for kw in kwargs {
        // keyword is a spread operator (indicated by None)
        if kw.node.arg.is_none() {
            if let ExprKind::Dict { keys, .. } = &kw.node.value.node {
                // ensure foo(**{"bar-bar": 1}) doesn't error
                if keys.iter().all(|expr| expr.as_ref().map_or(false, is_valid_kwarg_name)) ||
                    // handle case of foo(**{**bar})
                    (keys.len() == 1 && keys[0].is_none())
                {
                    let diagnostic =
                        Diagnostic::new(NoUnnecessaryDictKwargs, Range::from_located(expr));
                    checker.diagnostics.push(diagnostic);
                }
            }
        }
    }
}

/// PIE810
pub fn single_starts_ends_with(checker: &mut Checker, values: &[Expr], node: &Boolop) {
    if *node != Boolop::Or {
        return;
    }

    // Given `foo.startswith`, insert ("foo", "startswith") into the set.
    let mut seen = FxHashSet::default();
    for expr in values {
        if let ExprKind::Call {
            func,
            args,
            keywords,
            ..
        } = &expr.node
        {
            if !(args.len() == 1 && keywords.is_empty()) {
                continue;
            }
            if let ExprKind::Attribute { value, attr, .. } = &func.node {
                if attr != "startswith" && attr != "endswith" {
                    continue;
                }
                if let ExprKind::Name { id, .. } = &value.node {
                    if !seen.insert((id, attr)) {
                        checker.diagnostics.push(Diagnostic::new(
                            SingleStartsEndsWith {
                                attr: attr.to_string(),
                            },
                            Range::from_located(value),
                        ));
                    }
                }
            }
        }
    }
}

/// PIE807
pub fn prefer_list_builtin(checker: &mut Checker, expr: &Expr) {
    let ExprKind::Lambda { args, body } = &expr.node else {
        unreachable!("Expected ExprKind::Lambda");
    };
    if args.args.is_empty() {
        if let ExprKind::List { elts, .. } = &body.node {
            if elts.is_empty() {
                let mut diagnostic = Diagnostic::new(PreferListBuiltin, Range::from_located(expr));
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
