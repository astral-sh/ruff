use ruff_python_ast::name::QualifiedName;
use ruff_python_ast::visitor::Visitor;
use ruff_python_ast::{
    Arguments, ExceptHandlerKind, Expr, ExprKind, Stmt, StmtKind, Suite, visitor,
};
use ruff_python_semantic::analyze::function_type;
use ruff_python_semantic::{ScopeKind, SemanticModel};
use ruff_text_size::Ranged;
use ruff_text_size::TextRange;

use crate::settings::LinterSettings;

/// Returns `true` if a module member is public despite having an
/// underscore-prefixed name.
pub(crate) fn is_underscore_prefixed_public_member(qualified_name: &QualifiedName) -> bool {
    matches!(qualified_name.segments(), ["os", "_exit"])
}

/// Returns the value of the `name` parameter to, e.g., a `TypeVar` constructor.
pub(super) fn type_param_name(arguments: Arguments<'_>) -> Option<&str> {
    // Handle both `TypeVar("T")` and `TypeVar(name="T")`.
    let name_param = arguments.find_argument_value("name", 0)?;
    if let Some(string_literal) = name_param.as_string_literal_expr() {
        let value = string_literal.value();
        Some(value.to_str())
    } else {
        None
    }
}

pub(super) fn in_dunder_method(
    dunder_name: &str,
    semantic: &SemanticModel,
    settings: &LinterSettings,
) -> bool {
    let scope = semantic.current_scope();
    let ScopeKind::Function(stmt_function_def) = scope.kind else {
        return false;
    };
    let name = stmt_function_def.name();
    let decorator_list = stmt_function_def.decorator_list();
    if *name != *dunder_name {
        return false;
    }
    let Some(parent) = semantic.first_non_type_parent_scope(scope) else {
        return false;
    };

    if !matches!(
        function_type::classify(
            &name,
            decorator_list,
            parent,
            semantic,
            &settings.pep8_naming.classmethod_decorators,
            &settings.pep8_naming.staticmethod_decorators,
        ),
        function_type::FunctionType::Method
    ) {
        return false;
    }
    true
}

/// Visitor to track reads from an iterable in a loop.
#[derive(Debug)]
pub(crate) struct SequenceIndexVisitor<'a> {
    /// `letters`, given `for index, letter in enumerate(letters)`.
    sequence_name: &'a str,
    /// `index`, given `for index, letter in enumerate(letters)`.
    index_name: &'a str,
    /// `letter`, given `for index, letter in enumerate(letters)`.
    value_name: &'a str,
    /// The ranges of any `letters[index]` accesses.
    accesses: Vec<TextRange>,
    /// Whether any of the variables have been modified.
    modified: bool,
}

impl<'a> SequenceIndexVisitor<'a> {
    pub(crate) fn new(sequence_name: &'a str, index_name: &'a str, value_name: &'a str) -> Self {
        Self {
            sequence_name,
            index_name,
            value_name,
            accesses: Vec::new(),
            modified: false,
        }
    }

    pub(crate) fn into_accesses(self) -> Vec<TextRange> {
        self.accesses
    }
}

impl SequenceIndexVisitor<'_> {
    fn is_assignment(&self, expr: Expr<'_>) -> bool {
        // If we see the sequence, a subscript, or the index being modified, we'll stop emitting
        // diagnostics.
        match expr.classify() {
            ExprKind::Name(expr_name) => {
                let id = expr_name.id();

                id == self.sequence_name || id == self.index_name || id == self.value_name
            }
            ExprKind::Subscript(expr_subscript) => {
                let value = expr_subscript.value();
                let slice = expr_subscript.slice();

                let Some(name_node) = value.as_name_expr() else {
                    return false;
                };
                let id = name_node.id();
                if id == self.sequence_name {
                    let Some(name_node) = slice.as_name_expr() else {
                        return false;
                    };
                    let id = name_node.id();
                    if id == self.index_name {
                        return true;
                    }
                }
                false
            }
            _ => false,
        }
    }
}

