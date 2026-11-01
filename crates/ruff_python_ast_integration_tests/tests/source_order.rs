use std::fmt::{Debug, Write};

use insta::assert_snapshot;

use ruff_python_ast::visitor::source_order::{self, SourceOrderVisitor, TraversalSignal};
use ruff_python_ast::{
    AnyKind, AstNode, BoolOp, CmpOp, DictItem, Expr, Operator, Singleton, Suite, SyntaxNode,
    UnaryOp,
};
use ruff_python_parser::{Mode, ParseOptions, parse, parse_expression, parse_unchecked};
use ruff_text_size::Ranged;

#[test]
fn function_arguments() {
    let source = r"def a(b, c,/, d, e = 20, *args, named=5, other=20, **kwargs): pass";

    let trace = trace_source_order_visitation(source);

    assert_snapshot!(trace);
}

#[test]
fn function_positional_only_with_default() {
    let source = r"def a(b, c = 34,/, e = 20, *args): pass";

    let trace = trace_source_order_visitation(source);

    assert_snapshot!(trace);
}

#[test]
fn compare() {
    let source = r"4 < x < 5";

    let trace = trace_source_order_visitation(source);

    assert_snapshot!(trace);
}

#[test]
fn list_comprehension() {
    let source = "[x for x in numbers]";

    let trace = trace_source_order_visitation(source);

    assert_snapshot!(trace);
}

#[test]
fn dict_comprehension() {
    let source = "{x: x**2 for x in numbers}";

    let trace = trace_source_order_visitation(source);

    assert_snapshot!(trace);
}

#[test]
fn set_comprehension() {
    let source = "{x for x in numbers}";

    let trace = trace_source_order_visitation(source);

    assert_snapshot!(trace);
}

#[test]
fn dictionary_entry_ranges() {
    #[derive(Default)]
    struct Entries<'a>(Vec<DictItem<'a>>);

    impl<'a> SourceOrderVisitor<'a> for Entries<'a> {
        fn enter_node(&mut self, node: SyntaxNode<'a>) -> TraversalSignal {
            if let AnyKind::DictItem(entry) = node.classify() {
                self.0.push(entry);
            }
            TraversalSignal::Traverse
        }
    }

    for (source, valid, expected) in [
        (
            "{(key): (value), **(rest)}",
            true,
            &[("(key): (value)", "value"), ("**(rest)", "rest")][..],
        ),
        (
            "match x:\n    case {**  # capture\n          rest} + 1: pass\n",
            false,
            &[("**  # capture\n          rest", "rest")][..],
        ),
    ] {
        let parsed = parse_unchecked(source, ParseOptions::from(Mode::Module));
        assert_eq!(parsed.errors().is_empty(), valid);
        let mut entries = Entries::default();
        entries.visit_mod(parsed.syntax());
        let ranges: Vec<_> = entries
            .0
            .iter()
            .map(|entry| (&source[entry.range()], &source[entry.value().range()]))
            .collect();
        assert_eq!(ranges, expected);
    }
}

#[test]
fn mapping_pattern_with_misplaced_captures() {
    let source = "match x:\n    case {'a': a, **first, 'b': b, **second}: pass";
    let parsed = parse_unchecked(source, ParseOptions::from(Mode::Module));
    assert!(!parsed.errors().is_empty());

    let mut visitor = RecordVisitor::default();
    visitor.visit_mod(parsed.syntax());
    assert_snapshot!(visitor.output);
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

    let trace = trace_source_order_visitation(source);

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

    let trace = trace_source_order_visitation(source);

    assert_snapshot!(trace);
}

#[test]
fn type_aliases() {
    let source = r"type X[T: str, U, *Ts, **P] = list[T]";

    let trace = trace_source_order_visitation(source);

    assert_snapshot!(trace);
}

#[test]
fn class_type_parameters() {
    let source = r"class X[T: str, U, *Ts, **P]: ...";

    let trace = trace_source_order_visitation(source);

    assert_snapshot!(trace);
}

