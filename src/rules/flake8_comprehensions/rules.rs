use log::error;
use num_bigint::BigInt;
use rustpython_ast::{Comprehension, Constant, Expr, ExprKind, Keyword, Unaryop};

use super::fixes;
use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::registry::{Diagnostic, Rule};
use crate::violations;

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
    if function_name(func)? == name {
        Some(&args.first()?.node)
    } else {
        None
    }
}

/// C400 (`list(generator)`)
pub fn unnecessary_generator_list(
    checker: &mut Checker,
    expr: &Expr,
    func: &Expr,
    args: &[Expr],
    keywords: &[Keyword],
) {
    let Some(argument) = exactly_one_argument_with_matching_function("list", func, args, keywords) else {
        return;
    };
    if !checker.is_builtin("list") {
        return;
    }
    if let ExprKind::GeneratorExp { .. } = argument {
        let mut diagnostic = Diagnostic::new(
            violations::UnnecessaryGeneratorList,
            Range::from_located(expr),
        );
        if checker.patch(&Rule::UnnecessaryGeneratorList) {
            match fixes::fix_unnecessary_generator_list(checker.locator, checker.stylist, expr) {
                Ok(fix) => {
                    diagnostic.amend(fix);
                }
                Err(e) => error!("Failed to generate fix: {e}"),
            }
        }
        checker.diagnostics.push(diagnostic);
    }
}

/// C401 (`set(generator)`)
pub fn unnecessary_generator_set(
    checker: &mut Checker,
    expr: &Expr,
    func: &Expr,
    args: &[Expr],
    keywords: &[Keyword],
) {
    let Some(argument) = exactly_one_argument_with_matching_function("set", func, args, keywords) else {
        return;
    };
    if !checker.is_builtin("set") {
        return;
    }
    if let ExprKind::GeneratorExp { .. } = argument {
        let mut diagnostic = Diagnostic::new(
            violations::UnnecessaryGeneratorSet,
            Range::from_located(expr),
        );
        if checker.patch(&Rule::UnnecessaryGeneratorSet) {
            match fixes::fix_unnecessary_generator_set(checker.locator, checker.stylist, expr) {
                Ok(fix) => {
                    diagnostic.amend(fix);
                }
                Err(e) => error!("Failed to generate fix: {e}"),
            }
        }
        checker.diagnostics.push(diagnostic);
    }
}

