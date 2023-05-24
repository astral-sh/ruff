use std::collections::BTreeMap;
use std::iter;

use itertools::Either::{Left, Right};
use log::error;
use ruff_text_size::TextRange;
use rustc_hash::FxHashSet;
use rustpython_parser::ast::{
    self, Boolop, Constant, Expr, ExprContext, ExprLambda, Keyword, Ranged, Stmt,
};

use ruff_diagnostics::{AlwaysAutofixableViolation, Violation};
use ruff_diagnostics::{Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::comparable::ComparableExpr;
use ruff_python_ast::helpers::trailing_comment_start_offset;
use ruff_python_ast::types::RefEquality;
use ruff_python_stdlib::identifiers::is_identifier;

use crate::autofix::actions::delete_stmt;
use crate::checkers::ast::Checker;
use crate::registry::AsRule;

/// ## What it does
/// Checks for unnecessary `pass` statements in class and function bodies.
/// where it is not needed syntactically (e.g., when an indented docstring is
/// present).
///
/// ## Why is this bad?
/// When a function or class definition contains a docstring, an additional
/// `pass` statement is redundant.
///
/// ## Example
/// ```python
/// def foo():
///     """Placeholder docstring."""
///     pass
/// ```
///
/// Use instead:
/// ```python
/// def foo():
///     """Placeholder docstring."""
/// ```
///
/// ## References
/// - [Python documentation](https://docs.python.org/3/reference/simple_stmts.html#the-pass-statement)
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

/// ## What it does
/// Checks for duplicate field definitions in classes.
///
/// ## Why is this bad?
/// Defining a field multiple times in a class body is redundant and likely a
/// mistake.
///
/// ## Example
/// ```python
/// class Person:
///     name = Tom
///     ...
///     name = Ben
/// ```
///
/// Use instead:
/// ```python
/// class Person:
///     name = Tom
///     ...
/// ```
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

/// ## What it does
/// Checks for enums that contain duplicate values.
///
/// ## Why is this bad?
/// Enum values should be unique. Non-unique values are redundant and likely a
/// mistake.
///
/// ## Example
/// ```python
/// from enum import Enum
///
///
/// class Foo(Enum):
///     A = 1
///     B = 2
///     C = 1
/// ```
///
/// Use instead:
/// ```python
/// from enum import Enum
///
///
/// class Foo(Enum):
///     A = 1
///     B = 2
///     C = 3
/// ```
///
/// ## References
/// - [Python documentation](https://docs.python.org/3/library/enum.html#enum.Enum)
#[violation]
pub struct NonUniqueEnums {
    value: String,
}

impl Violation for NonUniqueEnums {
    #[derive_message_formats]
    fn message(&self) -> String {
        let NonUniqueEnums { value } = self;
        format!("Enum contains duplicate value: `{value}`")
    }
}

/// ## What it does
/// Checks for unnecessary dictionary unpacking operators (`**`).
///
/// ## Why is this bad?
/// Unpacking a dictionary into another dictionary is redundant. The unpacking
/// operator can be removed, making the code more readable.
///
/// ## Example
/// ```python
/// foo = {"A": 1, "B": 2}
/// bar = {**foo, **{"C": 3}}
/// ```
///
/// Use instead:
/// ```python
/// foo = {"A": 1, "B": 2}
/// bar = {**foo, "C": 3}
/// ```
///
/// ## References
/// - [Python documentation](https://docs.python.org/3/reference/expressions.html#dictionary-displays)
#[violation]
pub struct UnnecessarySpread;

impl Violation for UnnecessarySpread {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Unnecessary spread `**`")
    }
}

/// ## What it does
/// Checks for `startswith` or `endswith` calls on the same value with
/// different prefixes or suffixes.
///
/// ## Why is this bad?
/// The `startswith` and `endswith` methods accept tuples of prefixes or
/// suffixes respectively. Passing a tuple of prefixes or suffixes is more
/// more efficient and readable than calling the method multiple times.
///
/// ## Example
/// ```python
/// msg = "Hello, world!"
/// if msg.startswith("Hello") or msg.startswith("Hi"):
///     print("Greetings!")
/// ```
///
/// Use instead:
/// ```python
/// msg = "Hello, world!"
/// if msg.startswith(("Hello", "Hi")):
///     print("Greetings!")
/// ```
///
/// ## References
/// - [Python documentation](https://docs.python.org/3/library/stdtypes.html#str.startswith)
/// - [Python documentation](https://docs.python.org/3/library/stdtypes.html#str.endswith)
#[violation]
pub struct MultipleStartsEndsWith {
    attr: String,
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

/// ## What it does
/// Checks for unnecessary `dict` kwargs.
///
/// ## Why is this bad?
/// If the `dict` keys are valid identifiers, they can be passed as keyword
/// arguments directly.
///
/// ## Example
/// ```python
/// def foo(bar):
///     return bar + 1
///
///
/// print(foo(**{"bar": 2}))  # prints 3
/// ```
///
/// Use instead:
/// ```python
/// def foo(bar):
///     return bar + 1
///
///
/// print(foo(bar=2))  # prints 3
/// ```
///
/// ## References
/// - [Python documentation](https://docs.python.org/3/reference/expressions.html#dictionary-displays)
/// - [Python documentation](https://docs.python.org/3/reference/expressions.html#calls)
#[violation]
pub struct UnnecessaryDictKwargs;

impl Violation for UnnecessaryDictKwargs {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Unnecessary `dict` kwargs")
    }
}

