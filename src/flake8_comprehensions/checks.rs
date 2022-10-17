use num_bigint::BigInt;
use rustpython_ast::{Comprehension, Constant, Expr, ExprKind, KeywordData, Located, Unaryop};

use crate::ast::types::Range;
use crate::checks::{Check, CheckKind};

/// Check `list(generator)` compliance.
pub fn unnecessary_generator_list(expr: &Expr, func: &Expr, args: &[Expr]) -> Option<Check> {
    if args.len() == 1 {
        if let ExprKind::Name { id, .. } = &func.node {
            if id == "list" {
                if let ExprKind::GeneratorExp { .. } = &args[0].node {
                    return Some(Check::new(
                        CheckKind::UnnecessaryGeneratorList,
                        Range::from_located(expr),
                    ));
                }
            }
        }
    }
    None
}

/// Check `set(generator)` compliance.
pub fn unnecessary_generator_set(expr: &Expr, func: &Expr, args: &[Expr]) -> Option<Check> {
    if args.len() == 1 {
        if let ExprKind::Name { id, .. } = &func.node {
            if id == "set" {
                if let ExprKind::GeneratorExp { .. } = &args[0].node {
                    return Some(Check::new(
                        CheckKind::UnnecessaryGeneratorSet,
                        Range::from_located(expr),
                    ));
                }
            }
        }
    }
    None
}

/// Check `dict((x, y) for x, y in iterable)` compliance.
pub fn unnecessary_generator_dict(expr: &Expr, func: &Expr, args: &[Expr]) -> Option<Check> {
    if args.len() == 1 {
        if let ExprKind::Name { id, .. } = &func.node {
            if id == "dict" {
                if let ExprKind::GeneratorExp { elt, .. } = &args[0].node {
                    match &elt.node {
                        ExprKind::Tuple { elts, .. } if elts.len() == 2 => {
                            return Some(Check::new(
                                CheckKind::UnnecessaryGeneratorDict,
                                Range::from_located(expr),
                            ));
                        }
                        _ => {}
                    }
                }
            }
        }
    }
    None
}

/// Check `set([...])` compliance.
pub fn unnecessary_list_comprehension_set(
    expr: &Expr,
    func: &Expr,
    args: &[Expr],
) -> Option<Check> {
    if args.len() == 1 {
        if let ExprKind::Name { id, .. } = &func.node {
            if id == "set" {
                if let ExprKind::ListComp { .. } = &args[0].node {
                    return Some(Check::new(
                        CheckKind::UnnecessaryListComprehensionSet,
                        Range::from_located(expr),
                    ));
                }
            }
        }
    }
    None
}

/// Check `dict([...])` compliance.
pub fn unnecessary_list_comprehension_dict(
    expr: &Expr,
    func: &Expr,
    args: &[Expr],
) -> Option<Check> {
    if args.len() == 1 {
        if let ExprKind::Name { id, .. } = &func.node {
            if id == "dict" {
                if let ExprKind::ListComp { elt, .. } = &args[0].node {
                    match &elt.node {
                        ExprKind::Tuple { elts, .. } if elts.len() == 2 => {
                            return Some(Check::new(
                                CheckKind::UnnecessaryListComprehensionDict,
                                Range::from_located(expr),
                            ));
                        }
                        _ => {}
                    }
                }
            }
        }
    }
    None
}

/// Check `set([1, 2])` compliance.
pub fn unnecessary_literal_set(expr: &Expr, func: &Expr, args: &[Expr]) -> Option<Check> {
    if args.len() == 1 {
        if let ExprKind::Name { id, .. } = &func.node {
            if id == "set" {
                match &args[0].node {
                    ExprKind::List { .. } => {
                        return Some(Check::new(
                            CheckKind::UnnecessaryLiteralSet("list".to_string()),
                            Range::from_located(expr),
                        ));
                    }
                    ExprKind::Tuple { .. } => {
                        return Some(Check::new(
                            CheckKind::UnnecessaryLiteralSet("tuple".to_string()),
                            Range::from_located(expr),
                        ));
                    }
                    _ => {}
                }
            }
        }
    }
    None
}

/// Check `dict([(1, 2)])` compliance.
pub fn unnecessary_literal_dict(expr: &Expr, func: &Expr, args: &[Expr]) -> Option<Check> {
    if args.len() == 1 {
        if let ExprKind::Name { id, .. } = &func.node {
            if id == "dict" {
                match &args[0].node {
                    ExprKind::Tuple { elts, .. } => {
                        if let Some(elt) = elts.first() {
                            match &elt.node {
                                // dict((1, 2), ...))
                                ExprKind::Tuple { elts, .. } if elts.len() == 2 => {
                                    return Some(Check::new(
                                        CheckKind::UnnecessaryLiteralDict("tuple".to_string()),
                                        Range::from_located(expr),
                                    ));
                                }
                                _ => {}
                            }
                        } else {
                            // dict(())
                            return Some(Check::new(
                                CheckKind::UnnecessaryLiteralDict("tuple".to_string()),
                                Range::from_located(expr),
                            ));
                        }
                    }
                    ExprKind::List { elts, .. } => {
                        if let Some(elt) = elts.first() {
                            match &elt.node {
                                // dict([(1, 2), ...])
                                ExprKind::Tuple { elts, .. } if elts.len() == 2 => {
                                    return Some(Check::new(
                                        CheckKind::UnnecessaryLiteralDict("list".to_string()),
                                        Range::from_located(expr),
                                    ));
                                }
                                _ => {}
                            }
                        } else {
                            // dict([])
                            return Some(Check::new(
                                CheckKind::UnnecessaryLiteralDict("list".to_string()),
                                Range::from_located(expr),
                            ));
                        }
                    }
                    _ => {}
                }
            }
        }
    }
    None
}

