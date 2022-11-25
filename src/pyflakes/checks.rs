use regex::Regex;
use rustc_hash::FxHashSet;
use rustpython_ast::{Keyword, KeywordData};
use rustpython_parser::ast::{
    Arg, Arguments, Constant, Excepthandler, ExcepthandlerKind, Expr, ExprKind, Stmt, StmtKind,
};

use crate::ast::types::{BindingKind, FunctionScope, Range, Scope, ScopeKind};
use crate::checks::{Check, CheckKind};
use crate::pyflakes::format::FormatSummary;
use crate::vendored::format::{FieldName, FormatPart, FormatString, FromTemplate};

fn has_star_star_kwargs(keywords: &[Keyword]) -> bool {
    keywords.iter().any(|k| {
        let KeywordData { arg, .. } = &k.node;
        arg.is_none()
    })
}

fn has_star_args(args: &[Expr]) -> bool {
    args.iter()
        .any(|a| matches!(&a.node, ExprKind::Starred { .. }))
}

// F521
pub fn string_dot_format_invalid(literal: &str, location: Range) -> Option<Check> {
    match FormatString::from_str(literal) {
        Err(e) => Some(Check::new(
            CheckKind::StringDotFormatInvalidFormat(e.to_string()),
            location,
        )),
        Ok(format_string) => {
            for part in format_string.format_parts {
                if let FormatPart::Field { field_name, .. } = &part {
                    if let Err(e) = FieldName::parse(field_name) {
                        return Some(Check::new(
                            CheckKind::StringDotFormatInvalidFormat(e.to_string()),
                            location,
                        ));
                    }
                }
            }
            None
        }
    }
}

// F522
pub fn string_dot_format_extra_named_arguments(
    literal: &str,
    keywords: &[Keyword],
    location: Range,
) -> Option<Check> {
    if has_star_star_kwargs(keywords) {
        return None;
    }

    let keywords = keywords.iter().filter_map(|k| {
        let KeywordData { arg, .. } = &k.node;
        arg.clone()
    });

    match FormatString::from_str(literal) {
        Err(_) => None, // Cannot proceed - should be picked up by F521
        Ok(format_string) => {
            match FormatSummary::try_from(format_string) {
                Err(_) => None, // Cannot proceed - should be picked up by F521
                Ok(summary) => {
                    let missing: Vec<String> =
                        keywords.filter(|k| !summary.keywords.contains(k)).collect();

                    if missing.is_empty() {
                        None
                    } else {
                        Some(Check::new(
                            CheckKind::StringDotFormatExtraNamedArguments(missing),
                            location,
                        ))
                    }
                }
            }
        }
    }
}

// F523
pub fn string_dot_format_extra_positional_arguments(
    literal: &str,
    args: &[Expr],
    location: Range,
) -> Option<Check> {
    if has_star_args(args) {
        return None;
    }

    match FormatString::from_str(literal) {
        Err(_) => None, // Cannot proceed - should be picked up by F521
        Ok(format_string) => {
            match FormatSummary::try_from(format_string) {
                Err(_) => None, // Cannot proceed - should be picked up by F521
                Ok(summary) => {
                    let missing: Vec<String> = (0..args.len())
                        .filter(|i| !(summary.autos.contains(i) || summary.indexes.contains(i)))
                        .map(|i| i.to_string())
                        .collect();

                    if missing.is_empty() {
                        None
                    } else {
                        Some(Check::new(
                            CheckKind::StringDotFormatExtraPositionalArguments(missing),
                            location,
                        ))
                    }
                }
            }
        }
    }
}

// F524
pub fn string_dot_format_missing_argument(
    literal: &str,
    args: &[Expr],
    keywords: &[Keyword],
    location: Range,
) -> Option<Check> {
    if has_star_args(args) || has_star_star_kwargs(keywords) {
        return None;
    }

    let keywords: FxHashSet<_> = keywords
        .iter()
        .filter_map(|k| {
            let KeywordData { arg, .. } = &k.node;
            arg.clone()
        })
        .collect();

    match FormatString::from_str(literal) {
        Err(_) => None, // Cannot proceed - should be picked up by F521
        Ok(format_string) => {
            match FormatSummary::try_from(format_string) {
                Err(_) => None, // Cannot proceed - should be picked up by F521
                Ok(summary) => {
                    let missing: Vec<String> = summary
                        .autos
                        .into_iter()
                        .chain(summary.indexes.into_iter())
                        .filter(|&i| i >= args.len())
                        .map(|i| i.to_string())
                        .chain(summary.keywords.difference(&keywords).cloned())
                        .collect();

                    if missing.is_empty() {
                        None
                    } else {
                        Some(Check::new(
                            CheckKind::StringDotFormatMissingArguments(missing),
                            location,
                        ))
                    }
                }
            }
        }
    }
}

