use rustpython_parser::ast::{ExcepthandlerKind, Stmt, StmtKind};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::identifier_range;
use ruff_python_ast::source_code::Locator;

/// ## What it does
/// Checks for functions with a high `McCabe` complexity.
///
/// The `McCabe` complexity of a function is a measure of the complexity of
/// the control flow graph of the function. It is calculated by adding
/// one to the number of decision points in the function. A decision
/// point is a place in the code where the program has a choice of two
/// or more paths to follow.
///
/// ## Why is this bad?
/// Functions with a high complexity are hard to understand and maintain.
///
/// ## Options
/// - `mccabe.max-complexity`
///
/// ## Example
/// ```python
/// def foo(a, b, c):
///     if a:
///         if b:
///             if c:
///                 return 1
///             else:
///                 return 2
///         else:
///             return 3
///     else:
///         return 4
/// ```
///
/// Use instead:
/// ```python
/// def foo(a, b, c):
///     if not a:
///         return 4
///     if not b:
///         return 3
///     if not c:
///         return 2
///     return 1
/// ```
#[violation]
pub struct ComplexStructure {
    pub name: String,
    pub complexity: usize,
    pub max_complexity: usize,
}

impl Violation for ComplexStructure {
    #[derive_message_formats]
    fn message(&self) -> String {
        let ComplexStructure {
            name,
            complexity,
            max_complexity,
        } = self;
        format!("`{name}` is too complex ({complexity} > {max_complexity})")
    }
}

fn get_complexity_number(stmts: &[Stmt]) -> usize {
    let mut complexity = 0;
    for stmt in stmts {
        match &stmt.node {
            StmtKind::If { body, orelse, .. } => {
                complexity += 1;
                complexity += get_complexity_number(body);
                complexity += get_complexity_number(orelse);
            }
            StmtKind::For { body, orelse, .. } | StmtKind::AsyncFor { body, orelse, .. } => {
                complexity += 1;
                complexity += get_complexity_number(body);
                complexity += get_complexity_number(orelse);
            }
            StmtKind::With { body, .. } | StmtKind::AsyncWith { body, .. } => {
                complexity += get_complexity_number(body);
            }
            StmtKind::While { body, orelse, .. } => {
                complexity += 1;
                complexity += get_complexity_number(body);
                complexity += get_complexity_number(orelse);
            }
            StmtKind::Match { cases, .. } => {
                complexity += 1;
                for case in cases {
                    complexity += get_complexity_number(&case.body);
                }
            }
            StmtKind::Try {
                body,
                handlers,
                orelse,
                finalbody,
            }
            | StmtKind::TryStar {
                body,
                handlers,
                orelse,
                finalbody,
            } => {
                complexity += get_complexity_number(body);
                if !orelse.is_empty() {
                    complexity += 1;
                }
                complexity += get_complexity_number(orelse);
                complexity += get_complexity_number(finalbody);
                for handler in handlers {
                    complexity += 1;
                    let ExcepthandlerKind::ExceptHandler { body, .. } = &handler.node;
                    complexity += get_complexity_number(body);
                }
            }
            StmtKind::FunctionDef { body, .. } | StmtKind::AsyncFunctionDef { body, .. } => {
                complexity += 1;
                complexity += get_complexity_number(body);
            }
            StmtKind::ClassDef { body, .. } => {
                complexity += get_complexity_number(body);
            }
            _ => {}
        }
    }
    complexity
}