impl Visitor<'_> for SequenceIndexVisitor<'_> {
    fn visit_stmt(&mut self, stmt: Stmt<'_>) {
        if self.modified {
            return;
        }
        match stmt.classify() {
            StmtKind::Assign(stmt_assign) => {
                let targets = stmt_assign.targets();
                let value = stmt_assign.value();

                self.modified = targets.iter().any(|target| self.is_assignment(target));
                self.visit_expr(value);
            }
            StmtKind::AnnAssign(stmt_ann_assign) => {
                let target = stmt_ann_assign.target();
                let value = stmt_ann_assign.value();

                if let Some(value) = value {
                    self.modified = self.is_assignment(target);
                    self.visit_expr(value);
                }
            }
            StmtKind::AugAssign(stmt_aug_assign) => {
                let target = stmt_aug_assign.target();
                let value = stmt_aug_assign.value();

                self.modified = self.is_assignment(target);
                self.visit_expr(value);
            }
            StmtKind::Delete(stmt_delete) => {
                let targets = stmt_delete.targets();

                self.modified = targets.iter().any(|target| self.is_assignment(target));
            }
            _ => visitor::walk_stmt(self, stmt),
        }
    }

    fn visit_expr(&mut self, expr: Expr<'_>) {
        if self.modified {
            return;
        }
        if let Some(subscript_node) = expr.as_subscript_expr() {
            let value = subscript_node.value();
            let slice = subscript_node.slice();
            let range = subscript_node.range();
            if let Some(name_node_view) = value.as_name_expr() {
                let id = name_node_view.id();
                if id == self.sequence_name {
                    if let Some(name_node_view) = slice.as_name_expr() {
                        let id = name_node_view.id();
                        if id == self.index_name {
                            self.accesses.push(range);
                        }
                    }
                }
            }
        }

        visitor::walk_expr(self, expr);
    }
}

pub(crate) fn is_dunder_operator_method(method: &str) -> bool {
    matches!(
        method,
        "__lt__"
            | "__le__"
            | "__eq__"
            | "__ne__"
            | "__gt__"
            | "__ge__"
            | "__add__"
            | "__sub__"
            | "__mul__"
            | "__matmul__"
            | "__truediv__"
            | "__floordiv__"
            | "__mod__"
            | "__divmod__"
            | "__pow__"
            | "__lshift__"
            | "__rshift__"
            | "__and__"
            | "__xor__"
            | "__or__"
            | "__radd__"
            | "__rsub__"
            | "__rmul__"
            | "__rmatmul__"
            | "__rtruediv__"
            | "__rfloordiv__"
            | "__rmod__"
            | "__rdivmod__"
            | "__rpow__"
            | "__rlshift__"
            | "__rrshift__"
            | "__rand__"
            | "__rxor__"
            | "__ror__"
            | "__iadd__"
            | "__isub__"
            | "__imul__"
            | "__imatmul__"
            | "__itruediv__"
            | "__ifloordiv__"
            | "__imod__"
            | "__ipow__"
            | "__ilshift__"
            | "__irshift__"
            | "__iand__"
            | "__ixor__"
            | "__ior__"
    )
}

/// Returns `true` if a method is a known dunder method.
pub(super) fn is_known_dunder_method(method: &str) -> bool {
    if is_dunder_operator_method(method) {
        return true;
    }

    match method {
        "__abs__"
        | "__aenter__"
        | "__aexit__"
        | "__aiter__"
        | "__anext__"
        | "__attrs_init__"
        | "__attrs_post_init__"
        | "__attrs_pre_init__"
        | "__await__"
        | "__bool__"
        | "__buffer__"
        | "__bytes__"
        | "__call__"
        | "__ceil__"
        | "__class__"
        | "__class_getitem__"
        | "__complex__"
        | "__contains__"
        | "__copy__"
        | "__deepcopy__"
        | "__del__"
        | "__delattr__"
        | "__delete__"
        | "__delitem__"
        | "__dict__"
        | "__dir__"
        | "__doc__"
        | "__enter__"
        | "__exit__"
        | "__float__"
        | "__floor__"
        | "__format__"
        | "__fspath__"
        | "__get__"
        | "__getattr__"
        | "__getattribute__"
        | "__getitem__"
        | "__getnewargs__"
        | "__getnewargs_ex__"
        | "__getstate__"
        | "__hash__"
        | "__html__"
        | "__index__"
        | "__init__"
        | "__init_subclass__"
        | "__instancecheck__"
        | "__int__"
        | "__invert__"
        | "__iter__"
        | "__len__"
        | "__length_hint__"
        | "__missing__"
        | "__module__"
        | "__mro_entries__"
        | "__neg__"
        | "__new__"
        | "__next__"
        | "__pos__"
        | "__post_init__"
        | "__prepare__"
        | "__reduce__"
        | "__reduce_ex__"
        | "__release_buffer__"
        | "__replace__"
        | "__repr__"
        | "__reversed__"
        | "__round__"
        | "__set__"
        | "__set_name__"
        | "__setattr__"
        | "__setitem__"
        | "__setstate__"
        | "__sizeof__"
        | "__str__"
        | "__subclasscheck__"
        | "__subclasses__"
        | "__subclasshook__"
        | "__trunc__"
        | "__weakref__" => true,

        // Overridable sunder names from the `Enum` class.
        // See: https://docs.python.org/3/library/enum.html#supported-sunder-names
        "_add_alias_"
        | "_add_value_alias_"
        | "_name_"
        | "_value_"
        | "_missing_"
        | "_ignore_"
        | "_order_"
        | "_generate_next_value_" => true,

        _ => false,
    }
}