// F525
pub fn string_dot_format_mixing_automatic(literal: &str, location: Range) -> Option<Check> {
    match FormatString::from_str(literal) {
        Err(_) => None, // Cannot proceed - should be picked up by F521
        Ok(format_string) => {
            match FormatSummary::try_from(format_string) {
                Err(_) => None, // Cannot proceed - should be picked up by F521
                Ok(summary) => {
                    if summary.autos.is_empty() || summary.indexes.is_empty() {
                        None
                    } else {
                        Some(Check::new(
                            CheckKind::StringDotFormatMixingAutomatic,
                            location,
                        ))
                    }
                }
            }
        }
    }
}

/// F631
pub fn assert_tuple(test: &Expr, location: Range) -> Option<Check> {
    if let ExprKind::Tuple { elts, .. } = &test.node {
        if !elts.is_empty() {
            return Some(Check::new(CheckKind::AssertTuple, location));
        }
    }
    None
}

/// F634
pub fn if_tuple(test: &Expr, location: Range) -> Option<Check> {
    if let ExprKind::Tuple { elts, .. } = &test.node {
        if !elts.is_empty() {
            return Some(Check::new(CheckKind::IfTuple, location));
        }
    }
    None
}

/// F821
pub fn undefined_local(scopes: &[&Scope], name: &str) -> Option<Check> {
    let current = &scopes.last().expect("No current scope found.");
    if matches!(current.kind, ScopeKind::Function(_)) && !current.values.contains_key(name) {
        for scope in scopes.iter().rev().skip(1) {
            if matches!(scope.kind, ScopeKind::Function(_) | ScopeKind::Module) {
                if let Some(binding) = scope.values.get(name) {
                    if let Some((scope_id, location)) = binding.used {
                        if scope_id == current.id {
                            return Some(Check::new(
                                CheckKind::UndefinedLocal(name.to_string()),
                                location,
                            ));
                        }
                    }
                }
            }
        }
    }
    None
}

/// F841
pub fn unused_variables(scope: &Scope, dummy_variable_rgx: &Regex) -> Vec<Check> {
    let mut checks: Vec<Check> = vec![];

    if matches!(
        scope.kind,
        ScopeKind::Function(FunctionScope {
            uses_locals: true,
            ..
        })
    ) {
        return checks;
    }

    for (&name, binding) in &scope.values {
        if binding.used.is_none()
            && matches!(binding.kind, BindingKind::Assignment)
            && !dummy_variable_rgx.is_match(name)
            && name != "__tracebackhide__"
            && name != "__traceback_info__"
            && name != "__traceback_supplement__"
        {
            checks.push(Check::new(
                CheckKind::UnusedVariable(name.to_string()),
                binding.range,
            ));
        }
    }

    checks
}

/// F707
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

/// F831
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
    let mut idents: FxHashSet<&str> = FxHashSet::default();
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

/// F601, F602
pub fn repeated_keys(
    keys: &[Expr],
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
                            Range::from_located(k2),
                        ));
                    }
                }
                (Some(DictionaryKey::Variable(v1)), Some(DictionaryKey::Variable(v2))) => {
                    if check_repeated_variables && v1 == v2 {
                        checks.push(Check::new(
                            CheckKind::MultiValueRepeatedKeyVariable((*v2).to_string()),
                            Range::from_located(k2),
                        ));
                    }
                }
                _ => {}
            }
        }
    }

    checks
}

/// F621, F622
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

/// F701
pub fn break_outside_loop(stmt: &Stmt, parents: &[&Stmt], parent_stack: &[usize]) -> Option<Check> {
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

    if allowed {
        None
    } else {
        Some(Check::new(
            CheckKind::BreakOutsideLoop,
            Range::from_located(stmt),
        ))
    }
}

/// F702
pub fn continue_outside_loop(
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

    if allowed {
        None
    } else {
        Some(Check::new(
            CheckKind::ContinueOutsideLoop,
            Range::from_located(stmt),
        ))
    }
}
