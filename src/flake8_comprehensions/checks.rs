use num_bigint::BigInt;
use rustpython_ast::{
    Comprehension, Constant, Expr, ExprKind, Keyword, KeywordData, Located, Unaryop,
};

use crate::ast::types::Range;
use crate::checks::{Check, CheckKind};

fn function_name(func: &Expr) -> Option<&str> {
    if let ExprKind::Name { id, .. } = &func.node {
        Some(id)
    } else {
        None
    }
}

fn exactly_one_argument_with_matching_function<'a>(
    name: &str,
    func: &Expr,
    args: &'a [Expr],
    keywords: &[Keyword],
) -> Option<&'a ExprKind> {
    if !keywords.is_empty() {
        return None;
    }
    if args.len() != 1 {
        return None;
    }
    if function_name(func)? != name {
        return None;
    }
    Some(&args[0].node)
}

fn first_argument_with_matching_function<'a>(
    name: &str,
    func: &Expr,
    args: &'a [Expr],
) -> Option<&'a ExprKind> {
    if function_name(func)? != name {
        return None;
    }
    Some(&args.first()?.node)
}

/// C400 (`list(generator)`)
pub fn unnecessary_generator_list(
    expr: &Expr,
    func: &Expr,
    args: &[Expr],
    keywords: &[Keyword],
) -> Option<Check> {
    let argument = exactly_one_argument_with_matching_function("list", func, args, keywords)?;
    if let ExprKind::GeneratorExp { .. } = argument {
        return Some(Check::new(
            CheckKind::UnnecessaryGeneratorList,
            Range::from_located(expr),
        ));
    }
    None
}

/// C401 (`set(generator)`)
pub fn unnecessary_generator_set(
    expr: &Expr,
    func: &Expr,
    args: &[Expr],
    keywords: &[Keyword],
) -> Option<Check> {
    let argument = exactly_one_argument_with_matching_function("set", func, args, keywords)?;
    if let ExprKind::GeneratorExp { .. } = argument {
        return Some(Check::new(
            CheckKind::UnnecessaryGeneratorSet,
            Range::from_located(expr),
        ));
    }
    None
}

/// C402 (`dict((x, y) for x, y in iterable)`)
pub fn unnecessary_generator_dict(
    expr: &Expr,
    func: &Expr,
    args: &[Expr],
    keywords: &[Keyword],
) -> Option<Check> {
    let argument = exactly_one_argument_with_matching_function("dict", func, args, keywords)?;
    if let ExprKind::GeneratorExp { elt, .. } = argument {
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
    None
}

/// C403 (`set([...])`)
pub fn unnecessary_list_comprehension_set(
    expr: &Expr,
    func: &Expr,
    args: &[Expr],
    keywords: &[Keyword],
) -> Option<Check> {
    let argument = exactly_one_argument_with_matching_function("set", func, args, keywords)?;
    if let ExprKind::ListComp { .. } = &argument {
        return Some(Check::new(
            CheckKind::UnnecessaryListComprehensionSet,
            Range::from_located(expr),
        ));
    }
    None
}

/// C404 (`dict([...])`)
pub fn unnecessary_list_comprehension_dict(
    expr: &Expr,
    func: &Expr,
    args: &[Expr],
    keywords: &[Keyword],
) -> Option<Check> {
    let argument = exactly_one_argument_with_matching_function("dict", func, args, keywords)?;
    if let ExprKind::ListComp { elt, .. } = &argument {
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
    None
}

/// C405 (`set([1, 2])`)
pub fn unnecessary_literal_set(
    expr: &Expr,
    func: &Expr,
    args: &[Expr],
    keywords: &[Keyword],
) -> Option<Check> {
    let argument = exactly_one_argument_with_matching_function("set", func, args, keywords)?;
    let kind = match argument {
        ExprKind::List { .. } => "list",
        ExprKind::Tuple { .. } => "tuple",
        _ => return None,
    };
    Some(Check::new(
        CheckKind::UnnecessaryLiteralSet(kind.to_string()),
        Range::from_located(expr),
    ))
}

/// C406 (`dict([(1, 2)])`)
pub fn unnecessary_literal_dict(
    expr: &Expr,
    func: &Expr,
    args: &[Expr],
    keywords: &[Keyword],
) -> Option<Check> {
    let argument = exactly_one_argument_with_matching_function("dict", func, args, keywords)?;
    let (kind, elts) = match argument {
        ExprKind::Tuple { elts, .. } => ("tuple", elts),
        ExprKind::List { elts, .. } => ("list", elts),
        _ => return None,
    };

    if let Some(elt) = elts.first() {
        // dict((1, 2), ...)) or dict([(1, 2), ...])
        if !matches!(&elt.node, ExprKind::Tuple { elts, .. } if elts.len() == 2) {
            return None;
        }
    }

    Some(Check::new(
        CheckKind::UnnecessaryLiteralDict(kind.to_string()),
        Range::from_located(expr),
    ))
}

/// C408
pub fn unnecessary_collection_call(
    expr: &Expr,
    func: &Expr,
    args: &[Expr],
    keywords: &[Located<KeywordData>],
) -> Option<Check> {
    if !args.is_empty() {
        return None;
    }
    let id = function_name(func)?;
    match id {
        "dict" if keywords.is_empty() || keywords.iter().all(|kw| kw.node.arg.is_some()) => (),
        "list" | "tuple" => {
            // list() or tuple()
        }
        _ => return None,
    };
    Some(Check::new(
        CheckKind::UnnecessaryCollectionCall(id.to_string()),
        Range::from_located(expr),
    ))
}

/// C409
pub fn unnecessary_literal_within_tuple_call(
    expr: &Expr,
    func: &Expr,
    args: &[Expr],
) -> Option<Check> {
    let argument = first_argument_with_matching_function("tuple", func, args)?;
    let argument_kind = match argument {
        ExprKind::Tuple { .. } => "tuple",
        ExprKind::List { .. } => "list",
        _ => return None,
    };
    Some(Check::new(
        CheckKind::UnnecessaryLiteralWithinTupleCall(argument_kind.to_string()),
        Range::from_located(expr),
    ))
}

/// C410
pub fn unnecessary_literal_within_list_call(
    expr: &Expr,
    func: &Expr,
    args: &[Expr],
) -> Option<Check> {
    let argument = first_argument_with_matching_function("list", func, args)?;
    let argument_kind = match argument {
        ExprKind::Tuple { .. } => "tuple",
        ExprKind::List { .. } => "list",
        _ => return None,
    };
    Some(Check::new(
        CheckKind::UnnecessaryLiteralWithinListCall(argument_kind.to_string()),
        Range::from_located(expr),
    ))
}

/// C411
pub fn unnecessary_list_call(expr: &Expr, func: &Expr, args: &[Expr]) -> Option<Check> {
    let argument = first_argument_with_matching_function("list", func, args)?;
    if let ExprKind::ListComp { .. } = argument {
        return Some(Check::new(
            CheckKind::UnnecessaryListCall,
            Range::from_located(expr),
        ));
    }
    None
}

/// C413
pub fn unnecessary_call_around_sorted(expr: &Expr, func: &Expr, args: &[Expr]) -> Option<Check> {
    let outer = function_name(func)?;
    if !(outer == "list" || outer == "reversed") {
        return None;
    }
    if let ExprKind::Call { func, .. } = &args.first()?.node {
        if function_name(func)? == "sorted" {
            return Some(Check::new(
                CheckKind::UnnecessaryCallAroundSorted(outer.to_string()),
                Range::from_located(expr),
            ));
        }
    }
    None
}

/// C414
pub fn unnecessary_double_cast_or_process(
    expr: &Expr,
    func: &Expr,
    args: &[Expr],
) -> Option<Check> {
    let outer = function_name(func)?;
    if !["list", "tuple", "set", "reversed", "sorted"].contains(&outer) {
        return None;
    }

    fn new_check(inner: &str, outer: &str, expr: &Expr) -> Check {
        Check::new(
            CheckKind::UnnecessaryDoubleCastOrProcess(inner.to_string(), outer.to_string()),
            Range::from_located(expr),
        )
    }

    if let ExprKind::Call { func, .. } = &args.first()?.node {
        let inner = function_name(func)?;
        // Ex) set(tuple(...))
        if (outer == "set" || outer == "sorted")
            && (inner == "list" || inner == "tuple" || inner == "reversed" || inner == "sorted")
        {
            return Some(new_check(inner, outer, expr));
        }

        // Ex) list(tuple(...))
        if (outer == "list" || outer == "tuple") && (inner == "list" || inner == "tuple") {
            return Some(new_check(inner, outer, expr));
        }

        // Ex) set(set(...))
        if outer == "set" && inner == "set" {
            return Some(new_check(inner, outer, expr));
        }
    }
    None
}

/// C415
pub fn unnecessary_subscript_reversal(expr: &Expr, func: &Expr, args: &[Expr]) -> Option<Check> {
    let first_arg = args.first()?;
    let id = function_name(func)?;
    if !["set", "sorted", "reversed"].contains(&id) {
        return None;
    }
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
                                    CheckKind::UnnecessarySubscriptReversal(id.to_string()),
                                    Range::from_located(expr),
                                ));
                            }
                        }
                    }
                }
            }
        }
    }
    None
}

