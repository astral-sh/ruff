use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::identifier::Identifier;
use ruff_python_ast::{self as ast, Stmt, StmtKind, Suite};

use crate::Violation;
use crate::codes::Category;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for functions with a high `McCabe` complexity.
///
/// ## Why is this bad?
/// The `McCabe` complexity of a function is a measure of the complexity of
/// the control flow graph of the function. It is calculated by adding
/// one to the number of decision points in the function. A decision
/// point is a place in the code where the program has a choice of two
/// or more paths to follow.
///
/// Functions with a high complexity are hard to understand and maintain.
///
/// ## Example
/// ```python
/// def normalize_status(status):
///     if status == "new":
///         return "queued"
///     if status == "queued":
///         return "running"
///     if status == "running":
///         return "done"
///     if status == "failed":
///         return "retry"
///     if status == "cancelled":
///         return "closed"
///     return "unknown"
/// ```
///
/// Use instead:
///
/// ```python
/// STATUS_TRANSITIONS = {
///     "new": "queued",
///     "queued": "running",
///     "running": "done",
///     "failed": "retry",
///     "cancelled": "closed",
/// }
///
///
/// def normalize_status(status):
///     return STATUS_TRANSITIONS.get(status, "unknown")
/// ```
///
/// Note that this example assumes a `lint.mccabe.max-complexity` of 5 or less to trigger a
/// diagnostic because the initial code only has a complexity of 6.
///
/// ## Options
/// - `lint.mccabe.max-complexity`
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "v0.0.127", category = Category::Restriction)]
pub(crate) struct ComplexStructure {
    name: String,
    complexity: usize,
    max_complexity: usize,
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

fn get_complexity_number(stmts: Suite<'_>) -> usize {
    let mut complexity = 0;
    for stmt in stmts {
        match stmt.classify() {
            StmtKind::If(stmt_if) => {
                let body = stmt_if.body();
                let elif_else_clauses = stmt_if.elif_else_clauses();

                complexity += 1;
                complexity += get_complexity_number(body);
                for clause in elif_else_clauses {
                    if clause.test().is_some() {
                        complexity += 1;
                    }
                    complexity += get_complexity_number(clause.body());
                }
            }
            StmtKind::For(stmt_for) => {
                let body = stmt_for.body();
                let orelse = stmt_for.orelse();

                complexity += 1;
                complexity += get_complexity_number(body);
                complexity += get_complexity_number(orelse);
            }
            StmtKind::With(stmt_with) => {
                let body = stmt_with.body();

                complexity += get_complexity_number(body);
            }
            StmtKind::While(stmt_while) => {
                let body = stmt_while.body();
                let orelse = stmt_while.orelse();

                complexity += 1;
                complexity += get_complexity_number(body);
                complexity += get_complexity_number(orelse);
            }
            StmtKind::Match(stmt_match) => {
                let cases = stmt_match.cases();

                for case in cases {
                    complexity += 1;
                    complexity += get_complexity_number(case.body());
                }
                if let Some(last_case) = cases.last() {
                    // The complexity of an irrefutable pattern is similar to an `else` block of an `if` statement.
                    //
                    // For example:
                    // ```python
                    // match subject:
                    //     case 1: ...
                    //     case _: ...
                    //
                    // match subject:
                    //     case 1: ...
                    //     case foo: ...
                    // ```
                    if last_case.guard().is_none() && last_case.pattern().is_irrefutable() {
                        complexity -= 1;
                    }
                }
            }
            StmtKind::Try(stmt_try) => {
                let body = stmt_try.body();
                let handlers = stmt_try.handlers();
                let orelse = stmt_try.orelse();
                let finalbody = stmt_try.finalbody();

                complexity += get_complexity_number(body);
                if !orelse.is_empty() {
                    complexity += 1;
                }
                complexity += get_complexity_number(orelse);
                complexity += get_complexity_number(finalbody);
                for handler in handlers {
                    complexity += 1;
                    let ast::ExceptHandlerKind::ExceptHandler(handler) = handler.classify();
                    let body = handler.body();
                    complexity += get_complexity_number(body);
                }
            }
            StmtKind::FunctionDef(stmt_function_def) => {
                let body = stmt_function_def.body();

                complexity += 1;
                complexity += get_complexity_number(body);
            }
            StmtKind::ClassDef(stmt_class_def) => {
                let body = stmt_class_def.body();

                complexity += get_complexity_number(body);
            }
            _ => {}
        }
    }
    complexity
}

/// C901
pub(crate) fn function_is_too_complex(
    checker: &Checker,
    stmt: Stmt<'_>,
    name: &str,
    body: Suite<'_>,
    max_complexity: usize,
) {
    let complexity = get_complexity_number(body) + 1;
    if complexity > max_complexity {
        checker.report_diagnostic(
            ComplexStructure {
                name: name.to_string(),
                complexity,
                max_complexity,
            },
            stmt.identifier(),
        );
    }
}

#[cfg(test)]
mod tests {
    use anyhow::Result;

    use ruff_python_parser::parse_module;

    use super::get_complexity_number;

    #[test]
    fn trivial() -> Result<()> {
        let source = r"
def trivial():
    pass
";
        let parsed = parse_module(source)?;
        assert_eq!(get_complexity_number(parsed.suite()), 1);
        Ok(())
    }