/// C402 (`dict((x, y) for x, y in iterable)`)
pub fn unnecessary_generator_dict(
    checker: &mut Checker,
    expr: &Expr,
    func: &Expr,
    args: &[Expr],
    keywords: &[Keyword],
) {
    let Some(argument) = exactly_one_argument_with_matching_function("dict", func, args, keywords) else {
        return;
    };
    if let ExprKind::GeneratorExp { elt, .. } = argument {
        match &elt.node {
            ExprKind::Tuple { elts, .. } if elts.len() == 2 => {
                let mut diagnostic = Diagnostic::new(
                    violations::UnnecessaryGeneratorDict,
                    Range::from_located(expr),
                );
                if checker.patch(&Rule::UnnecessaryGeneratorDict) {
                    match fixes::fix_unnecessary_generator_dict(
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
            _ => {}
        }
    }
}

/// C403 (`set([...])`)
pub fn unnecessary_list_comprehension_set(
    checker: &mut Checker,
    expr: &Expr,
    func: &Expr,
    args: &[Expr],
    keywords: &[Keyword],
) {
    let Some(argument) = exactly_one_argument_with_matching_function("set", func, args, keywords) else {
        return;
    };
    if !checker.is_builtin("set") {
        return;
    }
    if let ExprKind::ListComp { .. } = &argument {
        let mut diagnostic = Diagnostic::new(
            violations::UnnecessaryListComprehensionSet,
            Range::from_located(expr),
        );
        if checker.patch(&Rule::UnnecessaryListComprehensionSet) {
            match fixes::fix_unnecessary_list_comprehension_set(
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

/// C404 (`dict([...])`)
pub fn unnecessary_list_comprehension_dict(
    checker: &mut Checker,
    expr: &Expr,
    func: &Expr,
    args: &[Expr],
    keywords: &[Keyword],
) {
    let Some(argument) = exactly_one_argument_with_matching_function("dict", func, args, keywords) else {
        return;
    };
    if !checker.is_builtin("dict") {
        return;
    }
    let ExprKind::ListComp { elt, .. } = &argument else {
        return;
    };
    let ExprKind::Tuple { elts, .. } = &elt.node else {
        return;
    };
    if elts.len() != 2 {
        return;
    }
    let mut diagnostic = Diagnostic::new(
        violations::UnnecessaryListComprehensionDict,
        Range::from_located(expr),
    );
    if checker.patch(&Rule::UnnecessaryListComprehensionDict) {
        match fixes::fix_unnecessary_list_comprehension_dict(checker.locator, checker.stylist, expr)
        {
            Ok(fix) => {
                diagnostic.amend(fix);
            }
            Err(e) => error!("Failed to generate fix: {e}"),
        }
    }
    checker.diagnostics.push(diagnostic);
}

/// C405 (`set([1, 2])`)
pub fn unnecessary_literal_set(
    checker: &mut Checker,
    expr: &Expr,
    func: &Expr,
    args: &[Expr],
    keywords: &[Keyword],
) {
    let Some(argument) = exactly_one_argument_with_matching_function("set", func, args, keywords) else {
        return;
    };
    if !checker.is_builtin("set") {
        return;
    }
    let kind = match argument {
        ExprKind::List { .. } => "list",
        ExprKind::Tuple { .. } => "tuple",
        _ => return,
    };
    let mut diagnostic = Diagnostic::new(
        violations::UnnecessaryLiteralSet(kind.to_string()),
        Range::from_located(expr),
    );
    if checker.patch(&Rule::UnnecessaryLiteralSet) {
        match fixes::fix_unnecessary_literal_set(checker.locator, checker.stylist, expr) {
            Ok(fix) => {
                diagnostic.amend(fix);
            }
            Err(e) => error!("Failed to generate fix: {e}"),
        }
    }
    checker.diagnostics.push(diagnostic);
}

/// C406 (`dict([(1, 2)])`)
pub fn unnecessary_literal_dict(
    checker: &mut Checker,
    expr: &Expr,
    func: &Expr,
    args: &[Expr],
    keywords: &[Keyword],
) {
    let Some(argument) = exactly_one_argument_with_matching_function("dict", func, args, keywords) else {
        return;
    };
    if !checker.is_builtin("dict") {
        return;
    }
    let (kind, elts) = match argument {
        ExprKind::Tuple { elts, .. } => ("tuple", elts),
        ExprKind::List { elts, .. } => ("list", elts),
        _ => return,
    };
    // Accept `dict((1, 2), ...))` `dict([(1, 2), ...])`.
    if !elts
        .iter()
        .all(|elt| matches!(&elt.node, ExprKind::Tuple { elts, .. } if elts.len() == 2))
    {
        return;
    }
    let mut diagnostic = Diagnostic::new(
        violations::UnnecessaryLiteralDict(kind.to_string()),
        Range::from_located(expr),
    );
    if checker.patch(&Rule::UnnecessaryLiteralDict) {
        match fixes::fix_unnecessary_literal_dict(checker.locator, checker.stylist, expr) {
            Ok(fix) => {
                diagnostic.amend(fix);
            }
            Err(e) => error!("Failed to generate fix: {e}"),
        }
    }
    checker.diagnostics.push(diagnostic);
}

/// C408
pub fn unnecessary_collection_call(
    checker: &mut Checker,
    expr: &Expr,
    func: &Expr,
    args: &[Expr],
    keywords: &[Keyword],
) {
    if !args.is_empty() {
        return;
    }
    let Some(id) = function_name(func) else {
        return;
    };
    match id {
        "dict" if keywords.is_empty() || keywords.iter().all(|kw| kw.node.arg.is_some()) => {
            // `dict()` or `dict(a=1)` (as opposed to `dict(**a)`)
        }
        "list" | "tuple" => {
            // `list()` or `tuple()`
        }
        _ => return,
    };
    if !checker.is_builtin(id) {
        return;
    }
    let mut diagnostic = Diagnostic::new(
        violations::UnnecessaryCollectionCall(id.to_string()),
        Range::from_located(expr),
    );
    if checker.patch(&Rule::UnnecessaryCollectionCall) {
        match fixes::fix_unnecessary_collection_call(checker.locator, checker.stylist, expr) {
            Ok(fix) => {
                diagnostic.amend(fix);
            }
            Err(e) => error!("Failed to generate fix: {e}"),
        }
    }
    checker.diagnostics.push(diagnostic);
}

/// C409
pub fn unnecessary_literal_within_tuple_call(
    checker: &mut Checker,
    expr: &Expr,
    func: &Expr,
    args: &[Expr],
) {
    let Some(argument) = first_argument_with_matching_function("tuple", func, args) else {
        return;
    };
    if !checker.is_builtin("tuple") {
        return;
    }
    let argument_kind = match argument {
        ExprKind::Tuple { .. } => "tuple",
        ExprKind::List { .. } => "list",
        _ => return,
    };
    let mut diagnostic = Diagnostic::new(
        violations::UnnecessaryLiteralWithinTupleCall(argument_kind.to_string()),
        Range::from_located(expr),
    );
    if checker.patch(&Rule::UnnecessaryLiteralWithinTupleCall) {
        match fixes::fix_unnecessary_literal_within_tuple_call(
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

/// C410
pub fn unnecessary_literal_within_list_call(
    checker: &mut Checker,
    expr: &Expr,
    func: &Expr,
    args: &[Expr],
) {
    let Some(argument) = first_argument_with_matching_function("list", func, args) else {
        return;
    };
    if !checker.is_builtin("list") {
        return;
    }
    let argument_kind = match argument {
        ExprKind::Tuple { .. } => "tuple",
        ExprKind::List { .. } => "list",
        _ => return,
    };
    let mut diagnostic = Diagnostic::new(
        violations::UnnecessaryLiteralWithinListCall(argument_kind.to_string()),
        Range::from_located(expr),
    );
    if checker.patch(&Rule::UnnecessaryLiteralWithinListCall) {
        match fixes::fix_unnecessary_literal_within_list_call(
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

/// C411
pub fn unnecessary_list_call(checker: &mut Checker, expr: &Expr, func: &Expr, args: &[Expr]) {
    let Some(argument) = first_argument_with_matching_function("list", func, args) else {
        return;
    };
    if !checker.is_builtin("list") {
        return;
    }
    if !matches!(argument, ExprKind::ListComp { .. }) {
        return;
    }
    let mut diagnostic =
        Diagnostic::new(violations::UnnecessaryListCall, Range::from_located(expr));
    if checker.patch(&Rule::UnnecessaryListCall) {
        match fixes::fix_unnecessary_list_call(checker.locator, checker.stylist, expr) {
            Ok(fix) => {
                diagnostic.amend(fix);
            }
            Err(e) => error!("Failed to generate fix: {e}"),
        }
    }
    checker.diagnostics.push(diagnostic);
}

/// C413
pub fn unnecessary_call_around_sorted(
    checker: &mut Checker,
    expr: &Expr,
    func: &Expr,
    args: &[Expr],
) {
    let Some(outer) = function_name(func) else {
        return;
    };
    if !(outer == "list" || outer == "reversed") {
        return;
    }
    let Some(arg) = args.first() else {
        return;
    };
    let ExprKind::Call { func, .. } = &arg.node else {
        return;
    };
    let Some(inner) = function_name(func) else {
        return;
    };
    if inner != "sorted" {
        return;
    }
    if !checker.is_builtin(inner) || !checker.is_builtin(outer) {
        return;
    }
    let mut diagnostic = Diagnostic::new(
        violations::UnnecessaryCallAroundSorted(outer.to_string()),
        Range::from_located(expr),
    );
    if checker.patch(&Rule::UnnecessaryCallAroundSorted) {
        match fixes::fix_unnecessary_call_around_sorted(checker.locator, checker.stylist, expr) {
            Ok(fix) => {
                diagnostic.amend(fix);
            }
            Err(e) => error!("Failed to generate fix: {e}"),
        }
    }
    checker.diagnostics.push(diagnostic);
}

/// C414
pub fn unnecessary_double_cast_or_process(
    checker: &mut Checker,
    expr: &Expr,
    func: &Expr,
    args: &[Expr],
) {
    fn diagnostic(inner: &str, outer: &str, location: Range) -> Diagnostic {
        Diagnostic::new(
            violations::UnnecessaryDoubleCastOrProcess(inner.to_string(), outer.to_string()),
            location,
        )
    }

    let Some(outer) = function_name(func) else {
        return;
    };
    if !(outer == "list"
        || outer == "tuple"
        || outer == "set"
        || outer == "reversed"
        || outer == "sorted")
    {
        return;
    }
    let Some(arg) = args.first() else {
        return;
    };
    let ExprKind::Call { func, .. } = &arg.node else {
        return;
    };
    let Some(inner) = function_name(func) else {
        return;
    };
    if !checker.is_builtin(inner) || !checker.is_builtin(outer) {
        return;
    }

    // Ex) set(tuple(...))
    if (outer == "set" || outer == "sorted")
        && (inner == "list" || inner == "tuple" || inner == "reversed" || inner == "sorted")
    {
        checker
            .diagnostics
            .push(diagnostic(inner, outer, Range::from_located(expr)));
        return;
    }

    // Ex) list(tuple(...))
    if (outer == "list" || outer == "tuple") && (inner == "list" || inner == "tuple") {
        checker
            .diagnostics
            .push(diagnostic(inner, outer, Range::from_located(expr)));
        return;
    }

    // Ex) set(set(...))
    if outer == "set" && inner == "set" {
        checker
            .diagnostics
            .push(diagnostic(inner, outer, Range::from_located(expr)));
    }
}

/// C415
pub fn unnecessary_subscript_reversal(
    checker: &mut Checker,
    expr: &Expr,
    func: &Expr,
    args: &[Expr],
) {
    let Some(first_arg) = args.first() else {
        return;
    };
    let Some(id) = function_name(func) else {
        return;
    };
    if !(id == "set" || id == "sorted" || id == "reversed") {
        return;
    }
    if !checker.is_builtin(id) {
        return;
    }
    let ExprKind::Subscript { slice, .. } = &first_arg.node else {
        return;
    };
    let ExprKind::Slice { lower, upper, step } = &slice.node else {
            return;
        };
    if lower.is_some() || upper.is_some() {
        return;
    }
    let Some(step) = step.as_ref() else {
        return;
    };
    let ExprKind::UnaryOp {
        op: Unaryop::USub,
        operand,
    } = &step.node else {
        return;
    };
    let ExprKind::Constant {
        value: Constant::Int(val),
        ..
    } = &operand.node else {
        return;
    };
    if *val != BigInt::from(1) {
        return;
    };
    checker.diagnostics.push(Diagnostic::new(
        violations::UnnecessarySubscriptReversal(id.to_string()),
        Range::from_located(expr),
    ));
}

/// C416
pub fn unnecessary_comprehension(
    checker: &mut Checker,
    expr: &Expr,
    elt: &Expr,
    generators: &[Comprehension],
) {
    if generators.len() != 1 {
        return;
    }
    let generator = &generators[0];
    if !(generator.ifs.is_empty() && generator.is_async == 0) {
        return;
    }
    let Some(elt_id) = function_name(elt) else {
        return;
    };

    let Some(target_id) = function_name(&generator.target) else {
        return;
    };
    if elt_id != target_id {
        return;
    }
    let id = match &expr.node {
        ExprKind::ListComp { .. } => "list",
        ExprKind::SetComp { .. } => "set",
        _ => return,
    };
    if !checker.is_builtin(id) {
        return;
    }
    let mut diagnostic = Diagnostic::new(
        violations::UnnecessaryComprehension(id.to_string()),
        Range::from_located(expr),
    );
    if checker.patch(&Rule::UnnecessaryComprehension) {
        match fixes::fix_unnecessary_comprehension(checker.locator, checker.stylist, expr) {
            Ok(fix) => {
                diagnostic.amend(fix);
            }
            Err(e) => error!("Failed to generate fix: {e}"),
        }
    }
    checker.diagnostics.push(diagnostic);
}

/// C417
pub fn unnecessary_map(checker: &mut Checker, expr: &Expr, func: &Expr, args: &[Expr]) {
    fn diagnostic(kind: &str, location: Range) -> Diagnostic {
        Diagnostic::new(violations::UnnecessaryMap(kind.to_string()), location)
    }

    let Some(id) = function_name(func)  else {
        return;
    };
    match id {
        "map" => {
            if !checker.is_builtin(id) {
                return;
            }

            if args.len() == 2 && matches!(&args[0].node, ExprKind::Lambda { .. }) {
                checker
                    .diagnostics
                    .push(diagnostic("generator", Range::from_located(expr)));
            }
        }
        "list" | "set" => {
            if !checker.is_builtin(id) {
                return;
            }

            if let Some(arg) = args.first() {
                if let ExprKind::Call { func, args, .. } = &arg.node {
                    let Some(argument) = first_argument_with_matching_function("map", func, args) else {
                        return;
                    };
                    if let ExprKind::Lambda { .. } = argument {
                        checker
                            .diagnostics
                            .push(diagnostic(id, Range::from_located(expr)));
                    }
                }
            }
        }
        "dict" => {
            if !checker.is_builtin(id) {
                return;
            }

            if args.len() == 1 {
                if let ExprKind::Call { func, args, .. } = &args[0].node {
                    let Some(argument) = first_argument_with_matching_function("map", func, args) else {
                        return;
                    };
                    if let ExprKind::Lambda { body, .. } = &argument {
                        if matches!(&body.node, ExprKind::Tuple { elts, .. } | ExprKind::List { elts, .. } if elts.len() == 2)
                        {
                            checker
                                .diagnostics
                                .push(diagnostic(id, Range::from_located(expr)));
                        }
                    }
                }
            }
        }
        _ => (),
    }
}
