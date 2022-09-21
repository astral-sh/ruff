use std::collections::BTreeSet;

use itertools::izip;
use rustpython_parser::ast::{
    Arg, Arguments, Cmpop, Constant, Excepthandler, ExcepthandlerKind, Expr, ExprKind, Keyword,
    Location, Stmt, StmtKind, Unaryop,
};

use crate::ast::operations::SourceCodeLocator;
use crate::ast::types::{Binding, BindingKind, FunctionScope, Scope, ScopeKind};
use crate::autofix::{fixer, fixes};
use crate::checks::{Check, CheckKind, Fix, RejectedCmpop};

/// Check IfTuple compliance.
pub fn check_if_tuple(test: &Expr, location: Location) -> Option<Check> {
    if let ExprKind::Tuple { elts, .. } = &test.node {
        if !elts.is_empty() {
            return Some(Check::new(CheckKind::IfTuple, location));
        }
    }
    None
}

/// Check AssertTuple compliance.
pub fn check_assert_tuple(test: &Expr, location: Location) -> Option<Check> {
    if let ExprKind::Tuple { elts, .. } = &test.node {
        if !elts.is_empty() {
            return Some(Check::new(CheckKind::AssertTuple, location));
        }
    }
    None
}

/// Check NotInTest and NotIsTest compliance.
pub fn check_not_tests(
    op: &Unaryop,
    operand: &Expr,
    check_not_in: bool,
    check_not_is: bool,
) -> Vec<Check> {
    let mut checks: Vec<Check> = vec![];

    if matches!(op, Unaryop::Not) {
        if let ExprKind::Compare { ops, .. } = &operand.node {
            for op in ops {
                match op {
                    Cmpop::In => {
                        if check_not_in {
                            checks.push(Check::new(CheckKind::NotInTest, operand.location));
                        }
                    }
                    Cmpop::Is => {
                        if check_not_is {
                            checks.push(Check::new(CheckKind::NotIsTest, operand.location));
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    checks
}

/// Check UnusedVariable compliance.
pub fn check_unused_variables(scope: &Scope) -> Vec<Check> {
    let mut checks: Vec<Check> = vec![];

    if matches!(
        scope.kind,
        ScopeKind::Function(FunctionScope { uses_locals: true })
    ) {
        return checks;
    }

    for (name, binding) in scope.values.iter() {
        // TODO(charlie): Ignore if using `locals`.
        if binding.used.is_none()
            && name != "_"
            && name != "__tracebackhide__"
            && name != "__traceback_info__"
            && name != "__traceback_supplement__"
            && matches!(binding.kind, BindingKind::Assignment)
        {
            checks.push(Check::new(
                CheckKind::UnusedVariable(name.to_string()),
                binding.location,
            ));
        }
    }

    checks
}

/// Check DoNotAssignLambda compliance.
pub fn check_do_not_assign_lambda(value: &Expr, location: Location) -> Option<Check> {
    if let ExprKind::Lambda { .. } = &value.node {
        Some(Check::new(CheckKind::DoNotAssignLambda, location))
    } else {
        None
    }
}

fn is_ambiguous_name(name: &str) -> bool {
    name == "l" || name == "I" || name == "O"
}

/// Check AmbiguousVariableName compliance.
pub fn check_ambiguous_variable_name(name: &str, location: Location) -> Option<Check> {
    if is_ambiguous_name(name) {
        Some(Check::new(
            CheckKind::AmbiguousVariableName(name.to_string()),
            location,
        ))
    } else {
        None
    }
}

/// Check AmbiguousClassName compliance.
pub fn check_ambiguous_class_name(name: &str, location: Location) -> Option<Check> {
    if is_ambiguous_name(name) {
        Some(Check::new(
            CheckKind::AmbiguousClassName(name.to_string()),
            location,
        ))
    } else {
        None
    }
}

/// Check AmbiguousFunctionName compliance.
pub fn check_ambiguous_function_name(name: &str, location: Location) -> Option<Check> {
    if is_ambiguous_name(name) {
        Some(Check::new(
            CheckKind::AmbiguousFunctionName(name.to_string()),
            location,
        ))
    } else {
        None
    }
}

/// Check UselessObjectInheritance compliance.
pub fn check_useless_object_inheritance(
    stmt: &Stmt,
    name: &str,
    bases: &[Expr],
    keywords: &[Keyword],
    scope: &Scope,
    locator: &mut SourceCodeLocator,
    autofix: &fixer::Mode,
) -> Option<Check> {
    for expr in bases {
        if let ExprKind::Name { id, .. } = &expr.node {
            if id == "object" {
                match scope.values.get(id) {
                    None
                    | Some(Binding {
                        kind: BindingKind::Builtin,
                        ..
                    }) => {
                        let mut check = Check::new(
                            CheckKind::UselessObjectInheritance(name.to_string()),
                            expr.location,
                        );
                        if matches!(autofix, fixer::Mode::Generate | fixer::Mode::Apply) {
                            if let Some(fix) = fixes::remove_class_def_base(
                                locator,
                                &stmt.location,
                                expr.location,
                                bases,
                                keywords,
                            ) {
                                check.amend(fix);
                            }
                        }
                        return Some(check);
                    }
                    _ => {}
                }
            }
        }
    }

    None
}

/// Check DefaultExceptNotLast compliance.
pub fn check_default_except_not_last(handlers: &Vec<Excepthandler>) -> Option<Check> {
    for (idx, handler) in handlers.iter().enumerate() {
        let ExcepthandlerKind::ExceptHandler { type_, .. } = &handler.node;
        if type_.is_none() && idx < handlers.len() - 1 {
            return Some(Check::new(
                CheckKind::DefaultExceptNotLast,
                handler.location,
            ));
        }
    }

    None
}

/// Check RaiseNotImplemented compliance.
pub fn check_raise_not_implemented(expr: &Expr) -> Option<Check> {
    match &expr.node {
        ExprKind::Call { func, .. } => {
            if let ExprKind::Name { id, .. } = &func.node {
                if id == "NotImplemented" {
                    return Some(Check::new(CheckKind::RaiseNotImplemented, expr.location));
                }
            }
        }
        ExprKind::Name { id, .. } => {
            if id == "NotImplemented" {
                return Some(Check::new(CheckKind::RaiseNotImplemented, expr.location));
            }
        }
        _ => {}
    }

    None
}

/// Check DuplicateArgumentName compliance.
pub fn check_duplicate_arguments(arguments: &Arguments) -> Vec<Check> {
    let mut checks: Vec<Check> = vec![];

    // Collect all the arguments into a single vector.
    let mut all_arguments: Vec<&Arg> = arguments
        .args
        .iter()
        .chain(arguments.posonlyargs.iter())
        .chain(arguments.kwonlyargs.iter())
        .collect();
    if let Some(arg) = &arguments.vararg {
        all_arguments.push(arg);
    }
    if let Some(arg) = &arguments.kwarg {
        all_arguments.push(arg);
    }

    // Search for duplicates.
    let mut idents: BTreeSet<&str> = BTreeSet::new();
    for arg in all_arguments {
        let ident = &arg.node.arg;
        if idents.contains(ident.as_str()) {
            checks.push(Check::new(CheckKind::DuplicateArgumentName, arg.location));
        }
        idents.insert(ident);
    }

    checks
}

/// Check AssertEquals compliance.
pub fn check_assert_equals(expr: &Expr, autofix: &fixer::Mode) -> Option<Check> {
    if let ExprKind::Attribute { value, attr, .. } = &expr.node {
        if attr == "assertEquals" {
            if let ExprKind::Name { id, .. } = &value.node {
                if id == "self" {
                    let mut check = Check::new(CheckKind::NoAssertEquals, expr.location);
                    if matches!(autofix, fixer::Mode::Generate | fixer::Mode::Apply) {
                        check.amend(Fix {
                            content: "assertEqual".to_string(),
                            start: Location::new(expr.location.row(), expr.location.column() + 1),
                            end: Location::new(
                                expr.location.row(),
                                expr.location.column() + 1 + "assertEquals".len(),
                            ),
                            applied: false,
                        });
                    }
                    return Some(check);
                }
            }
        }
    }
    None
}

#[derive(Debug, PartialEq)]
enum DictionaryKey<'a> {
    Constant(&'a Constant),
    Variable(&'a String),
}

fn convert_to_value(expr: &Expr) -> Option<DictionaryKey> {
    match &expr.node {
        ExprKind::Constant { value, .. } => Some(DictionaryKey::Constant(value)),
        ExprKind::Name { id, .. } => Some(DictionaryKey::Variable(id)),
        _ => None,
    }
}

/// Check MultiValueRepeatedKeyLiteral and MultiValueRepeatedKeyVariable compliance.
pub fn check_repeated_keys(
    keys: &Vec<Expr>,
    check_repeated_literals: bool,
    check_repeated_variables: bool,
) -> Vec<Check> {
    let mut checks: Vec<Check> = vec![];

    let num_keys = keys.len();
    for i in 0..num_keys {
        let k1 = &keys[i];
        let v1 = convert_to_value(k1);
        for k2 in keys.iter().take(num_keys).skip(i + 1) {
            let v2 = convert_to_value(k2);
            match (&v1, &v2) {
                (Some(DictionaryKey::Constant(v1)), Some(DictionaryKey::Constant(v2))) => {
                    if check_repeated_literals && v1 == v2 {
                        checks.push(Check::new(
                            CheckKind::MultiValueRepeatedKeyLiteral,
                            k2.location,
                        ))
                    }
                }
                (Some(DictionaryKey::Variable(v1)), Some(DictionaryKey::Variable(v2))) => {
                    if check_repeated_variables && v1 == v2 {
                        checks.push(Check::new(
                            CheckKind::MultiValueRepeatedKeyVariable((*v2).to_string()),
                            k2.location,
                        ))
                    }
                }
                _ => {}
            }
        }
    }

    checks
}

/// Check TrueFalseComparison and NoneComparison compliance.
pub fn check_literal_comparisons(
    left: &Expr,
    ops: &Vec<Cmpop>,
    comparators: &Vec<Expr>,
    check_none_comparisons: bool,
    check_true_false_comparisons: bool,
) -> Vec<Check> {
    let mut checks: Vec<Check> = vec![];

    let op = ops.first().unwrap();
    let comparator = left;

    // Check `left`.
    if check_none_comparisons
        && matches!(
            comparator.node,
            ExprKind::Constant {
                value: Constant::None,
                kind: None
            }
        )
    {
        if matches!(op, Cmpop::Eq) {
            checks.push(Check::new(
                CheckKind::NoneComparison(RejectedCmpop::Eq),
                comparator.location,
            ));
        }
        if matches!(op, Cmpop::NotEq) {
            checks.push(Check::new(
                CheckKind::NoneComparison(RejectedCmpop::NotEq),
                comparator.location,
            ));
        }
    }

    if check_true_false_comparisons {
        if let ExprKind::Constant {
            value: Constant::Bool(value),
            kind: None,
        } = comparator.node
        {
            if matches!(op, Cmpop::Eq) {
                checks.push(Check::new(
                    CheckKind::TrueFalseComparison(value, RejectedCmpop::Eq),
                    comparator.location,
                ));
            }
            if matches!(op, Cmpop::NotEq) {
                checks.push(Check::new(
                    CheckKind::TrueFalseComparison(value, RejectedCmpop::NotEq),
                    comparator.location,
                ));
            }
        }
    }

    // Check each comparator in order.
    for (op, comparator) in izip!(ops, comparators) {
        if check_none_comparisons
            && matches!(
                comparator.node,
                ExprKind::Constant {
                    value: Constant::None,
                    kind: None
                }
            )
        {
            if matches!(op, Cmpop::Eq) {
                checks.push(Check::new(
                    CheckKind::NoneComparison(RejectedCmpop::Eq),
                    comparator.location,
                ));
            }
            if matches!(op, Cmpop::NotEq) {
                checks.push(Check::new(
                    CheckKind::NoneComparison(RejectedCmpop::NotEq),
                    comparator.location,
                ));
            }
        }

        if check_true_false_comparisons {
            if let ExprKind::Constant {
                value: Constant::Bool(value),
                kind: None,
            } = comparator.node
            {
                if matches!(op, Cmpop::Eq) {
                    checks.push(Check::new(
                        CheckKind::TrueFalseComparison(value, RejectedCmpop::Eq),
                        comparator.location,
                    ));
                }
                if matches!(op, Cmpop::NotEq) {
                    checks.push(Check::new(
                        CheckKind::TrueFalseComparison(value, RejectedCmpop::NotEq),
                        comparator.location,
                    ));
                }
            }
        }
    }

    checks
}

fn is_constant(expr: &Expr) -> bool {
    match &expr.node {
        ExprKind::Constant { .. } => true,
        ExprKind::Tuple { elts, .. } => elts.iter().all(is_constant),
        _ => false,
    }
}

fn is_singleton(expr: &Expr) -> bool {
    matches!(
        expr.node,
        ExprKind::Constant {
            value: Constant::None | Constant::Bool(_) | Constant::Ellipsis,
            ..
        }
    )
}

fn is_constant_non_singleton(expr: &Expr) -> bool {
    is_constant(expr) && !is_singleton(expr)
}

/// Check IsLiteral compliance.
pub fn check_is_literal(
    left: &Expr,
    ops: &Vec<Cmpop>,
    comparators: &Vec<Expr>,
    location: Location,
) -> Vec<Check> {
    let mut checks: Vec<Check> = vec![];

    let mut left = left;
    for (op, right) in izip!(ops, comparators) {
        if matches!(op, Cmpop::Is | Cmpop::IsNot)
            && (is_constant_non_singleton(left) || is_constant_non_singleton(right))
        {
            checks.push(Check::new(CheckKind::IsLiteral, location));
        }
        left = right;
    }

    checks
}

/// Check TypeComparison compliance.
pub fn check_type_comparison(
    ops: &Vec<Cmpop>,
    comparators: &Vec<Expr>,
    location: Location,
) -> Vec<Check> {
    let mut checks: Vec<Check> = vec![];

    for (op, right) in izip!(ops, comparators) {
        if matches!(op, Cmpop::Is | Cmpop::IsNot | Cmpop::Eq | Cmpop::NotEq) {
            match &right.node {
                ExprKind::Call { func, args, .. } => {
                    if let ExprKind::Name { id, .. } = &func.node {
                        // Ex) type(False)
                        if id == "type" {
                            if let Some(arg) = args.first() {
                                // Allow comparison for types which are not obvious.
                                if !matches!(arg.node, ExprKind::Name { .. }) {
                                    checks.push(Check::new(CheckKind::TypeComparison, location));
                                }
                            }
                        }
                    }
                }
                ExprKind::Attribute { value, .. } => {
                    if let ExprKind::Name { id, .. } = &value.node {
                        // Ex) types.IntType
                        if id == "types" {
                            checks.push(Check::new(CheckKind::TypeComparison, location));
                        }
                    }
                }
                _ => {}
            }
        }
    }

    checks
}

/// Check TwoStarredExpressions and TooManyExpressionsInStarredAssignment compliance.
pub fn check_starred_expressions(
    elts: &[Expr],
    location: Location,
    check_too_many_expressions: bool,
    check_two_starred_expressions: bool,
) -> Option<Check> {
    let mut has_starred: bool = false;
    let mut starred_index: Option<usize> = None;
    for (index, elt) in elts.iter().enumerate() {
        if matches!(elt.node, ExprKind::Starred { .. }) {
            if has_starred && check_two_starred_expressions {
                return Some(Check::new(CheckKind::TwoStarredExpressions, location));
            }
            has_starred = true;
            starred_index = Some(index);
        }
    }

    if check_too_many_expressions {
        if let Some(starred_index) = starred_index {
            if starred_index >= 1 << 8 || elts.len() - starred_index > 1 << 24 {
                return Some(Check::new(
                    CheckKind::TooManyExpressionsInStarredAssignment,
                    location,
                ));
            }
        }
    }

    None
}

/// Check BreakOutsideLoop compliance.
pub fn check_break_outside_loop(
    stmt: &Stmt,
    parents: &[&Stmt],
    parent_stack: &[usize],
) -> Option<Check> {
    let mut allowed: bool = false;
    let mut parent = stmt;
    for index in parent_stack.iter().rev() {
        let child = parent;
        parent = parents[*index];
        match &parent.node {
            StmtKind::For { orelse, .. }
            | StmtKind::AsyncFor { orelse, .. }
            | StmtKind::While { orelse, .. } => {
                if !orelse.contains(child) {
                    allowed = true;
                    break;
                }
            }

            StmtKind::FunctionDef { .. }
            | StmtKind::AsyncFunctionDef { .. }
            | StmtKind::ClassDef { .. } => {
                break;
            }
            _ => {}
        }
    }

    if !allowed {
        Some(Check::new(CheckKind::BreakOutsideLoop, stmt.location))
    } else {
        None
    }
}

/// Check ContinueOutsideLoop compliance.
pub fn check_continue_outside_loop(
    stmt: &Stmt,
    parents: &[&Stmt],
    parent_stack: &[usize],
) -> Option<Check> {
    let mut allowed: bool = false;
    let mut parent = stmt;
    for index in parent_stack.iter().rev() {
        let child = parent;
        parent = parents[*index];
        match &parent.node {
            StmtKind::For { orelse, .. }
            | StmtKind::AsyncFor { orelse, .. }
            | StmtKind::While { orelse, .. } => {
                if !orelse.contains(child) {
                    allowed = true;
                    break;
                }
            }

            StmtKind::FunctionDef { .. }
            | StmtKind::AsyncFunctionDef { .. }
            | StmtKind::ClassDef { .. } => {
                break;
            }
            _ => {}
        }
    }

    if !allowed {
        Some(Check::new(CheckKind::ContinueOutsideLoop, stmt.location))
    } else {
        None
    }
}