    #[test]
    fn expr_as_statement() -> Result<()> {
        let source = r"
def expr_as_statement():
    0xF00D
";
        let parsed = parse_module(source)?;
        assert_eq!(get_complexity_number(parsed.suite()), 1);
        Ok(())
    }

    #[test]
    fn sequential() -> Result<()> {
        let source = r"
def sequential(n):
    k = n + 4
    s = k + n
    return s
";
        let parsed = parse_module(source)?;
        assert_eq!(get_complexity_number(parsed.suite()), 1);
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
        let parsed = parse_module(source)?;
        assert_eq!(get_complexity_number(parsed.suite()), 3);
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
        let parsed = parse_module(source)?;
        assert_eq!(get_complexity_number(parsed.suite()), 3);
        Ok(())
    }

    #[test]
    fn for_loop() -> Result<()> {
        let source = r"
def for_loop():
    for i in range(10):
        print(i)
";
        let parsed = parse_module(source)?;
        assert_eq!(get_complexity_number(parsed.suite()), 2);
        Ok(())
    }

    #[test]
    fn for_else() -> Result<()> {
        let source = r"
def for_else(mylist):
    for i in mylist:
        print(i)
    else:
        print(None)
";
        let parsed = parse_module(source)?;
        assert_eq!(get_complexity_number(parsed.suite()), 2);
        Ok(())
    }

    #[test]
    fn recursive() -> Result<()> {
        let source = r"
def recursive(n):
    if n > 4:
        return f(n - 1)
    else:
        return n
";
        let parsed = parse_module(source)?;
        assert_eq!(get_complexity_number(parsed.suite()), 2);
        Ok(())
    }

    #[test]
    fn nested_functions() -> Result<()> {
        let source = r"
def nested_functions():
    def a():
        def b():
            pass

        b()

    a()
";
        let parsed = parse_module(source)?;
        assert_eq!(get_complexity_number(parsed.suite()), 3);
        Ok(())
    }

    #[test]
    fn try_else() -> Result<()> {
        let source = r"
def try_else():
    try:
        print(1)
    except TypeA:
        print(2)
    except TypeB:
        print(3)
    else:
        print(4)
";
        let parsed = parse_module(source)?;
        assert_eq!(get_complexity_number(parsed.suite()), 4);
        Ok(())
    }

    #[test]
    fn nested_try_finally() -> Result<()> {
        let source = r"
def nested_try_finally():
    try:
        try:
            print(1)
        finally:
            print(2)
    finally:
        print(3)
";
        let parsed = parse_module(source)?;
        assert_eq!(get_complexity_number(parsed.suite()), 1);
        Ok(())
    }

    #[test]
    fn foobar() -> Result<()> {
        let source = r"
async def foobar(a, b, c):
    await whatever(a, b, c)
    if await b:
        pass
    async with c:
        pass
    async for x in a:
        pass
";
        let parsed = parse_module(source)?;
        assert_eq!(get_complexity_number(parsed.suite()), 3);
        Ok(())
    }

    #[test]
    fn annotated_assign() -> Result<()> {
        let source = r"
def annotated_assign():
    x: Any = None
";
        let parsed = parse_module(source)?;
        assert_eq!(get_complexity_number(parsed.suite()), 1);
        Ok(())
    }

    #[test]
    fn class() -> Result<()> {
        let source = r"
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
";
        let parsed = parse_module(source)?;
        assert_eq!(get_complexity_number(parsed.suite()), 9);
        Ok(())
    }

    #[test]
    fn finally() -> Result<()> {
        let source = r"
def process_detect_lines():
    try:
        pass
    finally:
        pass
";
        let parsed = parse_module(source)?;
        assert_eq!(get_complexity_number(parsed.suite()), 1);
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
        let parsed = parse_module(source)?;
        assert_eq!(get_complexity_number(parsed.suite()), 2);
        Ok(())
    }

    #[test]
    fn with() -> Result<()> {
        let source = r"
def with_lock():
    with lock:
        if foo:
            print('bar')
";
        let parsed = parse_module(source)?;
        assert_eq!(get_complexity_number(parsed.suite()), 2);
        Ok(())
    }

    #[test]
    fn simple_match_case() -> Result<()> {
        let source = r"
def f():
    match subject:
        case 2:
            print('foo')
        case _:
            print('bar')
";
        let parsed = parse_module(source)?;
        assert_eq!(get_complexity_number(parsed.suite()), 2);
        Ok(())
    }

    #[test]
    fn multiple_match_case() -> Result<()> {
        let source = r"
def f():
    match subject:
        case 2:
            print('foo')
        case 2:
            print('bar')
        case _:
            print('baz')
";
        let parsed = parse_module(source)?;
        assert_eq!(get_complexity_number(parsed.suite()), 3);
        Ok(())
    }

    #[test]
    fn named_catch_all_match_case() -> Result<()> {
        let source = r"
def f():
    match subject:
        case 2:
            print('hello')
        case x:
            print(x)
";
        let parsed = parse_module(source)?;
        assert_eq!(get_complexity_number(parsed.suite()), 2);
        Ok(())
    }

    #[test]
    fn match_case_catch_all_with_sequence() -> Result<()> {
        let source = r"
def f():
    match subject:
        case 2:
            print('hello')
        case 5 | _:
            print(x)
";
        let parsed = parse_module(source)?;
        assert_eq!(get_complexity_number(parsed.suite()), 2);
        Ok(())
    }
}
