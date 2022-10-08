use std::collections::BTreeSet;

use itertools::izip;
use regex::Regex;
use rustpython_parser::ast::{
    Arg, ArgData, Arguments, Cmpop, Constant, Excepthandler, ExcepthandlerKind, Expr, ExprKind,
    KeywordData, Located, Stmt, StmtKind, Unaryop,
};
use serde::{Deserialize, Serialize};

use crate::ast::types::{
    Binding, BindingKind, CheckLocator, FunctionScope, Range, Scope, ScopeKind,
};
use crate::checks::{Check, CheckKind, RejectedCmpop};
use crate::python::builtins::BUILTINS;

/// Check IfTuple compliance.
pub fn check_if_tuple(test: &Expr, location: Range) -> Option<Check> {
    if let ExprKind::Tuple { elts, .. } = &test.node {
        if !elts.is_empty() {
            return Some(Check::new(CheckKind::IfTuple, location));
        }
    }
    None
}

/// Check AssertTuple compliance.
pub fn check_assert_tuple(test: &Expr, location: Range) -> Option<Check> {
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
pub fn check_unused_variables(
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
pub fn check_do_not_assign_lambda(value: &Expr, location: Range) -> Option<Check> {
    if let ExprKind::Lambda { .. } = &value.node {
        Some(Check::new(CheckKind::DoNotAssignLambda, location))
    } else {
        None
    }
}

/// Check UselessMetaclassType compliance.
pub fn check_useless_metaclass_type(
    targets: &Vec<Expr>,
    value: &Expr,
    location: Range,
) -> Option<Check> {
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
pub fn check_unnecessary_abspath(func: &Expr, args: &Vec<Expr>, location: Range) -> Option<Check> {
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
pub fn check_type_of_primitive(func: &Expr, args: &Vec<Expr>, location: Range) -> Option<Check> {
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
pub fn check_ambiguous_variable_name(name: &str, location: Range) -> Option<Check> {
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
pub fn check_ambiguous_class_name(name: &str, location: Range) -> Option<Check> {
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
pub fn check_ambiguous_function_name(name: &str, location: Range) -> Option<Check> {
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
    name: &str,
    bases: &[Expr],
    scope: &Scope,
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
pub fn check_default_except_not_last(handlers: &Vec<Excepthandler>) -> Option<Check> {
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
pub fn check_raise_not_implemented(expr: &Expr) -> Option<Check> {
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
            checks.push(Check::new(
                CheckKind::DuplicateArgumentName,
                Range::from_located(arg),
            ));
        }
        idents.insert(ident);
    }

    checks
}

/// Check AssertEquals compliance.
pub fn check_assert_equals(expr: &Expr) -> Option<Check> {
    if let ExprKind::Attribute { value, attr, .. } = &expr.node {
        if attr == "assertEquals" {
            if let ExprKind::Name { id, .. } = &value.node {
                if id == "self" {
                    return Some(Check::new(
                        CheckKind::NoAssertEquals,
                        Range::from_located(expr),
                    ));
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
pub fn check_literal_comparisons(
    left: &Expr,
    ops: &Vec<Cmpop>,
    comparators: &Vec<Expr>,
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
pub fn check_is_literal(
    left: &Expr,
    ops: &Vec<Cmpop>,
    comparators: &Vec<Expr>,
    location: Range,
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
    location: Range,
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
pub fn check_break_outside_loop(
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
pub fn check_continue_outside_loop(
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
pub fn check_builtin_shadowing(
    name: &str,
    location: Range,
    node_type: ShadowingType,
) -> Option<Check> {
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

/// Returns `true` if a call is an argumented `super` invocation.
pub fn is_super_call_with_arguments(func: &Expr, args: &Vec<Expr>) -> bool {
    // Check: is this a `super` call?
    if let ExprKind::Name { id, .. } = &func.node {
        id == "super" && !args.is_empty()
    } else {
        false
    }
}

// flakes8-comprehensions
/// Check `list(generator)` compliance.
pub fn unnecessary_generator_list(expr: &Expr, func: &Expr, args: &Vec<Expr>) -> Option<Check> {
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
pub fn unnecessary_generator_set(expr: &Expr, func: &Expr, args: &Vec<Expr>) -> Option<Check> {
    if args.len() == 1 {
        if let ExprKind::Name { id, .. } = &func.node {
            if id == "set" {
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

/// Check `dict((x, y) for x, y in iterable)` compliance.
pub fn unnecessary_generator_dict(expr: &Expr, func: &Expr, args: &Vec<Expr>) -> Option<Check> {
    if args.len() == 1 {
        if let ExprKind::Name { id, .. } = &func.node {
            if id == "dict" {
                if let ExprKind::GeneratorExp { elt, .. } = &args[0].node {
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

/// Check `set([...])` compliance.
pub fn unnecessary_list_comprehension_set(
    expr: &Expr,
    func: &Expr,
    args: &Vec<Expr>,
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
    args: &Vec<Expr>,
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
pub fn unnecessary_literal_set(expr: &Expr, func: &Expr, args: &Vec<Expr>) -> Option<Check> {
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
pub fn unnecessary_literal_dict(expr: &Expr, func: &Expr, args: &Vec<Expr>) -> Option<Check> {
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
    args: &Vec<Expr>,
    keywords: &Vec<Located<KeywordData>>,
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

// flake8-super
/// Check that `super()` has no args
pub fn check_super_args(
    scope: &Scope,
    parents: &[&Stmt],
    expr: &Expr,
    func: &Expr,
    args: &Vec<Expr>,
) -> Option<Check> {
    if !is_super_call_with_arguments(func, args) {
        return None;
    }

    // Check: are we in a Function scope?
    if !matches!(scope.kind, ScopeKind::Function { .. }) {
        return None;
    }

    let mut parents = parents.iter().rev();

    // For a `super` invocation to be unnecessary, the first argument needs to match the enclosing
    // class, and the second argument needs to match the first argument to the enclosing function.
    if let [first_arg, second_arg] = args.as_slice() {
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
pub fn check_print_call(
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