pub fn unnecessary_collection_call(
    expr: &Expr,
    func: &Expr,
    args: &[Expr],
    keywords: &[Located<KeywordData>],
) -> Option<Check> {
    if args.is_empty() {
        if let ExprKind::Name { id, .. } = &func.node {
            if id == "list" || id == "tuple" {
                // list() or tuple()
                return Some(Check::new(
                    CheckKind::UnnecessaryCollectionCall(id.to_string()),
                    Range::from_located(expr),
                ));
            } else if id == "dict" {
                // dict() or dict(a=1)
                if keywords.is_empty() || keywords.iter().all(|kw| kw.node.arg.is_some()) {
                    return Some(Check::new(
                        CheckKind::UnnecessaryCollectionCall(id.to_string()),
                        Range::from_located(expr),
                    ));
                }
            }
        }
    }
    None
}

pub fn unnecessary_literal_within_tuple_call(
    expr: &Expr,
    func: &Expr,
    args: &[Expr],
) -> Option<Check> {
    if let ExprKind::Name { id, .. } = &func.node {
        if id == "tuple" {
            if let Some(arg) = args.first() {
                match &arg.node {
                    ExprKind::Tuple { .. } => {
                        return Some(Check::new(
                            CheckKind::UnnecessaryLiteralWithinTupleCall("tuple".to_string()),
                            Range::from_located(expr),
                        ));
                    }
                    ExprKind::List { .. } => {
                        return Some(Check::new(
                            CheckKind::UnnecessaryLiteralWithinTupleCall("list".to_string()),
                            Range::from_located(expr),
                        ));
                    }
                    _ => {}
                }
            }
        }
    }

    None
}

pub fn unnecessary_literal_within_list_call(
    expr: &Expr,
    func: &Expr,
    args: &[Expr],
) -> Option<Check> {
    if let ExprKind::Name { id, .. } = &func.node {
        if id == "list" {
            if let Some(arg) = args.first() {
                match &arg.node {
                    ExprKind::Tuple { .. } => {
                        return Some(Check::new(
                            CheckKind::UnnecessaryLiteralWithinListCall("tuple".to_string()),
                            Range::from_located(expr),
                        ));
                    }
                    ExprKind::List { .. } => {
                        return Some(Check::new(
                            CheckKind::UnnecessaryLiteralWithinListCall("list".to_string()),
                            Range::from_located(expr),
                        ));
                    }
                    _ => {}
                }
            }
        }
    }

    None
}

pub fn unnecessary_list_call(expr: &Expr, func: &Expr, args: &[Expr]) -> Option<Check> {
    if let ExprKind::Name { id, .. } = &func.node {
        if id == "list" {
            if let Some(arg) = args.first() {
                if let ExprKind::ListComp { .. } = &arg.node {
                    return Some(Check::new(
                        CheckKind::UnnecessaryListCall,
                        Range::from_located(expr),
                    ));
                }
            }
        }
    }
    None
}

pub fn unnecessary_call_around_sorted(expr: &Expr, func: &Expr, args: &[Expr]) -> Option<Check> {
    if let ExprKind::Name { id: outer, .. } = &func.node {
        if outer == "list" || outer == "reversed" {
            if let Some(arg) = args.first() {
                if let ExprKind::Call { func, .. } = &arg.node {
                    if let ExprKind::Name { id: inner, .. } = &func.node {
                        if inner == "sorted" {
                            return Some(Check::new(
                                CheckKind::UnnecessaryCallAroundSorted(outer.to_string()),
                                Range::from_located(expr),
                            ));
                        }
                    }
                }
            }
        }
    }
    None
}

