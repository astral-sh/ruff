use std::fmt::{Debug, Write};

use insta::assert_snapshot;

use ruff_python_ast::visitor::preorder::{PreorderVisitor, TraversalSignal};
use ruff_python_ast::{AnyNodeRef, BoolOp, CmpOp, Operator, Singleton, UnaryOp};
use ruff_python_parser::lexer::lex;
use ruff_python_parser::{parse_tokens, Mode};

#[test]
fn function_arguments() {
    let source = r"def a(b, c,/, d, e = 20, *args, named=5, other=20, **kwargs): pass";

    let trace = trace_preorder_visitation(source);

    assert_snapshot!(trace);
}

#[test]
fn function_positional_only_with_default() {
    let source = r"def a(b, c = 34,/, e = 20, *args): pass";

    let trace = trace_preorder_visitation(source);

    assert_snapshot!(trace);
}

#[test]
fn compare() {
    let source = r"4 < x < 5";

    let trace = trace_preorder_visitation(source);

    assert_snapshot!(trace);
}

#[test]
fn list_comprehension() {
    let source = "[x for x in numbers]";

    let trace = trace_preorder_visitation(source);

    assert_snapshot!(trace);
}

#[test]
fn dict_comprehension() {
    let source = "{x: x**2 for x in numbers}";

    let trace = trace_preorder_visitation(source);

    assert_snapshot!(trace);
}

#[test]
fn set_comprehension() {
    let source = "{x for x in numbers}";

    let trace = trace_preorder_visitation(source);

    assert_snapshot!(trace);
}

#[test]
fn match_class_pattern() {
    let source = r"
match x:
    case Point2D(0, 0):
        ...
    case Point3D(x=0, y=0, z=0):
        ...
";

    let trace = trace_preorder_visitation(source);

    assert_snapshot!(trace);
}

#[test]
fn decorators() {
    let source = r"
@decorator
def a():
    pass

@test
class A:
    pass
";

    let trace = trace_preorder_visitation(source);

    assert_snapshot!(trace);
}

#[test]
fn type_aliases() {
    let source = r"type X[T: str, U, *Ts, **P] = list[T]";

    let trace = trace_preorder_visitation(source);

    assert_snapshot!(trace);
}

#[test]
fn class_type_parameters() {
    let source = r"class X[T: str, U, *Ts, **P]: ...";

    let trace = trace_preorder_visitation(source);

    assert_snapshot!(trace);
}

#[test]
fn function_type_parameters() {
    let source = r"def X[T: str, U, *Ts, **P](): ...";

    let trace = trace_preorder_visitation(source);

    assert_snapshot!(trace);
}

#[test]
fn string_literals() {
    let source = r"'a' 'b' 'c'";

    let trace = trace_preorder_visitation(source);

    assert_snapshot!(trace);
}

#[test]
fn bytes_literals() {
    let source = r"b'a' b'b' b'c'";

    let trace = trace_preorder_visitation(source);

    assert_snapshot!(trace);
}

#[test]
fn f_strings() {
    let source = r"'pre' f'foo {bar:.{x}f} baz'";

    let trace = trace_preorder_visitation(source);

    assert_snapshot!(trace);
}

fn trace_preorder_visitation(source: &str) -> String {
    let tokens = lex(source, Mode::Module);
    let parsed = parse_tokens(tokens, source, Mode::Module, "test.py").unwrap();

    let mut visitor = RecordVisitor::default();
    visitor.visit_mod(&parsed);

    visitor.output
}

/// Emits a `tree` with a node for every visited AST node (labelled by the AST node's kind)
/// and leafs for attributes.
#[derive(Default)]
struct RecordVisitor {
    depth: usize,
    output: String,
}

impl RecordVisitor {
    fn emit(&mut self, text: &dyn Debug) {
        for _ in 0..self.depth {
            self.output.push_str("  ");
        }

        writeln!(self.output, "- {text:?}").unwrap();
    }
}

impl<'a> PreorderVisitor<'a> for RecordVisitor {
    fn enter_node(&mut self, node: AnyNodeRef<'a>) -> TraversalSignal {
        self.emit(&node.kind());
        self.depth += 1;

        TraversalSignal::Traverse
    }

    fn leave_node(&mut self, _node: AnyNodeRef<'a>) {
        self.depth -= 1;
    }

    fn visit_singleton(&mut self, singleton: &Singleton) {
        self.emit(&singleton);
    }

    fn visit_bool_op(&mut self, bool_op: &BoolOp) {
        self.emit(&bool_op);
    }

    fn visit_operator(&mut self, operator: &Operator) {
        self.emit(&operator);
    }

    fn visit_unary_op(&mut self, unary_op: &UnaryOp) {
        self.emit(&unary_op);
    }

    fn visit_cmp_op(&mut self, cmp_op: &CmpOp) {
        self.emit(&cmp_op);
    }
}