/// C416
pub fn unnecessary_comprehension(
    expr: &Expr,
    elt: &Expr,
    generators: &[Comprehension],
) -> Option<Check> {
    if generators.len() != 1 {
        return None;
    }
    let generator = &generators[0];
    if !(generator.ifs.is_empty() && generator.is_async == 0) {
        return None;
    }
    let elt_id = function_name(elt)?;
    let target_id = function_name(&generator.target)?;
    if elt_id != target_id {
        return None;
    }
    let expr_kind = match &expr.node {
        ExprKind::ListComp { .. } => "list",
        ExprKind::SetComp { .. } => "set",
        _ => return None,
    };
    Some(Check::new(
        CheckKind::UnnecessaryComprehension(expr_kind.to_string()),
        Range::from_located(expr),
    ))
}

/// C417
pub fn unnecessary_map(expr: &Expr, func: &Expr, args: &[Expr]) -> Option<Check> {
    fn new_check(kind: &str, expr: &Expr) -> Check {
        Check::new(
            CheckKind::UnnecessaryMap(kind.to_string()),
            Range::from_located(expr),
        )
    }
    let id = function_name(func)?;
    match id {
        "map" => {
            if args.len() == 2 && matches!(&args[0].node, ExprKind::Lambda { .. }) {
                return Some(new_check("generator", expr));
            }
        }
        "list" | "set" => {
            if let ExprKind::Call { func, args, .. } = &args.first()?.node {
                let argument = first_argument_with_matching_function("map", func, args)?;
                if let ExprKind::Lambda { .. } = argument {
                    return Some(new_check(id, expr));
                }
            }
        }
        "dict" => {
            if args.len() == 1 {
                if let ExprKind::Call { func, args, .. } = &args[0].node {
                    let argument = first_argument_with_matching_function("map", func, args)?;
                    if let ExprKind::Lambda { body, .. } = &argument {
                        if matches!(&body.node, ExprKind::Tuple { elts, .. } | ExprKind::List { elts, .. } if elts.len() == 2)
                        {
                            return Some(new_check(id, expr));
                        }
                    }
                }
            }
        }
        _ => (),
    }
    None
}
