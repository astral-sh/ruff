use fnv::{FnvHashMap, FnvHashSet};
use once_cell::sync::Lazy;
use regex::Regex;
use rustpython_ast::{Excepthandler, ExcepthandlerKind, Expr, ExprKind, Location, StmtKind};

#[inline(always)]
fn collect_call_path_inner<'a>(expr: &'a Expr, parts: &mut Vec<&'a str>) {
    match &expr.node {
        ExprKind::Call { func, .. } => {
            collect_call_path_inner(func, parts);
        }
        ExprKind::Attribute { value, attr, .. } => {
            collect_call_path_inner(value, parts);
            parts.push(attr);
        }
        ExprKind::Name { id, .. } => {
            parts.push(id);
        }
        _ => {}
    }
}

/// Convert an `Expr` to its call path (like `List`, or `typing.List`).
#[inline(always)]
pub fn compose_call_path(expr: &Expr) -> Option<String> {
    let segments = collect_call_paths(expr);
    if segments.is_empty() {
        None
    } else {
        Some(segments.join("."))
    }
}

/// Convert an `Expr` to its call path segments (like ["typing", "List"]).
#[inline(always)]
pub fn collect_call_paths(expr: &Expr) -> Vec<&str> {
    let mut segments = vec![];
    collect_call_path_inner(expr, &mut segments);
    segments
}

/// Return `true` if the `Expr` is a name or attribute reference to `${target}`.
pub fn match_name_or_attr(expr: &Expr, target: &str) -> bool {
    match &expr.node {
        ExprKind::Attribute { attr, .. } => target == attr,
        ExprKind::Name { id, .. } => target == id,
        _ => false,
    }
}

static DUNDER_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r"__[^\s]+__").unwrap());

pub fn is_assignment_to_a_dunder(node: &StmtKind) -> bool {
    // Check whether it's an assignment to a dunder, with or without a type
    // annotation. This is what pycodestyle (as of 2.9.1) does.
    match node {
        StmtKind::Assign {
            targets,
            value: _,
            type_comment: _,
        } => {
            if targets.len() != 1 {
                return false;
            }
            match &targets[0].node {
                ExprKind::Name { id, ctx: _ } => DUNDER_REGEX.is_match(id),
                _ => false,
            }
        }
        StmtKind::AnnAssign {
            target,
            annotation: _,
            value: _,
            simple: _,
        } => match &target.node {
            ExprKind::Name { id, ctx: _ } => DUNDER_REGEX.is_match(id),
            _ => false,
        },
        _ => false,
    }
}

/// Extract the names of all handled exceptions.
pub fn extract_handler_names(handlers: &[Excepthandler]) -> Vec<Vec<&str>> {
    let mut handler_names = vec![];
    for handler in handlers {
        match &handler.node {
            ExcepthandlerKind::ExceptHandler { type_, .. } => {
                if let Some(type_) = type_ {
                    if let ExprKind::Tuple { elts, .. } = &type_.node {
                        for type_ in elts {
                            let call_path = collect_call_paths(type_);
                            if !call_path.is_empty() {
                                handler_names.push(call_path);
                            }
                        }
                    } else {
                        let call_path = collect_call_paths(type_);
                        if !call_path.is_empty() {
                            handler_names.push(call_path);
                        }
                    }
                }
            }
        }
    }
    handler_names
}

/// Returns `true` if a call is an argumented `super` invocation.
pub fn is_super_call_with_arguments(func: &Expr, args: &[Expr]) -> bool {
    // Check: is this a `super` call?
    if let ExprKind::Name { id, .. } = &func.node {
        id == "super" && !args.is_empty()
    } else {
        false
    }
}

/// Convert a location within a file (relative to `base`) to an absolute
/// position.
pub fn to_absolute(relative: &Location, base: &Location) -> Location {
    if relative.row() == 1 {
        Location::new(
            relative.row() + base.row() - 1,
            relative.column() + base.column(),
        )
    } else {
        Location::new(relative.row() + base.row() - 1, relative.column())
    }
}

/// Return `true` if the `Expr` is a reference to `${module}.${target}`.
///
/// Useful for, e.g., ensuring that a `Union` reference represents
/// `typing.Union`.
pub fn match_module_member(
    expr: &Expr,
    module: &str,
    member: &str,
    from_imports: &FnvHashMap<&str, FnvHashSet<&str>>,
) -> bool {
    match_call_path(&collect_call_paths(expr), module, member, from_imports)
}