#[test]
fn function_type_parameters() {
    let source = r"def X[T: str, U, *Ts, **P](): ...";

    let trace = trace_source_order_visitation(source);

    assert_snapshot!(trace);
}

#[test]
fn string_literals() {
    let source = r"'a' 'b' 'c'";

    let trace = trace_source_order_visitation(source);

    assert_snapshot!(trace);
}

#[test]
fn bytes_literals() {
    let source = r"b'a' b'b' b'c'";

    let trace = trace_source_order_visitation(source);

    assert_snapshot!(trace);
}

#[test]
fn f_string_parts_from_either_end() {
    for (source, starts) in [("f''", &[0][..]), ("f'' 'x' f''", &[0, 4, 8])] {
        let parsed = parse_expression(source).unwrap();
        let mut parts = parsed.expr().as_f_string_expr().unwrap().value().iter();
        let mut expected = starts.iter().copied();
        while let Some(first) = expected.next() {
            assert_eq!(parts.len(), expected.len() + 1);
            assert_eq!(parts.next().map(|part| part.start().to_u32()), Some(first));
            assert_eq!(parts.len(), expected.len());
            assert_eq!(
                parts.next_back().map(|part| part.start().to_u32()),
                expected.next_back()
            );
        }
        assert_eq!(parts.len(), 0);
        assert!(parts.next().is_none());
        assert!(parts.next_back().is_none());
    }
}

#[test]
fn f_strings() {
    let source = r"'pre' f'foo {bar:.{x}f} baz'";

    let trace = trace_source_order_visitation(source);

    assert_snapshot!(trace);
}

#[test]
fn t_strings() {
    let source = r"t'pre' t'foo {bar:.{x}f} baz'";

    let trace = trace_source_order_visitation(source);

    assert_snapshot!(trace);
}

fn trace_source_order_visitation(source: &str) -> String {
    let parsed = parse(source, ParseOptions::from(Mode::Module)).unwrap();

    let mut visitor = RecordVisitor::default();
    visitor.visit_mod(parsed.syntax());

    visitor.output
}

#[test]
fn suites_and_annotations() {
    #[derive(Default)]
    struct Roles {
        suites: Vec<usize>,
        annotations: Vec<String>,
    }
    impl<'a> SourceOrderVisitor<'a> for Roles {
        fn visit_body(&mut self, body: Suite<'a>) {
            self.suites.push(body.len());
            source_order::walk_body(self, body);
        }

        fn visit_annotation(&mut self, expr: Expr<'a>) {
            if let Some(name) = expr.as_name_expr() {
                self.annotations.push(name.id().to_string());
            }
            source_order::walk_annotation(self, expr);
        }
    }

    let parsed = parse(
        "def f(x: A) -> B:\n    y: C = x\n    return y\n",
        ParseOptions::from(Mode::Module),
    )
    .unwrap();
    let mut roles = Roles::default();
    roles.visit_mod(parsed.syntax());
    assert_eq!(roles.suites, [1, 2]);
    assert_eq!(roles.annotations, ["A", "B", "C"]);
}

/// Emits a `tree` with a node for every visited AST node (labelled by the AST node's kind)
/// and leaves for attributes.
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

impl<'a> SourceOrderVisitor<'a> for RecordVisitor {
    fn enter_node(&mut self, node: SyntaxNode<'a>) -> TraversalSignal {
        self.emit(&node.node_kind());
        self.depth += 1;

        TraversalSignal::Traverse
    }

    fn leave_node(&mut self, _node: SyntaxNode<'a>) {
        self.depth -= 1;
    }

    fn visit_singleton(&mut self, singleton: Singleton) {
        self.emit(&singleton);
    }

    fn visit_bool_op(&mut self, bool_op: BoolOp) {
        self.emit(&bool_op);
    }

    fn visit_operator(&mut self, operator: Operator) {
        self.emit(&operator);
    }

    fn visit_unary_op(&mut self, unary_op: UnaryOp) {
        self.emit(&unary_op);
    }

    fn visit_cmp_op(&mut self, cmp_op: CmpOp) {
        self.emit(&cmp_op);
    }
}
