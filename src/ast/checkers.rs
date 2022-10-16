use std::collections::BTreeSet;

use itertools::izip;
use num_bigint::BigInt;
use regex::Regex;
use rustpython_parser::ast::{
    Arg, ArgData, Arguments, Cmpop, Comprehension, Constant, Excepthandler, ExcepthandlerKind,
    Expr, ExprKind, KeywordData, Located, Stmt, StmtKind, Unaryop,
};
use serde::{Deserialize, Serialize};

use crate::ast::helpers;
use crate::ast::types::{
    Binding, BindingKind, CheckLocator, FunctionScope, Range, Scope, ScopeKind,
};
use crate::checks::{Check, CheckKind, RejectedCmpop};
use crate::python::builtins::BUILTINS;

/// Check IfTuple compliance.
pub fn if_tuple(test: &Expr, location: Range) -> Option<Check> {
    if let ExprKind::Tuple { elts, .. } = &test.node {
        if !elts.is_empty() {
            return Some(Check::new(CheckKind::IfTuple, location));
        }
    }
    None
}

/// Check AssertTuple compliance.
pub fn assert_tuple(test: &Expr, location: Range) -> Option<Check> {
    if let ExprKind::Tuple { elts, .. } = &test.node {
        if !elts.is_empty() {
            return Some(Check::new(CheckKind::AssertTuple, location));
        }
    }
    None
}