/// ## What it does
/// Checks for lambdas that can be replaced with the `list` builtin.
///
/// ## Why is this bad?
/// Using `list` builtin is more readable.
///
/// ## Example
/// ```python
/// from dataclasses import dataclass, field
///
///
/// @dataclass
/// class Foo:
///     bar: list[int] = field(default_factory=lambda: [])
/// ```
///
/// Use instead:
/// ```python
/// from dataclasses import dataclass, field
///
///
/// @dataclass
/// class Foo:
///     bar: list[int] = field(default_factory=list)
/// ```
///
/// ## References
/// - [Python documentation](https://docs.python.org/3/library/functions.html#func-list)
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
pub(crate) fn no_unnecessary_pass(checker: &mut Checker, body: &[Stmt]) {
    if body.len() > 1 {
        // This only catches the case in which a docstring makes a `pass` statement
        // redundant. Consider removing all `pass` statements instead.
        let docstring_stmt = &body[0];
        let pass_stmt = &body[1];
        let Stmt::Expr(ast::StmtExpr { value, range: _ } )= docstring_stmt else {
            return;
        };
        if matches!(
            value.as_ref(),
            Expr::Constant(ast::ExprConstant {
                value: Constant::Str(..),
                ..
            })
        ) {
            if pass_stmt.is_pass_stmt() {
                let mut diagnostic = Diagnostic::new(UnnecessaryPass, pass_stmt.range());
                if checker.patch(diagnostic.kind.rule()) {
                    if let Some(index) = trailing_comment_start_offset(pass_stmt, checker.locator) {
                        diagnostic.set_fix(Fix::automatic(Edit::range_deletion(
                            pass_stmt.range().add_end(index),
                        )));
                    } else {
                        #[allow(deprecated)]
                        diagnostic.try_set_fix_from_edit(|| {
                            delete_stmt(
                                pass_stmt,
                                None,
                                &[],
                                checker.locator,
                                checker.indexer,
                                checker.stylist,
                            )
                        });
                    }
                }
                checker.diagnostics.push(diagnostic);
            }
        }
    }
}

