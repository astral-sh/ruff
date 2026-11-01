use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::{self as ast, Expr, ExprKind, NodeList};
use ruff_python_semantic::analyze::typing;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::codes::Category;
use crate::{AlwaysFixableViolation, Fix};

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
/// {(1, 2): 3, (4, 5): 6}
/// >>> dict(d1.keys())        # Correct fix
/// {1: 2, 4: 5}
/// ```
///
/// When the comprehension iterates over a sequence, Ruff's suggested fix is correct. However, Ruff
/// cannot consistently infer if the iterable type is a sequence or a mapping and cannot suggest
/// the correct fix for mappings.
///
/// Additionally, rewriting comprehensions inside an object's `__len__` method may cause a
/// `RecursionError`, as collection constructors can call back into `__len__`.
///
/// ## Fix safety
/// This rule's fix is always marked as unsafe because of the known problems described above and
/// because comments may be dropped when rewriting the comprehension.
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "v0.0.73", category = Category::Pedantic)]
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
fn add_diagnostic(checker: &Checker, expr: Expr<'_>) {
    let Some(comprehension_kind) = ComprehensionKind::try_from_expr(expr) else {
        return;
    };
    if !checker
        .semantic()
        .has_builtin_binding(comprehension_kind.as_str())
    {
        return;
    }
    let mut diagnostic = checker.report_diagnostic(
        UnnecessaryComprehension {
            kind: comprehension_kind,
        },
        expr.range(),
    );
    diagnostic.try_set_fix(|| {
        fixes::fix_unnecessary_comprehension(expr, checker.locator(), checker.stylist())
            .map(Fix::unsafe_edit)
    });
}

/// C416
pub(crate) fn unnecessary_dict_comprehension(
    checker: &Checker,
    &expr: &Expr<'_>,
    key: Expr<'_>,
    value: Expr<'_>,
    generators: NodeList<'_, ruff_python_ast::kind::Comprehension>,
) {
    let Some([generator]) = generators.as_array() else {
        return;
    };
    if !generator.ifs().is_empty() || generator.is_async() {
        return;
    }
    let ExprKind::Tuple(tuple) = generator.target().classify() else {
        return;
    };
    let elts = tuple.elts();
    let Some(
        [
            ast::ExprFields::Name(ast::ExprNameFields { id: target_key, .. }),
            ast::ExprFields::Name(ast::ExprNameFields {
                id: target_value, ..
            }),
        ],
    ) = elts.as_array().map(|items| items.map(Expr::fields))
    else {
        return;
    };
    let ExprKind::Name(name_node) = key.classify() else {
        return;
    };
    let key = name_node.id();
    let ExprKind::Name(name_node) = value.classify() else {
        return;
    };
    let value = name_node.id();

    if *target_key == *key && *target_value == *value {
        add_diagnostic(checker, expr);
    }
}

/// C416
pub(crate) fn unnecessary_list_set_comprehension(
    checker: &Checker,
    &expr: &Expr<'_>,
    elt: Expr<'_>,
    generators: NodeList<'_, ruff_python_ast::kind::Comprehension>,
) {
    let Some([generator]) = generators.as_array() else {
        return;
    };
    if !generator.ifs().is_empty() || generator.is_async() {
        return;
    }
    if is_dict_items(checker, generator.iter()) {
        match (generator.target().fields(), elt.fields()) {
            // [(k, v) for k, v in dict.items()] or [(k, v) for [k, v] in dict.items()]
            (
                ast::ExprFields::Tuple(ast::ExprTupleFields {
                    elts: target_elts, ..
                })
                | ast::ExprFields::List(ast::ExprListFields {
                    elts: target_elts, ..
                }),
                ast::ExprFields::Tuple(ast::ExprTupleFields { elts, .. }),
            ) => {
                let Some(
                    [
                        ast::ExprFields::Name(ast::ExprNameFields { id: target_key, .. }),
                        ast::ExprFields::Name(ast::ExprNameFields {
                            id: target_value, ..
                        }),
                    ],
                ) = target_elts.as_array().map(|items| items.map(Expr::fields))
                else {
                    return;
                };
                let Some(
                    [
                        ast::ExprFields::Name(ast::ExprNameFields { id: key, .. }),
                        ast::ExprFields::Name(ast::ExprNameFields { id: value, .. }),
                    ],
                ) = elts.as_array().map(|items| items.map(Expr::fields))
                else {
                    return;
                };
                if target_key == key && target_value == value {
                    add_diagnostic(checker, expr);
                }
            }
            // [x for x in dict.items()]
            (
                ast::ExprFields::Name(ast::ExprNameFields {
                    id: target_name, ..
                }),
                ast::ExprFields::Name(ast::ExprNameFields { id: elt_name, .. }),
            ) if target_name == elt_name => {
                add_diagnostic(checker, expr);
            }
            _ => {}
        }
    } else {
        // [x for x in iterable]
        let ExprKind::Name(name_node) = generator.target().classify() else {
            return;
        };
        let target_name = name_node.id();
        let ExprKind::Name(name_node) = elt.classify() else {
            return;
        };
        let elt_name = name_node.id();
        if elt_name == target_name {
            add_diagnostic(checker, expr);
        }
    }
}

fn is_dict_items(checker: &Checker, expr: Expr<'_>) -> bool {
    let ExprKind::Call(call_node) = expr.classify() else {
        return false;
    };
    let func = call_node.func();

    let ExprKind::Attribute(attribute) = func.classify() else {
        return false;
    };
    let value = attribute.value();
    let attr = attribute.attr();

    if attr.as_str() != "items" {
        return false;
    }

    let ExprKind::Name(name) = value.classify() else {
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

    fn try_from_expr(expr: Expr<'_>) -> Option<Self> {
        match expr.classify() {
            ExprKind::ListComp(_) => Some(Self::List),
            ExprKind::DictComp(_) => Some(Self::Dict),
            ExprKind::SetComp(_) => Some(Self::Set),
            _ => None,
        }
    }
}

impl std::fmt::Display for ComprehensionKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}
