use fnv::{FnvHashMap, FnvHashSet};
use once_cell::sync::Lazy;
use regex::Regex;
use rustpython_ast::{Excepthandler, ExcepthandlerKind, Expr, ExprKind, Location, Stmt, StmtKind};

use crate::ast::types::Range;
use crate::SourceCodeLocator;

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

/// Rewrite any import aliases on a call path.
pub fn dealias_call_path<'a>(
    call_path: Vec<&'a str>,
    import_aliases: &FnvHashMap<&str, &'a str>,
) -> Vec<&'a str> {
    if let Some(head) = call_path.first() {
        if let Some(origin) = import_aliases.get(head) {
            let tail = &call_path[1..];
            let mut call_path: Vec<&str> = vec![];
            call_path.extend(origin.split('.'));
            call_path.extend(tail);
            call_path
        } else {
            call_path
        }
    } else {
        call_path
    }
}

/// Return `true` if the `Expr` is a name or attribute reference to `${target}`.
pub fn match_name_or_attr(expr: &Expr, target: &str) -> bool {
    match &expr.node {
        ExprKind::Attribute { attr, .. } => target == attr,
        ExprKind::Name { id, .. } => target == id,
        _ => false,
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
    import_aliases: &FnvHashMap<&str, &str>,
) -> bool {
    match_call_path(
        &dealias_call_path(collect_call_paths(expr), import_aliases),
        module,
        member,
        from_imports,
    )
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

    // Case (1): It's a builtin (like `list`).
    // Case (2a): We imported from the parent (`from typing.re import Match`,
    // `Match`).
    // Case (2b): We imported star from the parent (`from typing.re import *`,
    // `Match`).
    if num_segments == 0 {
        module.is_empty()
            || from_imports
                .get(module)
                .map(|imports| imports.contains(member) || imports.contains("*"))
                .unwrap_or(false)
    } else {
        let components: Vec<&str> = module.split('.').collect();

        // Case (3a): it's a fully qualified call path (`import typing`,
        // `typing.re.Match`). Case (3b): it's a fully qualified call path (`import
        // typing.re`, `typing.re.Match`).
        if components == call_path {
            return true;
        }

        // Case (4): We imported from the grandparent (`from typing import re`,
        // `re.Match`)
        let num_matches = (0..components.len())
            .take(num_segments)
            .take_while(|i| components[components.len() - 1 - i] == call_path[num_segments - 1 - i])
            .count();
        if num_matches > 0 {
            let cut = components.len() - num_matches;
            // TODO(charlie): Rewrite to avoid this allocation.
            let module = components[..cut].join(".");
            let member = components[cut];
            if from_imports
                .get(&module.as_str())
                .map(|imports| imports.contains(member))
                .unwrap_or(false)
            {
                return true;
            }
        }

        false
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

/// Format the module name for a relative import.
pub fn format_import_from(level: Option<&usize>, module: Option<&String>) -> String {
    let mut module_name = String::with_capacity(16);
    if let Some(level) = level {
        for _ in 0..*level {
            module_name.push('.');
        }
    }
    if let Some(module) = module {
        module_name.push_str(module);
    }
    module_name
}

/// Split a target string (like `typing.List`) into (`typing`, `List`).
pub fn to_module_and_member(target: &str) -> (&str, &str) {
    if let Some(index) = target.rfind('.') {
        (&target[..index], &target[index + 1..])
    } else {
        ("", target)
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

/// Return `true` if a `Stmt` has leading content.
pub fn match_leading_content(stmt: &Stmt, locator: &SourceCodeLocator) -> bool {
    let range = Range {
        location: Location::new(stmt.location.row(), 0),
        end_location: stmt.location,
    };
    let prefix = locator.slice_source_code_range(&range);
    prefix.chars().any(|char| !char.is_whitespace())
}

/// Return `true` if a `Stmt` has trailing content.
pub fn match_trailing_content(stmt: &Stmt, locator: &SourceCodeLocator) -> bool {
    let range = Range {
        location: stmt.end_location.unwrap(),
        end_location: Location::new(stmt.end_location.unwrap().row() + 1, 0),
    };
    let suffix = locator.slice_source_code_range(&range);
    for char in suffix.chars() {
        if char == '#' {
            return false;
        }
        if !char.is_whitespace() {
            return true;
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
            &FnvHashMap::default(),
            &FnvHashMap::default(),
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
            &FnvHashMap::default(),
            &FnvHashMap::default(),
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
            &FnvHashMap::default(),
        ));
        let expr = parser::parse_expression("re.Match", "<filename>")?;
        assert!(!match_module_member(
            &expr,
            "typing.re",
            "Match",
            &FnvHashMap::default(),
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
            &FnvHashMap::from_iter([("typing.re", FnvHashSet::from_iter(["*"]))]),
            &FnvHashMap::default()
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
            &FnvHashMap::from_iter([("typing.re", FnvHashSet::from_iter(["Match"]))]),
            &FnvHashMap::default()
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
            &FnvHashMap::from_iter([("typing", FnvHashSet::from_iter(["re"]))]),
            &FnvHashMap::default()
        ));

        let expr = parser::parse_expression("match.Match", "<filename>")?;
        assert!(match_module_member(
            &expr,
            "typing.re.match",
            "Match",
            &FnvHashMap::from_iter([("typing.re", FnvHashSet::from_iter(["match"]))]),
            &FnvHashMap::default()
        ));

        let expr = parser::parse_expression("re.match.Match", "<filename>")?;
        assert!(match_module_member(
            &expr,
            "typing.re.match",
            "Match",
            &FnvHashMap::from_iter([("typing", FnvHashSet::from_iter(["re"]))]),
            &FnvHashMap::default()
        ));
        Ok(())
    }

    #[test]
    fn from_alias() -> Result<()> {
        let expr = parser::parse_expression("IMatch", "<filename>")?;
        assert!(match_module_member(
            &expr,
            "typing.re",
            "Match",
            &FnvHashMap::from_iter([("typing.re", FnvHashSet::from_iter(["Match"]))]),
            &FnvHashMap::from_iter([("IMatch", "Match")]),
        ));
        Ok(())
    }

    #[test]
    fn from_aliased_parent() -> Result<()> {
        let expr = parser::parse_expression("t.Match", "<filename>")?;
        assert!(match_module_member(
            &expr,
            "typing.re",
            "Match",
            &FnvHashMap::default(),
            &FnvHashMap::from_iter([("t", "typing.re")]),
        ));
        Ok(())
    }

    #[test]
    fn from_aliased_grandparent() -> Result<()> {
        let expr = parser::parse_expression("t.re.Match", "<filename>")?;
        assert!(match_module_member(
            &expr,
            "typing.re",
            "Match",
            &FnvHashMap::default(),
            &FnvHashMap::from_iter([("t", "typing")]),
        ));
        Ok(())
    }
}