pub(super) fn num_statements(stmts: Suite<'_>) -> usize {
    let mut count = 0;
    for stmt in stmts {
        match stmt.classify() {
            StmtKind::If(stmt_if) => {
                count += 1;
                count += num_statements(stmt_if.body());
                for clause in stmt_if.elif_else_clauses() {
                    count += 1;
                    count += num_statements(clause.body());
                }
            }
            StmtKind::For(stmt_for) => {
                count += num_statements(stmt_for.body());
                count += num_statements(stmt_for.orelse());
            }
            StmtKind::While(stmt_while) => {
                count += 1;
                count += num_statements(stmt_while.body());
                count += num_statements(stmt_while.orelse());
            }
            StmtKind::Match(stmt_match) => {
                count += 1;
                for case in stmt_match.cases() {
                    count += 1;
                    count += num_statements(case.body());
                }
            }
            StmtKind::Try(stmt_try) => {
                count += 1;
                count += num_statements(stmt_try.body());
                let orelse = stmt_try.orelse();
                if !orelse.is_empty() {
                    count += 1 + num_statements(orelse);
                }
                let finalbody = stmt_try.finalbody();
                if !finalbody.is_empty() {
                    // Unclear why, but follow Pylint's convention.
                    count += 2 + num_statements(finalbody);
                }
                let handlers = stmt_try.handlers();
                if handlers.len() > 1 {
                    count += 1;
                }
                for handler in handlers {
                    count += 1;
                    let ExceptHandlerKind::ExceptHandler(handler) = handler.classify();
                    count += num_statements(handler.body());
                }
            }
            StmtKind::FunctionDef(function) => count += 1 + num_statements(function.body()),
            StmtKind::With(with) => count += 1 + num_statements(with.body()),
            StmtKind::Return(_) => {}
            _ => {
                count += 1;
            }
        }
    }
    count
}

#[cfg(test)]
mod tests {
    use anyhow::Result;

    use ruff_python_parser::parse_module;

    use super::num_statements;

    #[test]
    fn pass() -> Result<()> {
        let source: &str = r"
def f():
    pass
";
        let parsed = parse_module(source)?;
        assert_eq!(num_statements(parsed.suite()), 2);
        Ok(())
    }

    #[test]
    fn if_else() -> Result<()> {
        let source: &str = r"
def f():
    if a:
        print()
    else:
        print()
";
        let parsed = parse_module(source)?;
        assert_eq!(num_statements(parsed.suite()), 5);
        Ok(())
    }

    #[test]
    fn if_else_if_corner() -> Result<()> {
        let source: &str = r"
def f():
    if a:
        print()
    else:
        if a:
            print()
";
        let parsed = parse_module(source)?;
        assert_eq!(num_statements(parsed.suite()), 6);
        Ok(())
    }

    #[test]
    fn if_elif() -> Result<()> {
        let source: &str = r"
def f():
    if a:
        print()
    elif a:
        print()
";
        let parsed = parse_module(source)?;
        assert_eq!(num_statements(parsed.suite()), 5);
        Ok(())
    }

    #[test]
    fn if_elif_else() -> Result<()> {
        let source: &str = r"
def f():
    if a:
        print()
    elif a == 2:
        print()
    elif a == 3:
        print()
    else:
        print()
";
        let parsed = parse_module(source)?;
        assert_eq!(num_statements(parsed.suite()), 9);
        Ok(())
    }