pub fn function_is_too_complex(
    stmt: &Stmt,
    name: &str,
    body: &[Stmt],
    max_complexity: usize,
    locator: &Locator,
) -> Option<Diagnostic> {
    let complexity = get_complexity_number(body) + 1;
    if complexity > max_complexity {
        Some(Diagnostic::new(
            ComplexStructure {
                name: name.to_string(),
                complexity,
                max_complexity,
            },
            identifier_range(stmt, locator),
        ))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use anyhow::Result;
    use rustpython_parser as parser;

    use super::get_complexity_number;

    #[test]
    fn trivial() -> Result<()> {
        let source = r#"
def trivial():
    pass
"#;
        let stmts = parser::parse_program(source, "<filename>")?;
        assert_eq!(get_complexity_number(&stmts), 1);
        Ok(())
    }

    #[test]
    fn expr_as_statement() -> Result<()> {
        let source = r#"
def expr_as_statement():
    0xF00D
"#;
        let stmts = parser::parse_program(source, "<filename>")?;
        assert_eq!(get_complexity_number(&stmts), 1);
        Ok(())
    }

    #[test]
    fn sequential() -> Result<()> {
        let source = r#"
def sequential(n):
    k = n + 4
    s = k + n
    return s
"#;
        let stmts = parser::parse_program(source, "<filename>")?;
        assert_eq!(get_complexity_number(&stmts), 1);
        Ok(())
    }

    #[test]
    fn if_elif_else_dead_path() -> Result<()> {
        let source = r#"
def if_elif_else_dead_path(n):
    if n > 3:
        return "bigger than three"
    elif n > 4:
        return "is never executed"
    else:
        return "smaller than or equal to three"
"#;
        let stmts = parser::parse_program(source, "<filename>")?;
        assert_eq!(get_complexity_number(&stmts), 3);
        Ok(())
    }

    #[test]
    fn nested_ifs() -> Result<()> {
        let source = r#"
def nested_ifs():
    if n > 3:
        if n > 4:
            return "bigger than four"
        else:
            return "bigger than three"
    else:
        return "smaller than or equal to three"
"#;
        let stmts = parser::parse_program(source, "<filename>")?;
        assert_eq!(get_complexity_number(&stmts), 3);
        Ok(())
    }

    #[test]
    fn for_loop() -> Result<()> {
        let source = r#"
def for_loop():
    for i in range(10):
        print(i)
"#;
        let stmts = parser::parse_program(source, "<filename>")?;
        assert_eq!(get_complexity_number(&stmts), 2);
        Ok(())
    }

    #[test]
    fn for_else() -> Result<()> {
        let source = r#"
def for_else(mylist):
    for i in mylist:
        print(i)
    else:
        print(None)
"#;
        let stmts = parser::parse_program(source, "<filename>")?;
        assert_eq!(get_complexity_number(&stmts), 2);
        Ok(())
    }

    #[test]
    fn recursive() -> Result<()> {
        let source = r#"
def recursive(n):
    if n > 4:
        return f(n - 1)
    else:
        return n
"#;
        let stmts = parser::parse_program(source, "<filename>")?;
        assert_eq!(get_complexity_number(&stmts), 2);
        Ok(())
    }

    #[test]
    fn nested_functions() -> Result<()> {
        let source = r#"
def nested_functions():
    def a():
        def b():
            pass

        b()

    a()
"#;
        let stmts = parser::parse_program(source, "<filename>")?;
        assert_eq!(get_complexity_number(&stmts), 3);
        Ok(())
    }

    #[test]
    fn try_else() -> Result<()> {
        let source = r#"
def try_else():
    try:
        print(1)
    except TypeA:
        print(2)
    except TypeB:
        print(3)
    else:
        print(4)
"#;
        let stmts = parser::parse_program(source, "<filename>")?;
        assert_eq!(get_complexity_number(&stmts), 4);
        Ok(())
    }

    #[test]
    fn nested_try_finally() -> Result<()> {
        let source = r#"
def nested_try_finally():
    try:
        try:
            print(1)
        finally:
            print(2)
    finally:
        print(3)
"#;
        let stmts = parser::parse_program(source, "<filename>")?;
        assert_eq!(get_complexity_number(&stmts), 1);
        Ok(())
    }

    #[test]
    fn foobar() -> Result<()> {
        let source = r#"
async def foobar(a, b, c):
    await whatever(a, b, c)
    if await b:
        pass
    async with c:
        pass
    async for x in a:
        pass
"#;
        let stmts = parser::parse_program(source, "<filename>")?;
        assert_eq!(get_complexity_number(&stmts), 3);
        Ok(())
    }

    #[test]
    fn annotated_assign() -> Result<()> {
        let source = r#"
def annotated_assign():
    x: Any = None
"#;
        let stmts = parser::parse_program(source, "<filename>")?;
        assert_eq!(get_complexity_number(&stmts), 1);
        Ok(())
    }

    #[test]
    fn class() -> Result<()> {
        let source = r#"
class Class:
    def handle(self, *args, **options):
        if args:
            return

        class ServiceProvider:
            def a(self):
                pass

            def b(self, data):
                if not args:
                    pass

        class Logger:
            def c(*args, **kwargs):
                pass

            def error(self, message):
                pass

            def info(self, message):
                pass

            def exception(self):
                pass

        return ServiceProvider(Logger())
"#;
        let stmts = parser::parse_program(source, "<filename>")?;
        assert_eq!(get_complexity_number(&stmts), 9);
        Ok(())
    }

    #[test]
    fn finally() -> Result<()> {
        let source = r#"
def process_detect_lines():
    try:
        pass
    finally:
        pass
"#;
        let stmts = parser::parse_program(source, "<filename>")?;
        assert_eq!(get_complexity_number(&stmts), 1);
        Ok(())
    }

    #[test]
    fn if_in_finally() -> Result<()> {
        let source = r#"
def process_detect_lines():
    try:
        pass
    finally:
        if res:
            errors.append(f"Non-zero exit code {res}")
"#;
        let stmts = parser::parse_program(source, "<filename>")?;
        assert_eq!(get_complexity_number(&stmts), 2);
        Ok(())
    }

    #[test]
    fn with() -> Result<()> {
        let source = r#"
def with_lock():
    with lock:
        if foo:
            print('bar')
"#;
        let stmts = parser::parse_program(source, "<filename>")?;
        assert_eq!(get_complexity_number(&stmts), 2);
        Ok(())
    }
}