/// Return `true` if the `call_path` is a reference to `${module}.${target}`.
///
/// Optimized version of `match_module_member` for pre-computed call paths.
pub fn match_call_path(
    call_path: &[&str],
    module: &str,
    member: &str,
    from_imports: &FnvHashMap<&str, FnvHashSet<&str>>,
) -> bool {
    // If we have no segments, we can't ever match.
    let num_segments = call_path.len();
    if num_segments == 0 {
        return false;
    }

    // If the last segment doesn't match the member, we can't ever match.
    if call_path[num_segments - 1] != member {
        return false;
    }

    // We now only need the module path, so throw out the member name.
    let call_path = &call_path[..num_segments - 1];
    let num_segments = call_path.len();

    // Case (1a): We imported star from the parent (`from typing.re import *`,
    // `Match`).
    // Case (1b): We imported from the parent (`from typing.re import Match`,
    // `Match`).
    // Case (2): It's a builtin (like `list`).
    if num_segments == 0
        && (module.is_empty()
            || from_imports
                .get(module)
                .map(|imports| imports.contains(member) || imports.contains("*"))
                .unwrap_or(false))
    {
        return true;
    }

    // Case (3a): it's a fully qualified call path (`import typing`,
    // `typing.re.Match`). Case (3b): it's a fully qualified call path (`import
    // typing.re`, `typing.re.Match`).
    if num_segments > 0
        && module
            .split('.')
            .enumerate()
            .all(|(index, segment)| index < num_segments && call_path[index] == segment)
    {
        return true;
    }

    // Case (4): We imported from the grandparent (`from typing import re`,
    // `re.Match`)
    if num_segments > 0 {
        // Find the number of common segments.
        // TODO(charlie): Rewrite to avoid this allocation.
        let parts: Vec<&str> = module.split('.').collect();
        let num_matches = (0..parts.len())
            .take(num_segments)
            .take_while(|i| parts[parts.len() - 1 - i] == call_path[num_segments - 1 - i])
            .count();
        if num_matches > 0 {
            // Verify that we have an import of the final common segment, from the remaining
            // parent.
            let cut = parts.len() - num_matches;
            // TODO(charlie): Rewrite to avoid this allocation.
            let module = parts[..cut].join(".");
            let member = parts[cut];
            if from_imports
                .get(&module.as_str())
                .map(|imports| imports.contains(member))
                .unwrap_or(false)
            {
                return true;
            }
        }
    }

    false
}

#[cfg(test)]
mod tests {
    use anyhow::Result;
    use fnv::{FnvHashMap, FnvHashSet};
    use rustpython_parser::parser;

    use crate::ast::helpers::match_module_member;

    #[test]
    fn builtin() -> Result<()> {
        let expr = parser::parse_expression("list", "<filename>")?;
        assert!(match_module_member(
            &expr,
            "",
            "list",
            &FnvHashMap::default()
        ));
        Ok(())
    }

    #[test]
    fn fully_qualified() -> Result<()> {
        let expr = parser::parse_expression("typing.re.Match", "<filename>")?;
        assert!(match_module_member(
            &expr,
            "typing.re",
            "Match",
            &FnvHashMap::default()
        ));
        Ok(())
    }

    #[test]
    fn unimported() -> Result<()> {
        let expr = parser::parse_expression("Match", "<filename>")?;
        assert!(!match_module_member(
            &expr,
            "typing.re",
            "Match",
            &FnvHashMap::default(),
        ));
        let expr = parser::parse_expression("re.Match", "<filename>")?;
        assert!(!match_module_member(
            &expr,
            "typing.re",
            "Match",
            &FnvHashMap::default(),
        ));
        Ok(())
    }

    #[test]
    fn from_star() -> Result<()> {
        let expr = parser::parse_expression("Match", "<filename>")?;
        assert!(match_module_member(
            &expr,
            "typing.re",
            "Match",
            &FnvHashMap::from_iter([("typing.re", FnvHashSet::from_iter(["*"]))])
        ));
        Ok(())
    }

    #[test]
    fn from_parent() -> Result<()> {
        let expr = parser::parse_expression("Match", "<filename>")?;
        assert!(match_module_member(
            &expr,
            "typing.re",
            "Match",
            &FnvHashMap::from_iter([("typing.re", FnvHashSet::from_iter(["Match"]))])
        ));
        Ok(())
    }

    #[test]
    fn from_grandparent() -> Result<()> {
        let expr = parser::parse_expression("re.Match", "<filename>")?;
        assert!(match_module_member(
            &expr,
            "typing.re",
            "Match",
            &FnvHashMap::from_iter([("typing", FnvHashSet::from_iter(["re"]))])
        ));

        let expr = parser::parse_expression("match.Match", "<filename>")?;
        assert!(match_module_member(
            &expr,
            "typing.re.match",
            "Match",
            &FnvHashMap::from_iter([("typing.re", FnvHashSet::from_iter(["match"]))])
        ));

        let expr = parser::parse_expression("re.match.Match", "<filename>")?;
        assert!(match_module_member(
            &expr,
            "typing.re.match",
            "Match",
            &FnvHashMap::from_iter([("typing", FnvHashSet::from_iter(["re"]))])
        ));
        Ok(())
    }
}