/// Check NotInTest and NotIsTest compliance.
pub fn not_tests(
    op: &Unaryop,
    operand: &Expr,
    check_not_in: bool,
    check_not_is: bool,
    locator: &dyn CheckLocator,
) -> Vec<Check> {
    let mut checks: Vec<Check> = vec![];

    if matches!(op, Unaryop::Not) {
        if let ExprKind::Compare { ops, .. } = &operand.node {
            for op in ops {
                match op {
                    Cmpop::In => {
                        if check_not_in {
                            checks.push(Check::new(
                                CheckKind::NotInTest,
                                locator.locate_check(Range::from_located(operand)),
                            ));
                        }
                    }
                    Cmpop::Is => {
                        if check_not_is {
                            checks.push(Check::new(
                                CheckKind::NotIsTest,
                                locator.locate_check(Range::from_located(operand)),
                            ));
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
pub fn unused_variables(
    scope: &Scope,
    locator: &dyn CheckLocator,
    dummy_variable_rgx: &Regex,
) -> Vec<Check> {
    let mut checks: Vec<Check> = vec![];

    if matches!(
        scope.kind,
        ScopeKind::Function(FunctionScope { uses_locals: true })
    ) {
        return checks;
    }

    for (name, binding) in scope.values.iter() {
        if binding.used.is_none()
            && matches!(binding.kind, BindingKind::Assignment)
            && !dummy_variable_rgx.is_match(name)
            && name != "__tracebackhide__"
            && name != "__traceback_info__"
            && name != "__traceback_supplement__"
        {
            checks.push(Check::new(
                CheckKind::UnusedVariable(name.to_string()),
                locator.locate_check(binding.range),
            ));
        }
    }

    checks
}

/// Check DoNotAssignLambda compliance.
pub fn do_not_assign_lambda(value: &Expr, location: Range) -> Option<Check> {
    if let ExprKind::Lambda { .. } = &value.node {
        Some(Check::new(CheckKind::DoNotAssignLambda, location))
    } else {
        None
    }
}

/// Check UselessMetaclassType compliance.
pub fn useless_metaclass_type(targets: &[Expr], value: &Expr, location: Range) -> Option<Check> {
    if targets.len() == 1 {
        if let ExprKind::Name { id, .. } = targets.first().map(|expr| &expr.node).unwrap() {
            if id == "__metaclass__" {
                if let ExprKind::Name { id, .. } = &value.node {
                    if id == "type" {
                        return Some(Check::new(CheckKind::UselessMetaclassType, location));
                    }
                }
            }
        }
    }
    None
}

/// Check UnnecessaryAbspath compliance.
pub fn unnecessary_abspath(func: &Expr, args: &[Expr], location: Range) -> Option<Check> {
    // Validate the arguments.
    if args.len() == 1 {
        if let ExprKind::Name { id, .. } = &args[0].node {
            if id == "__file__" {
                match &func.node {
                    ExprKind::Attribute { attr: id, .. } | ExprKind::Name { id, .. } => {
                        if id == "abspath" {
                            return Some(Check::new(CheckKind::UnnecessaryAbspath, location));
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    None
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Primitive {
    Bool,
    Str,
    Bytes,
    Int,
    Float,
    Complex,
}

impl Primitive {
    fn from_constant(constant: &Constant) -> Option<Self> {
        match constant {
            Constant::Bool(_) => Some(Primitive::Bool),
            Constant::Str(_) => Some(Primitive::Str),
            Constant::Bytes(_) => Some(Primitive::Bytes),
            Constant::Int(_) => Some(Primitive::Int),
            Constant::Float(_) => Some(Primitive::Float),
            Constant::Complex { .. } => Some(Primitive::Complex),
            _ => None,
        }
    }

    pub fn builtin(&self) -> String {
        match self {
            Primitive::Bool => "bool".to_string(),
            Primitive::Str => "str".to_string(),
            Primitive::Bytes => "bytes".to_string(),
            Primitive::Int => "int".to_string(),
            Primitive::Float => "float".to_string(),
            Primitive::Complex => "complex".to_string(),
        }
    }
}

/// Check TypeOfPrimitive compliance.
pub fn type_of_primitive(func: &Expr, args: &[Expr], location: Range) -> Option<Check> {
    // Validate the arguments.
    if args.len() == 1 {
        match &func.node {
            ExprKind::Attribute { attr: id, .. } | ExprKind::Name { id, .. } => {
                if id == "type" {
                    if let ExprKind::Constant { value, .. } = &args[0].node {
                        if let Some(primitive) = Primitive::from_constant(value) {
                            return Some(Check::new(
                                CheckKind::TypeOfPrimitive(primitive),
                                location,
                            ));
                        }
                    }
                }
            }
            _ => {}
        }
    }

    None
}

fn is_ambiguous_name(name: &str) -> bool {
    name == "l" || name == "I" || name == "O"
}

/// Check AmbiguousVariableName compliance.
pub fn ambiguous_variable_name(name: &str, location: Range) -> Option<Check> {
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
pub fn ambiguous_class_name(name: &str, location: Range) -> Option<Check> {
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
pub fn ambiguous_function_name(name: &str, location: Range) -> Option<Check> {
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
pub fn useless_object_inheritance(name: &str, bases: &[Expr], scope: &Scope) -> Option<Check> {
    for expr in bases {
        if let ExprKind::Name { id, .. } = &expr.node {
            if id == "object" {
                match scope.values.get(id) {
                    None
                    | Some(Binding {
                        kind: BindingKind::Builtin,
                        ..
                    }) => {
                        return Some(Check::new(
                            CheckKind::UselessObjectInheritance(name.to_string()),
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

/// Check DefaultExceptNotLast compliance.
pub fn default_except_not_last(handlers: &[Excepthandler]) -> Option<Check> {
    for (idx, handler) in handlers.iter().enumerate() {
        let ExcepthandlerKind::ExceptHandler { type_, .. } = &handler.node;
        if type_.is_none() && idx < handlers.len() - 1 {
            return Some(Check::new(
                CheckKind::DefaultExceptNotLast,
                Range::from_located(handler),
            ));
        }
    }

    None
}

/// Check RaiseNotImplemented compliance.
pub fn raise_not_implemented(expr: &Expr) -> Option<Check> {
    match &expr.node {
        ExprKind::Call { func, .. } => {
            if let ExprKind::Name { id, .. } = &func.node {
                if id == "NotImplemented" {
                    return Some(Check::new(
                        CheckKind::RaiseNotImplemented,
                        Range::from_located(expr),
                    ));
                }
            }
        }
        ExprKind::Name { id, .. } => {
            if id == "NotImplemented" {
                return Some(Check::new(
                    CheckKind::RaiseNotImplemented,
                    Range::from_located(expr),
                ));
            }
        }
        _ => {}
    }

    None
}

/// Check DuplicateArgumentName compliance.
pub fn duplicate_arguments(arguments: &Arguments) -> Vec<Check> {
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
            checks.push(Check::new(
                CheckKind::DuplicateArgumentName,
                Range::from_located(arg),
            ));
        }
        idents.insert(ident);
    }

    checks
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
pub fn repeated_keys(
    keys: &[Expr],
    check_repeated_literals: bool,
    check_repeated_variables: bool,
    locator: &dyn CheckLocator,
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
                            locator.locate_check(Range::from_located(k2)),
                        ))
                    }
                }
                (Some(DictionaryKey::Variable(v1)), Some(DictionaryKey::Variable(v2))) => {
                    if check_repeated_variables && v1 == v2 {
                        checks.push(Check::new(
                            CheckKind::MultiValueRepeatedKeyVariable((*v2).to_string()),
                            locator.locate_check(Range::from_located(k2)),
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
pub fn literal_comparisons(
    left: &Expr,
    ops: &[Cmpop],
    comparators: &[Expr],
    check_none_comparisons: bool,
    check_true_false_comparisons: bool,
    locator: &dyn CheckLocator,
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
                locator.locate_check(Range::from_located(comparator)),
            ));
        }
        if matches!(op, Cmpop::NotEq) {
            checks.push(Check::new(
                CheckKind::NoneComparison(RejectedCmpop::NotEq),
                locator.locate_check(Range::from_located(comparator)),
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
                    locator.locate_check(Range::from_located(comparator)),
                ));
            }
            if matches!(op, Cmpop::NotEq) {
                checks.push(Check::new(
                    CheckKind::TrueFalseComparison(value, RejectedCmpop::NotEq),
                    locator.locate_check(Range::from_located(comparator)),
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
                    locator.locate_check(Range::from_located(comparator)),
                ));
            }
            if matches!(op, Cmpop::NotEq) {
                checks.push(Check::new(
                    CheckKind::NoneComparison(RejectedCmpop::NotEq),
                    locator.locate_check(Range::from_located(comparator)),
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
                        locator.locate_check(Range::from_located(comparator)),
                    ));
                }
                if matches!(op, Cmpop::NotEq) {
                    checks.push(Check::new(
                        CheckKind::TrueFalseComparison(value, RejectedCmpop::NotEq),
                        locator.locate_check(Range::from_located(comparator)),
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
pub fn is_literal(left: &Expr, ops: &[Cmpop], comparators: &[Expr], location: Range) -> Vec<Check> {
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
pub fn type_comparison(ops: &[Cmpop], comparators: &[Expr], location: Range) -> Vec<Check> {
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
pub fn starred_expressions(
    elts: &[Expr],
    check_too_many_expressions: bool,
    check_two_starred_expressions: bool,
    location: Range,
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
                return Some(Check::new(CheckKind::ExpressionsInStarAssignment, location));
            }
        }
    }

    None
}

/// Check BreakOutsideLoop compliance.
pub fn break_outside_loop(
    stmt: &Stmt,
    parents: &[&Stmt],
    parent_stack: &[usize],
    locator: &dyn CheckLocator,
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
        Some(Check::new(
            CheckKind::BreakOutsideLoop,
            locator.locate_check(Range::from_located(stmt)),
        ))
    } else {
        None
    }
}

/// Check ContinueOutsideLoop compliance.
pub fn continue_outside_loop(
    stmt: &Stmt,
    parents: &[&Stmt],
    parent_stack: &[usize],
    locator: &dyn CheckLocator,
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
        Some(Check::new(
            CheckKind::ContinueOutsideLoop,
            locator.locate_check(Range::from_located(stmt)),
        ))
    } else {
        None
    }
}

// flake8-builtins
pub enum ShadowingType {
    Variable,
    Argument,
    Attribute,
}

/// Check builtin name shadowing
pub fn builtin_shadowing(name: &str, location: Range, node_type: ShadowingType) -> Option<Check> {
    if BUILTINS.contains(&name) {
        Some(Check::new(
            match node_type {
                ShadowingType::Variable => CheckKind::BuiltinVariableShadowing(name.to_string()),
                ShadowingType::Argument => CheckKind::BuiltinArgumentShadowing(name.to_string()),
                ShadowingType::Attribute => CheckKind::BuiltinAttributeShadowing(name.to_string()),
            },
            location,
        ))
    } else {
        None
    }
}

// flake8-comprehensions
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

// flake8-super
/// Check that `super()` has no args
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

    // For a `super` invocation to be unnecessary, the first argument needs to match the enclosing
    // class, and the second argument needs to match the first argument to the enclosing function.
    if let [first_arg, second_arg] = args {
        // Find the enclosing function definition (if any).
        if let Some(StmtKind::FunctionDef {
            args: parent_args, ..
        }) = parents
            .find(|stmt| matches!(stmt.node, StmtKind::FunctionDef { .. }))
            .map(|stmt| &stmt.node)
        {
            // Extract the name of the first argument to the enclosing function.
            if let Some(ArgData {
                arg: parent_arg, ..
            }) = parent_args.args.first().map(|expr| &expr.node)
            {
                // Find the enclosing class definition (if any).
                if let Some(StmtKind::ClassDef {
                    name: parent_name, ..
                }) = parents
                    .find(|stmt| matches!(stmt.node, StmtKind::ClassDef { .. }))
                    .map(|stmt| &stmt.node)
                {
                    if let (
                        ExprKind::Name {
                            id: first_arg_id, ..
                        },
                        ExprKind::Name {
                            id: second_arg_id, ..
                        },
                    ) = (&first_arg.node, &second_arg.node)
                    {
                        if first_arg_id == parent_name && second_arg_id == parent_arg {
                            return Some(Check::new(
                                CheckKind::SuperCallWithParameters,
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

// flake8-print
/// Check whether a function call is a `print` or `pprint` invocation
pub fn print_call(
    expr: &Expr,
    func: &Expr,
    check_print: bool,
    check_pprint: bool,
) -> Option<Check> {
    if let ExprKind::Name { id, .. } = &func.node {
        if check_print && id == "print" {
            return Some(Check::new(CheckKind::PrintFound, Range::from_located(expr)));
        } else if check_pprint && id == "pprint" {
            return Some(Check::new(
                CheckKind::PPrintFound,
                Range::from_located(expr),
            ));
        }
    }

    if let ExprKind::Attribute { value, attr, .. } = &func.node {
        if let ExprKind::Name { id, .. } = &value.node {
            if check_pprint && id == "pprint" && attr == "pprint" {
                return Some(Check::new(
                    CheckKind::PPrintFound,
                    Range::from_located(expr),
                ));
            }
        }
    }

    None
}

// pep8-naming
pub fn invalid_class_name(class_def: &Stmt, name: &str) -> Option<Check> {
    let stripped = name.strip_prefix('_').unwrap_or(name);
    if !stripped
        .chars()
        .next()
        .map(|c| c.is_uppercase())
        .unwrap_or(false)
        || stripped.contains('_')
    {
        return Some(Check::new(
            CheckKind::InvalidClassName(name.to_string()),
            Range::from_located(class_def),
        ));
    }
    None
}

pub fn invalid_function_name(func_def: &Stmt, name: &str) -> Option<Check> {
    if name.chars().any(|c| c.is_uppercase()) {
        return Some(Check::new(
            CheckKind::InvalidFunctionName(name.to_string()),
            Range::from_located(func_def),
        ));
    }
    None
}

pub fn invalid_argument_name(location: Range, name: &str) -> Option<Check> {
    if name.chars().any(|c| c.is_uppercase()) {
        return Some(Check::new(
            CheckKind::InvalidArgumentName(name.to_string()),
            location,
        ));
    }
    None
}

pub fn invalid_first_argument_name_for_class_method(
    scope: &Scope,
    decorator_list: &[Expr],
    args: &Arguments,
) -> Option<Check> {
    if !matches!(scope.kind, ScopeKind::Class) {
        return None;
    }

    if decorator_list.iter().any(|decorator| {
        if let ExprKind::Name { id, .. } = &decorator.node {
            id == "classmethod"
        } else {
            false
        }
    }) {
        if let Some(arg) = args.args.first() {
            if arg.node.arg != "cls" {
                return Some(Check::new(
                    CheckKind::InvalidFirstArgumentNameForClassMethod,
                    Range::from_located(arg),
                ));
            }
        }
    }
    None
}

pub fn invalid_first_argument_name_for_method(
    scope: &Scope,
    decorator_list: &[Expr],
    args: &Arguments,
) -> Option<Check> {
    if !matches!(scope.kind, ScopeKind::Class) {
        return None;
    }

    if decorator_list.iter().any(|decorator| {
        if let ExprKind::Name { id, .. } = &decorator.node {
            id == "classmethod" || id == "staticmethod"
        } else {
            false
        }
    }) {
        return None;
    }

    if let Some(arg) = args.args.first() {
        if arg.node.arg != "self" {
            return Some(Check::new(
                CheckKind::InvalidFirstArgumentNameForMethod,
                Range::from_located(arg),
            ));
        }
    }
    None
}