pub fn unnecessary_double_cast_or_process(
    expr: &Expr,
    func: &Expr,
    args: &[Expr],
) -> Option<Check> {
    if let ExprKind::Name { id: outer, .. } = &func.node {
        if outer == "list"
            || outer == "tuple"
            || outer == "set"
            || outer == "reversed"
            || outer == "sorted"
        {
            if let Some(arg) = args.first() {
                if let ExprKind::Call { func, .. } = &arg.node {
                    if let ExprKind::Name { id: inner, .. } = &func.node {
                        // Ex) set(tuple(...))
                        if (outer == "set" || outer == "sorted")
                            && (inner == "list"
                                || inner == "tuple"
                                || inner == "reversed"
                                || inner == "sorted")
                        {
                            return Some(Check::new(
                                CheckKind::UnnecessaryDoubleCastOrProcess(
                                    inner.to_string(),
                                    outer.to_string(),
                                ),
                                Range::from_located(expr),
                            ));
                        }

                        // Ex) list(tuple(...))
                        if (outer == "list" || outer == "tuple")
                            && (inner == "list" || inner == "tuple")
                        {
                            return Some(Check::new(
                                CheckKind::UnnecessaryDoubleCastOrProcess(
                                    inner.to_string(),
                                    outer.to_string(),
                                ),
                                Range::from_located(expr),
                            ));
                        }

                        // Ex) set(set(...))
                        if outer == "set" && inner == "set" {
                            return Some(Check::new(
                                CheckKind::UnnecessaryDoubleCastOrProcess(
                                    inner.to_string(),
                                    outer.to_string(),
                                ),
                                Range::from_located(expr),
                            ));
                        }
                    }
                }
            }
        }
    }
    None
}

pub fn unnecessary_subscript_reversal(expr: &Expr, func: &Expr, args: &[Expr]) -> Option<Check> {
    if let Some(first_arg) = args.first() {
        if let ExprKind::Name { id, .. } = &func.node {
            if id == "set" || id == "sorted" || id == "reversed" {
                if let ExprKind::Subscript { slice, .. } = &first_arg.node {
                    if let ExprKind::Slice { lower, upper, step } = &slice.node {
                        if lower.is_none() && upper.is_none() {
                            if let Some(step) = step {
                                if let ExprKind::UnaryOp {
                                    op: Unaryop::USub,
                                    operand,
                                } = &step.node
                                {
                                    if let ExprKind::Constant {
                                        value: Constant::Int(val),
                                        ..
                                    } = &operand.node
                                    {
                                        if *val == BigInt::from(1) {
                                            return Some(Check::new(
                                                CheckKind::UnnecessarySubscriptReversal(
                                                    id.to_string(),
                                                ),
                                                Range::from_located(expr),
                                            ));
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    None
}

pub fn unnecessary_comprehension(
    expr: &Expr,
    elt: &Expr,
    generators: &[Comprehension],
) -> Option<Check> {
    if generators.len() == 1 {
        let generator = &generators[0];
        if generator.ifs.is_empty() && generator.is_async == 0 {
            if let ExprKind::Name { id: elt_id, .. } = &elt.node {
                if let ExprKind::Name { id: target_id, .. } = &generator.target.node {
                    if elt_id == target_id {
                        match &expr.node {
                            ExprKind::ListComp { .. } => {
                                return Some(Check::new(
                                    CheckKind::UnnecessaryComprehension("list".to_string()),
                                    Range::from_located(expr),
                                ))
                            }
                            ExprKind::SetComp { .. } => {
                                return Some(Check::new(
                                    CheckKind::UnnecessaryComprehension("set".to_string()),
                                    Range::from_located(expr),
                                ))
                            }
                            _ => {}
                        };
                    }
                }
            }
        }
    }

    None
}

pub fn unnecessary_map(expr: &Expr, func: &Expr, args: &[Expr]) -> Option<Check> {
    if let ExprKind::Name { id, .. } = &func.node {
        if id == "map" {
            if args.len() == 2 {
                if let ExprKind::Lambda { .. } = &args[0].node {
                    return Some(Check::new(
                        CheckKind::UnnecessaryMap("generator".to_string()),
                        Range::from_located(expr),
                    ));
                }
            }
        } else if id == "list" || id == "set" {
            if let Some(arg) = args.first() {
                if let ExprKind::Call { func, args, .. } = &arg.node {
                    if let ExprKind::Name { id: f, .. } = &func.node {
                        if f == "map" {
                            if let Some(arg) = args.first() {
                                if let ExprKind::Lambda { .. } = &arg.node {
                                    return Some(Check::new(
                                        CheckKind::UnnecessaryMap(id.to_string()),
                                        Range::from_located(expr),
                                    ));
                                }
                            }
                        }
                    }
                }
            }
        } else if id == "dict" {
            if args.len() == 1 {
                if let ExprKind::Call { func, args, .. } = &args[0].node {
                    if let ExprKind::Name { id: f, .. } = &func.node {
                        if f == "map" {
                            if let Some(arg) = args.first() {
                                if let ExprKind::Lambda { body, .. } = &arg.node {
                                    match &body.node {
                                        ExprKind::Tuple { elts, .. }
                                        | ExprKind::List { elts, .. }
                                            if elts.len() == 2 =>
                                        {
                                            return Some(Check::new(
                                                CheckKind::UnnecessaryMap(id.to_string()),
                                                Range::from_located(expr),
                                            ))
                                        }
                                        _ => {}
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    None
}
