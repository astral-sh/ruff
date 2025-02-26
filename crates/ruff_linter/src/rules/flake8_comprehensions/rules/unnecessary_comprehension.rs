use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Fix};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::{self as ast, Comprehension, Expr};
use ruff_python_semantic::analyze::typing;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

use crate::rules::flake8_comprehensions::fixes;

/// ## What it does
/// Checks for unnecessary dict, list, and set comprehension.
///
/// ## Why is this bad?
/// It's unnecessary to use a dict/list/set comprehension to build a data structure if the
/// elements are unchanged. Wrap the iterable with `dict()`, `list()`, or `set()` instead.
///
/// ## Example
/// ```python
/// {a: b for a, b in iterable}
/// [x for x in iterable]
/// {x for x in iterable}
/// ```
///
/// Use instead:
/// ```python
/// dict(iterable)
/// list(iterable)
/// set(iterable)
/// ```
///
/// ## Known problems
///
/// This rule may produce false positives for dictionary comprehensions that iterate over a mapping.
/// The dict constructor behaves differently depending on if it receives a sequence (e.g., a
/// list) or a mapping (e.g., a dict). When a comprehension iterates over the keys of a mapping,
/// replacing it with a `dict()` constructor call will give a different result.
///
/// For example:
///
/// ```pycon
/// >>> d1 = {(1, 2): 3, (4, 5): 6}
/// >>> {x: y for x, y in d1}  # Iterates over the keys of a mapping
/// {1: 2, 4: 5}
/// >>> dict(d1)               # Ruff's incorrect suggested fix
/// (1, 2): 3, (4, 5): 6}
/// >>> dict(d1.keys())        # Correct fix
/// {1: 2, 4: 5}
/// ```
///
/// When the comprehension iterates over a sequence, Ruff's suggested fix is correct. However, Ruff
/// cannot consistently infer if the iterable type is a sequence or a mapping and cannot suggest
/// the correct fix for mappings.
///
/// ## Fix safety
/// Due to the known problem with dictionary comprehensions, this fix is marked as unsafe.
///
/// Additionally, this fix may drop comments when rewriting the comprehension.
#[derive(ViolationMetadata)]
pub(crate) struct UnnecessaryComprehension {
    kind: ComprehensionKind,
}

impl AlwaysFixableViolation for UnnecessaryComprehension {
    #[derive_message_formats]
    fn message(&self) -> String {
        let UnnecessaryComprehension { kind } = self;
        format!("Unnecessary {kind} comprehension (rewrite using `{kind}()`)")
    }

    fn fix_title(&self) -> String {
        let UnnecessaryComprehension { kind } = self;
        format!("Rewrite using `{kind}()`")
    }
}

/// Add diagnostic for C416 based on the expression node id.
fn add_diagnostic(checker: &Checker, expr: &Expr) {
    let Some(comprehension_kind) = ComprehensionKind::try_from_expr(expr) else {
        return;
    };
    if !checker
        .semantic()
        .has_builtin_binding(comprehension_kind.as_str())
    {
        return;
    }
    let mut diagnostic = Diagnostic::new(
        UnnecessaryComprehension {
            kind: comprehension_kind,
        },
        expr.range(),
    );
    diagnostic.try_set_fix(|| {
        fixes::fix_unnecessary_comprehension(expr, checker.locator(), checker.stylist())
            .map(Fix::unsafe_edit)
    });
    checker.report_diagnostic(diagnostic);
}

/// C416
pub(crate) fn unnecessary_dict_comprehension(
    checker: &Checker,
    expr: &Expr,
    key: &Expr,
    value: &Expr,
    generators: &[Comprehension],
) {
    let [generator] = generators else {
        return;
    };
    if !generator.ifs.is_empty() || generator.is_async {
        return;
    }
    let Expr::Tuple(ast::ExprTuple { elts, .. }) = &generator.target else {
        return;
    };
    let [Expr::Name(ast::ExprName { id: target_key, .. }), Expr::Name(ast::ExprName {
        id: target_value, ..
    })] = elts.as_slice()
    else {
        return;
    };
    let Expr::Name(ast::ExprName { id: key, .. }) = &key else {
        return;
    };
    let Expr::Name(ast::ExprName { id: value, .. }) = &value else {
        return;
    };

    if target_key == key && target_value == value {
        add_diagnostic(checker, expr);
    }
}

/// C416
pub(crate) fn unnecessary_list_set_comprehension(
    checker: &Checker,
    expr: &Expr,
    elt: &Expr,
    generators: &[Comprehension],
) {
    let [generator] = generators else {
        return;
    };
    if !generator.ifs.is_empty() || generator.is_async {
        return;
    }
    if is_dict_items(checker, &generator.iter) {
        match (&generator.target, elt) {
            // [(k, v) for k, v in dict.items()] or [(k, v) for [k, v] in dict.items()]
            (
                Expr::Tuple(ast::ExprTuple {
                    elts: target_elts, ..
                })
                | Expr::List(ast::ExprList {
                    elts: target_elts, ..
                }),
                Expr::Tuple(ast::ExprTuple { elts, .. }),
            ) => {
                let [Expr::Name(ast::ExprName { id: target_key, .. }), Expr::Name(ast::ExprName {
                    id: target_value, ..
                })] = target_elts.as_slice()
                else {
                    return;
                };
                let [Expr::Name(ast::ExprName { id: key, .. }), Expr::Name(ast::ExprName { id: value, .. })] =
                    elts.as_slice()
                else {
                    return;
                };
                if target_key == key && target_value == value {
                    add_diagnostic(checker, expr);
                }
            }
            // [x for x in dict.items()]
            (
                Expr::Name(ast::ExprName {
                    id: target_name, ..
                }),
                Expr::Name(ast::ExprName { id: elt_name, .. }),
            ) if target_name == elt_name => {
                add_diagnostic(checker, expr);
            }
            _ => {}
        }
    } else {
        // [x for x in iterable]
        let Expr::Name(ast::ExprName {
            id: target_name, ..
        }) = &generator.target
        else {
            return;
        };
        let Expr::Name(ast::ExprName { id: elt_name, .. }) = &elt else {
            return;
        };
        if elt_name == target_name {
            add_diagnostic(checker, expr);
        }
    }
}

fn is_dict_items(checker: &Checker, expr: &Expr) -> bool {
    let Expr::Call(ast::ExprCall { func, .. }) = expr else {
        return false;
    };

    let Expr::Attribute(ast::ExprAttribute { value, attr, .. }) = func.as_ref() else {
        return false;
    };

    if attr.as_str() != "items" {
        return false;
    }

    let Expr::Name(name) = value.as_ref() else {
        return false;
    };

    let Some(id) = checker.semantic().resolve_name(name) else {
        return false;
    };

    typing::is_dict(checker.semantic().binding(id), checker.semantic())
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
enum ComprehensionKind {
    List,
    Set,
    Dict,
}

impl ComprehensionKind {
    const fn as_str(self) -> &'static str {
        match self {
            Self::List => "list",
            Self::Dict => "dict",
            Self::Set => "set",
        }
    }

    const fn try_from_expr(expr: &Expr) -> Option<Self> {
        match expr {
            Expr::ListComp(_) => Some(Self::List),
            Expr::DictComp(_) => Some(Self::Dict),
            Expr::SetComp(_) => Some(Self::Set),
            _ => None,
        }
    }
}

impl std::fmt::Display for ComprehensionKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}
