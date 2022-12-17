use once_cell::sync::Lazy;
use regex::Regex;
use rustc_hash::{FxHashMap, FxHashSet};
use rustpython_ast::{Constant, KeywordData, Location};
use rustpython_parser::ast::{ArgData, Expr, ExprKind, Stmt, StmtKind};

use crate::ast::helpers;
use crate::ast::types::{Binding, BindingKind, Range, Scope, ScopeKind};
use crate::autofix::Fix;
use crate::checks::{Check, CheckKind};
use crate::pyupgrade::types::Primitive;
use crate::settings::types::PythonVersion;

/// UP001
pub fn useless_metaclass_type(targets: &[Expr], value: &Expr, location: Range) -> Option<Check> {
    if targets.len() != 1 {
        return None;
    }
    let ExprKind::Name { id, .. } = targets.first().map(|expr| &expr.node).unwrap() else {
        return None;
    };
    if id != "__metaclass__" {
        return None;
    }
    let ExprKind::Name { id, .. } = &value.node else {
        return None;
    };
    if id != "type" {
        return None;
    }
    Some(Check::new(CheckKind::UselessMetaclassType, location))
}

/// UP003
pub fn type_of_primitive(func: &Expr, args: &[Expr], location: Range) -> Option<Check> {
    // Validate the arguments.
    if args.len() != 1 {
        return None;
    }

    let (ExprKind::Attribute { attr: id, .. } | ExprKind::Name { id, .. }) = &func.node else {
        return None;
    };
    if id != "type" {
        return None;
    }

    let ExprKind::Constant { value, .. } = &args[0].node else {
        return None;
    };

    let primitive = Primitive::from_constant(value)?;
    Some(Check::new(CheckKind::TypeOfPrimitive(primitive), location))
}

/// UP004
pub fn useless_object_inheritance(
    name: &str,
    bases: &[Expr],
    scope: &Scope,
    bindings: &[Binding],
) -> Option<Check> {
    for expr in bases {
        let ExprKind::Name { id, .. } = &expr.node else {
            continue;
        };
        if id != "object" {
            continue;
        }
        if !matches!(
            scope
                .values
                .get(&id.as_str())
                .map(|index| &bindings[*index]),
            None | Some(Binding {
                kind: BindingKind::Builtin,
                ..
            })
        ) {
            continue;
        }
        return Some(Check::new(
            CheckKind::UselessObjectInheritance(name.to_string()),
            Range::from_located(expr),
        ));
    }

    None
}

/// UP008
pub fn super_args(
    scope: &Scope,
    parents: &[&Stmt],
    expr: &Expr,
    func: &Expr,
    args: &[Expr],
) -> Option<Check> {
    if !helpers::is_super_call_with_arguments(func, args) {
        return None;
    }

    // Check: are we in a Function scope?
    if !matches!(scope.kind, ScopeKind::Function { .. }) {
        return None;
    }

    let mut parents = parents.iter().rev();

    // For a `super` invocation to be unnecessary, the first argument needs to match
    // the enclosing class, and the second argument needs to match the first
    // argument to the enclosing function.
    let [first_arg, second_arg] = args else {
        return None;
    };

    // Find the enclosing function definition (if any).
    let Some(StmtKind::FunctionDef {
        args: parent_args, ..
    }) = parents
        .find(|stmt| matches!(stmt.node, StmtKind::FunctionDef { .. }))
        .map(|stmt| &stmt.node) else {
        return None;
    };

    // Extract the name of the first argument to the enclosing function.
    let Some(ArgData {
        arg: parent_arg, ..
    }) = parent_args.args.first().map(|expr| &expr.node) else {
        return None;
    };

    // Find the enclosing class definition (if any).
    let Some(StmtKind::ClassDef {
        name: parent_name, ..
    }) = parents
        .find(|stmt| matches!(stmt.node, StmtKind::ClassDef { .. }))
        .map(|stmt| &stmt.node) else {
        return None;
    };

    let (
        ExprKind::Name {
            id: first_arg_id, ..
        },
        ExprKind::Name {
            id: second_arg_id, ..
        },
    ) = (&first_arg.node, &second_arg.node) else {
        return None;
    };

    if first_arg_id == parent_name && second_arg_id == parent_arg {
        return Some(Check::new(
            CheckKind::SuperCallWithParameters,
            Range::from_located(expr),
        ));
    }

    None
}

// Regex from PEP263.
static CODING_COMMENT_REGEX: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^[ \t\f]*#.*?coding[:=][ \t]*utf-?8").unwrap());

/// UP009
pub fn unnecessary_coding_comment(lineno: usize, line: &str, autofix: bool) -> Option<Check> {
    // PEP3120 makes utf-8 the default encoding.
    if CODING_COMMENT_REGEX.is_match(line) {
        let mut check = Check::new(
            CheckKind::PEP3120UnnecessaryCodingComment,
            Range {
                location: Location::new(lineno + 1, 0),
                end_location: Location::new(lineno + 2, 0),
            },
        );
        if autofix {
            check.amend(Fix::deletion(
                Location::new(lineno + 1, 0),
                Location::new(lineno + 2, 0),
            ));
        }
        Some(check)
    } else {
        None
    }
}

/// UP011
pub fn unnecessary_lru_cache_params(
    decorator_list: &[Expr],
    target_version: PythonVersion,
    from_imports: &FxHashMap<&str, FxHashSet<&str>>,
    import_aliases: &FxHashMap<&str, &str>,
) -> Option<Check> {
    for expr in decorator_list.iter() {
        let ExprKind::Call {
            func,
            args,
            keywords,
        } = &expr.node
        else {
            continue;
        };

        if !(args.is_empty()
            && helpers::match_module_member(
                func,
                "functools",
                "lru_cache",
                from_imports,
                import_aliases,
            ))
        {
            continue;
        }

        let range = Range {
            location: func.end_location.unwrap(),
            end_location: expr.end_location.unwrap(),
        };
        // Ex) `functools.lru_cache()`
        if keywords.is_empty() {
            return Some(Check::new(CheckKind::UnnecessaryLRUCacheParams, range));
        }
        // Ex) `functools.lru_cache(maxsize=None)`
        if !(target_version >= PythonVersion::Py39 && keywords.len() == 1) {
            continue;
        }

        let KeywordData { arg, value } = &keywords[0].node;
        if !(arg.as_ref().map(|arg| arg == "maxsize").unwrap_or_default()
            && matches!(
                value.node,
                ExprKind::Constant {
                    value: Constant::None,
                    kind: None,
                }
            ))
        {
            continue;
        }
        return Some(Check::new(CheckKind::UnnecessaryLRUCacheParams, range));
    }
    None
}
