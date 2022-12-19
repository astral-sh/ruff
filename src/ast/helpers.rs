use log::error;
use once_cell::sync::Lazy;
use regex::Regex;
use rustc_hash::{FxHashMap, FxHashSet};
use rustpython_ast::{
    Arguments, Constant, Excepthandler, ExcepthandlerKind, Expr, ExprKind, Keyword, KeywordData,
    Location, Stmt, StmtKind,
};
use rustpython_parser::lexer;
use rustpython_parser::lexer::Tok;

use crate::ast::types::Range;
use crate::SourceCodeLocator;

/// Create an `Expr` with default location from an `ExprKind`.
pub fn create_expr(node: ExprKind) -> Expr {
    Expr::new(Location::default(), Location::default(), node)
}

/// Create a `Stmt` with a default location from a `StmtKind`.
pub fn create_stmt(node: StmtKind) -> Stmt {
    Stmt::new(Location::default(), Location::default(), node)
}

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
pub fn compose_call_path(expr: &Expr) -> Option<String> {
    let segments = collect_call_paths(expr);
    if segments.is_empty() {
        None
    } else {
        Some(segments.join("."))
    }
}

/// Convert an `Expr` to its call path segments (like ["typing", "List"]).
pub fn collect_call_paths(expr: &Expr) -> Vec<&str> {
    let mut segments = vec![];
    collect_call_path_inner(expr, &mut segments);
    segments
}

/// Rewrite any import aliases on a call path.
pub fn dealias_call_path<'a>(
    call_path: Vec<&'a str>,
    import_aliases: &FxHashMap<&str, &'a str>,
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

/// Return `true` if the `Expr` is a reference to `${module}.${target}`.
///
/// Useful for, e.g., ensuring that a `Union` reference represents
/// `typing.Union`.
pub fn match_module_member(
    expr: &Expr,
    module: &str,
    member: &str,
    from_imports: &FxHashMap<&str, FxHashSet<&str>>,
    import_aliases: &FxHashMap<&str, &str>,
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
    from_imports: &FxHashMap<&str, FxHashSet<&str>>,
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
            || from_imports.get(module).map_or(false, |imports| {
                imports.contains(member) || imports.contains("*")
            })
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
                .map_or(false, |imports| imports.contains(member))
            {
                return true;
            }
        }

        false
    }
}

static DUNDER_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r"__[^\s]+__").unwrap());

/// Return `true` if the `Stmt` is an assignment to a dunder (like `__all__`).
pub fn is_assignment_to_a_dunder(stmt: &Stmt) -> bool {
    // Check whether it's an assignment to a dunder, with or without a type
    // annotation. This is what pycodestyle (as of 2.9.1) does.
    match &stmt.node {
        StmtKind::Assign { targets, .. } => {
            if targets.len() != 1 {
                return false;
            }
            match &targets[0].node {
                ExprKind::Name { id, ctx: _ } => DUNDER_REGEX.is_match(id),
                _ => false,
            }
        }
        StmtKind::AnnAssign { target, .. } => match &target.node {
            ExprKind::Name { id, ctx: _ } => DUNDER_REGEX.is_match(id),
            _ => false,
        },
        _ => false,
    }
}

/// Return `true` if the `Expr` is a singleton (`None`, `True`, `False`, or
/// `...`).
pub fn is_singleton(expr: &Expr) -> bool {
    matches!(
        expr.node,
        ExprKind::Constant {
            value: Constant::None | Constant::Bool(_) | Constant::Ellipsis,
            ..
        }
    )
}

/// Return `true` if the `Expr` is a constant or tuple of constants.
pub fn is_constant(expr: &Expr) -> bool {
    match &expr.node {
        ExprKind::Constant { .. } => true,
        ExprKind::Tuple { elts, .. } => elts.iter().all(is_constant),
        _ => false,
    }
}

/// Return `true` if the `Expr` is a non-singleton constant.
pub fn is_constant_non_singleton(expr: &Expr) -> bool {
    is_constant(expr) && !is_singleton(expr)
}

/// Return the `Keyword` with the given name, if it's present in the list of
/// `Keyword` arguments.
pub fn find_keyword<'a>(keywords: &'a [Keyword], keyword_name: &str) -> Option<&'a Keyword> {
    keywords.iter().find(|keyword| {
        let KeywordData { arg, .. } = &keyword.node;
        arg.as_ref().map_or(false, |arg| arg == keyword_name)
    })
}

/// Return `true` if an `Expr` is `None`.
pub fn is_const_none(expr: &Expr) -> bool {
    matches!(
        &expr.node,
        ExprKind::Constant {
            value: Constant::None,
            kind: None
        },
    )
}