/// PIE794
pub(crate) fn duplicate_class_field_definition<'a, 'b>(
    checker: &mut Checker<'a>,
    parent: &'b Stmt,
    body: &'b [Stmt],
) where
    'b: 'a,
{
    let mut seen_targets: FxHashSet<&str> = FxHashSet::default();
    for stmt in body {
        // Extract the property name from the assignment statement.
        let target = match stmt {
            Stmt::Assign(ast::StmtAssign { targets, .. }) => {
                if targets.len() != 1 {
                    continue;
                }
                if let Expr::Name(ast::ExprName { id, .. }) = &targets[0] {
                    id
                } else {
                    continue;
                }
            }
            Stmt::AnnAssign(ast::StmtAnnAssign { target, .. }) => {
                if let Expr::Name(ast::ExprName { id, .. }) = target.as_ref() {
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
                stmt.range(),
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
                        #[allow(deprecated)]
                        diagnostic.set_fix_from_edit(fix);
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
pub(crate) fn non_unique_enums<'a, 'b>(
    checker: &mut Checker<'a>,
    parent: &'b Stmt,
    body: &'b [Stmt],
) where
    'b: 'a,
{
    let Stmt::ClassDef(ast::StmtClassDef { bases, .. }) = parent else {
        return;
    };

    if !bases.iter().any(|expr| {
        checker
            .semantic_model()
            .resolve_call_path(expr)
            .map_or(false, |call_path| call_path.as_slice() == ["enum", "Enum"])
    }) {
        return;
    }

    let mut seen_targets: FxHashSet<ComparableExpr> = FxHashSet::default();
    for stmt in body {
        let Stmt::Assign(ast::StmtAssign { value, .. }) = stmt else {
            continue;
        };

        if let Expr::Call(ast::ExprCall { func, .. }) = value.as_ref() {
            if checker
                .semantic_model()
                .resolve_call_path(func)
                .map_or(false, |call_path| call_path.as_slice() == ["enum", "auto"])
            {
                continue;
            }
        }

        if !seen_targets.insert(ComparableExpr::from(value)) {
            let diagnostic = Diagnostic::new(
                NonUniqueEnums {
                    value: checker.generator().expr(value),
                },
                stmt.range(),
            );
            checker.diagnostics.push(diagnostic);
        }
    }
}

/// PIE800
pub(crate) fn unnecessary_spread(checker: &mut Checker, keys: &[Option<Expr>], values: &[Expr]) {
    for item in keys.iter().zip(values.iter()) {
        if let (None, value) = item {
            // We only care about when the key is None which indicates a spread `**`
            // inside a dict.
            if let Expr::Dict(_) = value {
                let diagnostic = Diagnostic::new(UnnecessarySpread, value.range());
                checker.diagnostics.push(diagnostic);
            }
        }
    }
}

/// Return `true` if a key is a valid keyword argument name.
fn is_valid_kwarg_name(key: &Expr) -> bool {
    if let Expr::Constant(ast::ExprConstant {
        value: Constant::Str(value),
        ..
    }) = key
    {
        is_identifier(value)
    } else {
        false
    }
}

/// PIE804
pub(crate) fn unnecessary_dict_kwargs(checker: &mut Checker, expr: &Expr, kwargs: &[Keyword]) {
    for kw in kwargs {
        // keyword is a spread operator (indicated by None)
        if kw.arg.is_none() {
            if let Expr::Dict(ast::ExprDict { keys, .. }) = &kw.value {
                // ensure foo(**{"bar-bar": 1}) doesn't error
                if keys.iter().all(|expr| expr.as_ref().map_or(false, is_valid_kwarg_name)) ||
                    // handle case of foo(**{**bar})
                    (keys.len() == 1 && keys[0].is_none())
                {
                    let diagnostic = Diagnostic::new(UnnecessaryDictKwargs, expr.range());
                    checker.diagnostics.push(diagnostic);
                }
            }
        }
    }
}

/// PIE810
pub(crate) fn multiple_starts_ends_with(checker: &mut Checker, expr: &Expr) {
    let Expr::BoolOp(ast::ExprBoolOp { op: Boolop::Or, values, range: _ }) = expr else {
        return;
    };

    let mut duplicates = BTreeMap::new();
    for (index, call) in values.iter().enumerate() {
        let Expr::Call(ast::ExprCall {
            func,
            args,
            keywords,
            range: _
        }) = &call else {
            continue
        };

        if !(args.len() == 1 && keywords.is_empty()) {
            continue;
        }

        let Expr::Attribute(ast::ExprAttribute { value, attr, .. } )= func.as_ref() else {
            continue
        };
        if attr != "startswith" && attr != "endswith" {
            continue;
        }

        let Expr::Name(ast::ExprName { id: arg_name, .. } )= value.as_ref() else {
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
                expr.range(),
            );
            if checker.patch(diagnostic.kind.rule()) {
                let words: Vec<&Expr> = indices
                    .iter()
                    .map(|index| &values[*index])
                    .map(|expr| {
                        let Expr::Call(ast::ExprCall { func: _, args, keywords: _, range: _}) = expr else {
                            unreachable!("{}", format!("Indices should only contain `{attr_name}` calls"))
                        };
                        args.get(0)
                            .unwrap_or_else(|| panic!("`{attr_name}` should have one argument"))
                    })
                    .collect();

                let node = Expr::Tuple(ast::ExprTuple {
                    elts: words
                        .iter()
                        .flat_map(|value| {
                            if let Expr::Tuple(ast::ExprTuple { elts, .. }) = value {
                                Left(elts.iter())
                            } else {
                                Right(iter::once(*value))
                            }
                        })
                        .map(Clone::clone)
                        .collect(),
                    ctx: ExprContext::Load,
                    range: TextRange::default(),
                });
                let node1 = Expr::Name(ast::ExprName {
                    id: arg_name.into(),
                    ctx: ExprContext::Load,
                    range: TextRange::default(),
                });
                let node2 = Expr::Attribute(ast::ExprAttribute {
                    value: Box::new(node1),
                    attr: attr_name.into(),
                    ctx: ExprContext::Load,
                    range: TextRange::default(),
                });
                let node3 = Expr::Call(ast::ExprCall {
                    func: Box::new(node2),
                    args: vec![node],
                    keywords: vec![],
                    range: TextRange::default(),
                });
                let call = node3;

                // Generate the combined `BoolOp`.
                let mut call = Some(call);
                let node = Expr::BoolOp(ast::ExprBoolOp {
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
                    range: TextRange::default(),
                });
                let bool_op = node;
                diagnostic.set_fix(Fix::suggested(Edit::range_replacement(
                    checker.generator().expr(&bool_op),
                    expr.range(),
                )));
            }
            checker.diagnostics.push(diagnostic);
        }
    }
}

/// PIE807
pub(crate) fn reimplemented_list_builtin(checker: &mut Checker, expr: &ExprLambda) {
    let ExprLambda {
        args,
        body,
        range: _,
    } = expr;

    if args.args.is_empty()
        && args.kwonlyargs.is_empty()
        && args.posonlyargs.is_empty()
        && args.vararg.is_none()
        && args.kwarg.is_none()
    {
        if let Expr::List(ast::ExprList { elts, .. }) = body.as_ref() {
            if elts.is_empty() {
                let mut diagnostic = Diagnostic::new(ReimplementedListBuiltin, expr.range());
                if checker.patch(diagnostic.kind.rule()) {
                    diagnostic.set_fix(Fix::automatic(Edit::range_replacement(
                        "list".to_string(),
                        expr.range(),
                    )));
                }
                checker.diagnostics.push(diagnostic);
            }
        }
    }
}