    #[test]
    fn match_case() -> Result<()> {
        let source: &str = r"
def f():
    match x:
        case 3:
            pass
        case _:
            pass
";
        let parsed = parse_module(source)?;
        assert_eq!(num_statements(parsed.suite()), 6);
        Ok(())
    }

    #[test]
    fn many_statements() -> Result<()> {
        let source: &str = r"
async def f():
    a = 1
    b = 2
    c = 3
    await some_other_func()
    if a == 1:
        print('hello')
    else:
        other_func()
    count = 1
    while True:
        count += 1
        if count > 20:
            break;

    with open(f):
        with open(e):
            a -= 1
            import time
            pass
";
        let parsed = parse_module(source)?;
        assert_eq!(num_statements(parsed.suite()), 19);
        Ok(())
    }

    #[test]
    fn for_() -> Result<()> {
        let source: &str = r"
def f():
    for i in range(10):
        pass
";
        let parsed = parse_module(source)?;
        assert_eq!(num_statements(parsed.suite()), 2);
        Ok(())
    }

    #[test]
    fn for_else() -> Result<()> {
        let source: &str = r"
def f():
    for i in range(10):
        print()
    else:
        print()
";
        let parsed = parse_module(source)?;
        assert_eq!(num_statements(parsed.suite()), 3);
        Ok(())
    }

    #[test]
    fn nested_def() -> Result<()> {
        let source: &str = r"
def f():
    def g():
        print()
        print()

    print()
";
        let parsed = parse_module(source)?;
        assert_eq!(num_statements(parsed.suite()), 5);
        Ok(())
    }

    #[test]
    fn nested_class() -> Result<()> {
        let source: &str = r"
def f():
    class A:
        def __init__(self):
            pass

        def f(self):
            pass

    print()
";
        let parsed = parse_module(source)?;
        assert_eq!(num_statements(parsed.suite()), 3);
        Ok(())
    }

    #[test]
    fn return_not_counted() -> Result<()> {
        let source: &str = r"
def f():
    return
";
        let parsed = parse_module(source)?;
        assert_eq!(num_statements(parsed.suite()), 1);
        Ok(())
    }

    #[test]
    fn with() -> Result<()> {
        let source: &str = r"
def f():
    with a:
        if a:
            print()
        else:
            print()

";
        let parsed = parse_module(source)?;
        assert_eq!(num_statements(parsed.suite()), 6);
        Ok(())
    }

    #[test]
    fn try_except() -> Result<()> {
        let source: &str = r"
def f():
    try:
        print()
    except Exception:
        raise
";
        let parsed = parse_module(source)?;
        assert_eq!(num_statements(parsed.suite()), 5);
        Ok(())
    }

    #[test]
    fn try_except_else() -> Result<()> {
        let source: &str = r"
def f():
    try:
        print()
    except ValueError:
        pass
    else:
        print()
";
        let parsed = parse_module(source)?;
        assert_eq!(num_statements(parsed.suite()), 7);
        Ok(())
    }

    #[test]
    fn try_except_else_finally() -> Result<()> {
        let source: &str = r"
def f():
    try:
        print()
    except ValueError:
        pass
    else:
        print()
    finally:
        pass
";
        let parsed = parse_module(source)?;
        assert_eq!(num_statements(parsed.suite()), 10);
        Ok(())
    }

    #[test]
    fn try_except_except() -> Result<()> {
        let source: &str = r"
def f():
    try:
        print()
    except ValueError:
        pass
    except Exception:
        raise
";
        let parsed = parse_module(source)?;
        assert_eq!(num_statements(parsed.suite()), 8);
        Ok(())
    }

    #[test]
    fn try_except_except_finally() -> Result<()> {
        let source: &str = r"
def f():
    try:
        print()
    except:
        pass
    except:
        pass
    finally:
        print()
";
        let parsed = parse_module(source)?;
        assert_eq!(num_statements(parsed.suite()), 11);
        Ok(())
    }

    #[test]
    fn yield_() -> Result<()> {
        let source: &str = r"
def f():
    for i in range(10):
        yield i
";
        let parsed = parse_module(source)?;
        assert_eq!(num_statements(parsed.suite()), 2);
        Ok(())
    }
}