/// Return `true` if a keyword argument is present with a non-`None` value.
pub fn has_non_none_keyword(keywords: &[Keyword], keyword: &str) -> bool {
    find_keyword(keywords, keyword).map_or(false, |keyword| {
        let KeywordData { value, .. } = &keyword.node;
        !is_const_none(value)
    })
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

/// Return the set of all bound argument names.
pub fn collect_arg_names<'a>(arguments: &'a Arguments) -> FxHashSet<&'a str> {
    let mut arg_names: FxHashSet<&'a str> = FxHashSet::default();
    for arg in &arguments.posonlyargs {
        arg_names.insert(arg.node.arg.as_str());
    }
    for arg in &arguments.args {
        arg_names.insert(arg.node.arg.as_str());
    }
    if let Some(arg) = &arguments.vararg {
        arg_names.insert(arg.node.arg.as_str());
    }
    for arg in &arguments.kwonlyargs {
        arg_names.insert(arg.node.arg.as_str());
    }
    if let Some(arg) = &arguments.kwarg {
        arg_names.insert(arg.node.arg.as_str());
    }
    arg_names
}

/// Returns `true` if a call is an argumented `super` invocation.
pub fn is_super_call_with_arguments(func: &Expr, args: &[Expr]) -> bool {
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
pub fn to_absolute(relative: Location, base: Location) -> Location {
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

/// Return the number of trailing empty lines following a statement.
pub fn count_trailing_lines(stmt: &Stmt, locator: &SourceCodeLocator) -> usize {
    let suffix =
        locator.slice_source_code_at(&Location::new(stmt.end_location.unwrap().row() + 1, 0));
    suffix
        .lines()
        .take_while(|line| line.trim().is_empty())
        .count()
}

/// Return the appropriate visual `Range` for any message that spans a `Stmt`.
/// Specifically, this method returns the range of a function or class name,
/// rather than that of the entire function or class body.
pub fn identifier_range(stmt: &Stmt, locator: &SourceCodeLocator) -> Range {
    if matches!(
        stmt.node,
        StmtKind::ClassDef { .. }
            | StmtKind::FunctionDef { .. }
            | StmtKind::AsyncFunctionDef { .. }
    ) {
        let contents = locator.slice_source_code_range(&Range::from_located(stmt));
        for (start, tok, end) in lexer::make_tokenizer(&contents).flatten() {
            if matches!(tok, Tok::Name { .. }) {
                let start = to_absolute(start, stmt.location);
                let end = to_absolute(end, stmt.location);
                return Range {
                    location: start,
                    end_location: end,
                };
            }
        }
        error!("Failed to find identifier for {:?}", stmt);
    }
    Range::from_located(stmt)
}

/// Return `true` if a `Stmt` appears to be part of a multi-statement line, with
/// other statements preceding it.
pub fn preceded_by_continuation(stmt: &Stmt, locator: &SourceCodeLocator) -> bool {
    // Does the previous line end in a continuation? This will have a specific
    // false-positive, which is that if the previous line ends in a comment, it
    // will be treated as a continuation. So we should only use this information to
    // make conservative choices.
    // TODO(charlie): Come up with a more robust strategy.
    if stmt.location.row() > 1 {
        let range = Range {
            location: Location::new(stmt.location.row() - 1, 0),
            end_location: Location::new(stmt.location.row(), 0),
        };
        let line = locator.slice_source_code_range(&range);
        if line.trim().ends_with('\\') {
            return true;
        }
    }
    false
}

/// Return `true` if a `Stmt` appears to be part of a multi-statement line, with
/// other statements preceding it.
pub fn preceded_by_multi_statement_line(stmt: &Stmt, locator: &SourceCodeLocator) -> bool {
    match_leading_content(stmt, locator) || preceded_by_continuation(stmt, locator)
}

/// Return `true` if a `Stmt` appears to be part of a multi-statement line, with
/// other statements following it.
pub fn followed_by_multi_statement_line(stmt: &Stmt, locator: &SourceCodeLocator) -> bool {
    match_trailing_content(stmt, locator)
}

#[cfg(test)]
mod tests {
    use anyhow::Result;
    use rustc_hash::{FxHashMap, FxHashSet};
    use rustpython_ast::Location;
    use rustpython_parser::parser;

    use crate::ast::helpers::{identifier_range, match_module_member, match_trailing_content};
    use crate::ast::types::Range;
    use crate::source_code_locator::SourceCodeLocator;

    #[test]
    fn builtin() -> Result<()> {
        let expr = parser::parse_expression("list", "<filename>")?;
        assert!(match_module_member(
            &expr,
            "",
            "list",
            &FxHashMap::default(),
            &FxHashMap::default(),
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
            &FxHashMap::default(),
            &FxHashMap::default(),
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
            &FxHashMap::default(),
            &FxHashMap::default(),
        ));
        let expr = parser::parse_expression("re.Match", "<filename>")?;
        assert!(!match_module_member(
            &expr,
            "typing.re",
            "Match",
            &FxHashMap::default(),
            &FxHashMap::default(),
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
            &FxHashMap::from_iter([("typing.re", FxHashSet::from_iter(["*"]))]),
            &FxHashMap::default()
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
            &FxHashMap::from_iter([("typing.re", FxHashSet::from_iter(["Match"]))]),
            &FxHashMap::default()
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
            &FxHashMap::from_iter([("typing", FxHashSet::from_iter(["re"]))]),
            &FxHashMap::default()
        ));

        let expr = parser::parse_expression("match.Match", "<filename>")?;
        assert!(match_module_member(
            &expr,
            "typing.re.match",
            "Match",
            &FxHashMap::from_iter([("typing.re", FxHashSet::from_iter(["match"]))]),
            &FxHashMap::default()
        ));

        let expr = parser::parse_expression("re.match.Match", "<filename>")?;
        assert!(match_module_member(
            &expr,
            "typing.re.match",
            "Match",
            &FxHashMap::from_iter([("typing", FxHashSet::from_iter(["re"]))]),
            &FxHashMap::default()
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
            &FxHashMap::from_iter([("typing.re", FxHashSet::from_iter(["Match"]))]),
            &FxHashMap::from_iter([("IMatch", "Match")]),
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
            &FxHashMap::default(),
            &FxHashMap::from_iter([("t", "typing.re")]),
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
            &FxHashMap::default(),
            &FxHashMap::from_iter([("t", "typing")]),
        ));
        Ok(())
    }

    #[test]
    fn trailing_content() -> Result<()> {
        let contents = "x = 1";
        let program = parser::parse_program(contents, "<filename>")?;
        let stmt = program.first().unwrap();
        let locator = SourceCodeLocator::new(contents);
        assert!(!match_trailing_content(stmt, &locator));

        let contents = "x = 1; y = 2";
        let program = parser::parse_program(contents, "<filename>")?;
        let stmt = program.first().unwrap();
        let locator = SourceCodeLocator::new(contents);
        assert!(match_trailing_content(stmt, &locator));

        let contents = "x = 1  ";
        let program = parser::parse_program(contents, "<filename>")?;
        let stmt = program.first().unwrap();
        let locator = SourceCodeLocator::new(contents);
        assert!(!match_trailing_content(stmt, &locator));

        let contents = "x = 1  # Comment";
        let program = parser::parse_program(contents, "<filename>")?;
        let stmt = program.first().unwrap();
        let locator = SourceCodeLocator::new(contents);
        assert!(!match_trailing_content(stmt, &locator));

        let contents = r#"
x = 1
y = 2
"#
        .trim();
        let program = parser::parse_program(contents, "<filename>")?;
        let stmt = program.first().unwrap();
        let locator = SourceCodeLocator::new(contents);
        assert!(!match_trailing_content(stmt, &locator));

        Ok(())
    }

    #[test]
    fn extract_identifier_range() -> Result<()> {
        let contents = "def f(): pass".trim();
        let program = parser::parse_program(contents, "<filename>")?;
        let stmt = program.first().unwrap();
        let locator = SourceCodeLocator::new(contents);
        assert_eq!(
            identifier_range(stmt, &locator),
            Range {
                location: Location::new(1, 4),
                end_location: Location::new(1, 5),
            }
        );

        let contents = r#"
def \
  f():
  pass
"#
        .trim();
        let program = parser::parse_program(contents, "<filename>")?;
        let stmt = program.first().unwrap();
        let locator = SourceCodeLocator::new(contents);
        assert_eq!(
            identifier_range(stmt, &locator),
            Range {
                location: Location::new(2, 2),
                end_location: Location::new(2, 3),
            }
        );

        let contents = "class Class(): pass".trim();
        let program = parser::parse_program(contents, "<filename>")?;
        let stmt = program.first().unwrap();
        let locator = SourceCodeLocator::new(contents);
        assert_eq!(
            identifier_range(stmt, &locator),
            Range {
                location: Location::new(1, 6),
                end_location: Location::new(1, 11),
            }
        );

        let contents = "class Class: pass".trim();
        let program = parser::parse_program(contents, "<filename>")?;
        let stmt = program.first().unwrap();
        let locator = SourceCodeLocator::new(contents);
        assert_eq!(
            identifier_range(stmt, &locator),
            Range {
                location: Location::new(1, 6),
                end_location: Location::new(1, 11),
            }
        );

        let contents = r#"
@decorator()
class Class():
  pass
"#
        .trim();
        let program = parser::parse_program(contents, "<filename>")?;
        let stmt = program.first().unwrap();
        let locator = SourceCodeLocator::new(contents);
        assert_eq!(
            identifier_range(stmt, &locator),
            Range {
                location: Location::new(2, 6),
                end_location: Location::new(2, 11),
            }
        );

        let contents = r#"x = y + 1"#.trim();
        let program = parser::parse_program(contents, "<filename>")?;
        let stmt = program.first().unwrap();
        let locator = SourceCodeLocator::new(contents);
        assert_eq!(
            identifier_range(stmt, &locator),
            Range {
                location: Location::new(1, 0),
                end_location: Location::new(1, 9),
            }
        );

        Ok(())
    }
}
