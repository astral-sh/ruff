extern crate core;

use anyhow::Result;
use num_bigint::BigInt;
use num_traits::{float, Num};
use rustpython_ast::{
    Arguments, Constant, Expr, ExprContext, ExprKind, Keyword, KeywordData, Location, Operator,
    Stmt, StmtKind, Withitem,
};
use tree_sitter::{Node, Parser, Point};

fn to_location(point: Point) -> Location {
    Location::new(point.row + 1, point.column + 1)
}

fn print_node(node: Node, source: &[u8]) {
    let range = node.range();
    let text = &source[range.start_byte..range.end_byte];
    let line = range.start_point.row;
    let col = range.start_point.column;
    println!(
        "[Line: {}, Col: {}] {}: `{}`",
        line,
        col,
        node.kind(),
        std::str::from_utf8(text).unwrap()
    );
}

fn extract_module(node: Node, source: &[u8]) -> Vec<Stmt> {
    let mut cursor = node.walk();
    node.children(&mut cursor)
        .map(|node| extract_statement(node, source))
        .collect()
}

fn extract_suite(node: Node, source: &[u8]) -> Vec<Stmt> {
    let mut cursor = node.walk();
    node.children(&mut cursor)
        .map(|node| extract_statement(node, source))
        .collect()
}

fn extract_text(node: Node, source: &[u8]) -> String {
    let range = node.range();
    let text = &source[range.start_byte..range.end_byte];
    std::str::from_utf8(text).unwrap().to_string()
}

fn extract_augmented_operator(node: Node, source: &[u8]) -> Operator {
    match node.kind() {
        "+=" => Operator::Add,
        "-=" => Operator::Sub,
        "*=" => Operator::Mult,
        "@=" => Operator::MatMult,
        "/=" => Operator::Div,
        "%=" => Operator::Mod,
        "**=" => Operator::Pow,
        "<<=" => Operator::LShift,
        ">>=" => Operator::RShift,
        "|=" => Operator::BitOr,
        "^=" => Operator::BitXor,
        "&=" => Operator::BitAnd,
        "//=" => Operator::FloorDiv,
        _ => panic!("Invalid operator: {:?}", node),
    }
}

fn extract_operator(node: Node, source: &[u8]) -> Operator {
    match node.kind() {
        "+" => Operator::Add,
        "-" => Operator::Sub,
        "*" => Operator::Mult,
        "@" => Operator::MatMult,
        "/" => Operator::Div,
        "%" => Operator::Mod,
        "**" => Operator::Pow,
        "<<" => Operator::LShift,
        ">>" => Operator::RShift,
        "|" => Operator::BitOr,
        "^" => Operator::BitXor,
        "&" => Operator::BitAnd,
        "//" => Operator::FloorDiv,
        _ => panic!("Invalid operator: {:?}", node),
    }
}

fn extract_arguments(node: Node, source: &[u8]) -> Arguments {
    Arguments {
        posonlyargs: vec![],
        args: vec![],
        vararg: None,
        kwonlyargs: vec![],
        kw_defaults: vec![],
        kwarg: None,
        defaults: vec![],
    }
}

fn extract_with_clause(node: Node, source: &[u8]) -> Vec<Withitem> {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        print_node(child, source);
    }
    return vec![];
}

fn extract_statement(node: Node, source: &[u8]) -> Stmt {
    match node.kind() {
        "for_statement" => Stmt::new(
            to_location(node.start_position()),
            to_location(node.end_position()),
            StmtKind::For {
                target: Box::new(extract_expression(
                    node.child_by_field_name("left").unwrap(),
                    source,
                )),
                iter: Box::new(extract_expression(
                    node.child_by_field_name("right").unwrap(),
                    source,
                )),
                body: extract_suite(node.child_by_field_name("body").unwrap(), source),
                // STOPSHIP(charlie): Unimplemented.
                orelse: vec![],
                type_comment: None,
            },
        ),
        "while_statement" => Stmt::new(
            to_location(node.start_position()),
            to_location(node.end_position()),
            StmtKind::While {
                test: Box::new(extract_expression(
                    node.child_by_field_name("condition").unwrap(),
                    source,
                )),
                body: extract_suite(node.child_by_field_name("body").unwrap(), source),
                // STOPSHIP(charlie): Unimplemented.
                orelse: vec![],
            },
        ),
        "with_statement" => Stmt::new(
            to_location(node.start_position()),
            to_location(node.end_position()),
            StmtKind::With {
                // TODO(charlie): If async, this will be 2? Also, we need to iterate until we find
                // this, probably.
                items: extract_with_clause(node.child(1).unwrap(), source),
                body: extract_suite(node.child_by_field_name("body").unwrap(), source),
                type_comment: None,
            },
        ),
        "class_definition" => {
            if let Some((bases, keywords)) = node
                .child_by_field_name("superclasses")
                .map(|node| extract_argument_list(node, source))
            {
                Stmt::new(
                    to_location(node.start_position()),
                    to_location(node.end_position()),
                    StmtKind::ClassDef {
                        name: extract_text(node.child_by_field_name("name").unwrap(), source),
                        bases,
                        keywords,
                        body: extract_suite(node.child_by_field_name("body").unwrap(), source),
                        // TODO(charlie): How do I access these? Probably need to pass them down or
                        // recurse.
                        decorator_list: vec![],
                    },
                )
            } else {
                Stmt::new(
                    to_location(node.start_position()),
                    to_location(node.end_position()),
                    StmtKind::ClassDef {
                        name: extract_text(node.child_by_field_name("name").unwrap(), source),
                        bases: vec![],
                        keywords: vec![],
                        body: extract_suite(node.child_by_field_name("body").unwrap(), source),
                        // TODO(charlie): How do I access these? Probably need to pass them down or
                        // recurse.
                        decorator_list: vec![],
                    },
                )
            }
        }
        "function_definition" => Stmt::new(
            to_location(node.start_position()),
            to_location(node.end_position()),
            StmtKind::FunctionDef {
                name: extract_text(node.child(1).unwrap(), source),
                args: Box::new(extract_arguments(node.child(2).unwrap(), source)),
                body: extract_suite(node.child_by_field_name("body").unwrap(), source),
                decorator_list: vec![],
                returns: None,
                type_comment: None,
            },
        ),
        "return_statement" => Stmt::new(
            to_location(node.start_position()),
            to_location(node.end_position()),
            StmtKind::Return {
                value: node
                    .child(1)
                    .map(|node| Box::new(extract_expression(node, source))),
            },
        ),
        "pass_statement" => Stmt::new(
            to_location(node.start_position()),
            to_location(node.end_position()),
            StmtKind::Pass,
        ),
        "expression_statement" => {
            let node = node.child(0).unwrap();
            match node.kind() {
                "assignment" => Stmt::new(
                    to_location(node.start_position()),
                    to_location(node.end_position()),
                    StmtKind::Assign {
                        targets: vec![],
                        value: Box::new(extract_expression(node.child(2).unwrap(), source)),
                        type_comment: None,
                    },
                ),
                "augmented_assignment" => Stmt::new(
                    to_location(node.start_position()),
                    to_location(node.end_position()),
                    StmtKind::AugAssign {
                        target: Box::new(extract_expression(
                            node.child_by_field_name("left").unwrap(),
                            source,
                        )),
                        value: Box::new(extract_expression(
                            node.child_by_field_name("right").unwrap(),
                            source,
                        )),
                        op: extract_augmented_operator(
                            node.child_by_field_name("operator").unwrap(),
                            source,
                        ),
                    },
                ),
                _ => Stmt::new(
                    to_location(node.start_position()),
                    to_location(node.end_position()),
                    StmtKind::Expr {
                        value: Box::new(extract_expression(node, source)),
                    },
                ),
            }
        }
        _ => panic!("Unhandled node: {}", node.kind()),
    }
}

fn extract_expression_list(node: Node, source: &[u8]) -> Vec<Expr> {
    let mut cursor = node.walk();
    node.children(&mut cursor)
        .filter(|node| node.kind() != "(" && node.kind() != ")" && node.kind() != ",")
        .map(|node| extract_expression(node, source))
        .collect()
}

fn extract_keyword_argument(node: Node, source: &[u8]) -> Keyword {
    Keyword::new(
        Default::default(),
        Default::default(),
        KeywordData {
            arg: Some(extract_text(
                node.child_by_field_name("name").unwrap(),
                source,
            )),
            value: Box::new(extract_expression(
                node.child_by_field_name("value").unwrap(),
                source,
            )),
        },
    )
}

fn extract_argument_list(node: Node, source: &[u8]) -> (Vec<Expr>, Vec<Keyword>) {
    let mut args = vec![];
    let mut keywords = vec![];
    for child in node.children(&mut node.walk()) {
        match child.kind() {
            "keyword_argument" => {
                keywords.push(extract_keyword_argument(child, source));
            }
            "identifier" | "integer" => {
                args.push(extract_expression(child, source));
            }
            _ => {}
        }
    }
    (args, keywords)
}

fn extract_expression(node: Node, source: &[u8]) -> Expr {
    match node.kind() {
        "integer" => Expr::new(
            to_location(node.start_position()),
            to_location(node.end_position()),
            ExprKind::Constant {
                value: Constant::Int(
                    BigInt::from_str_radix(&extract_text(node, source), 10).unwrap(),
                ),
                kind: None,
            },
        ),
        "float" => Expr::new(
            to_location(node.start_position()),
            to_location(node.end_position()),
            ExprKind::Constant {
                value: Constant::Float(extract_text(node, source).parse::<f64>().unwrap()),
                kind: None,
            },
        ),
        "string" => Expr::new(
            to_location(node.start_position()),
            to_location(node.end_position()),
            ExprKind::Constant {
                value: Constant::Str(extract_text(node, source)),
                kind: None,
            },
        ),
        "tuple" => Expr::new(
            to_location(node.start_position()),
            to_location(node.end_position()),
            ExprKind::Tuple {
                elts: extract_expression_list(node, source),
                ctx: ExprContext::Load,
            },
        ),
        "identifier" => Expr::new(
            to_location(node.start_position()),
            to_location(node.end_position()),
            ExprKind::Name {
                id: std::str::from_utf8(&source[node.range().start_byte..node.range().end_byte])
                    .unwrap()
                    .to_string(),
                ctx: ExprContext::Load,
            },
        ),
        "call" => {
            let argument_list =
                extract_argument_list(node.child_by_field_name("arguments").unwrap(), source);
            Expr::new(
                to_location(node.start_position()),
                to_location(node.end_position()),
                ExprKind::Call {
                    func: Box::new(extract_expression(
                        node.child_by_field_name("function").unwrap(),
                        source,
                    )),
                    args: argument_list.0,
                    keywords: argument_list.1,
                },
            )
        }
        "binary_operator" => {
            print_node(node, source);
            Expr::new(
                to_location(node.start_position()),
                to_location(node.end_position()),
                ExprKind::BinOp {
                    left: Box::new(extract_expression(
                        node.child_by_field_name("left").unwrap(),
                        source,
                    )),
                    op: extract_operator(node.child_by_field_name("operator").unwrap(), source),
                    right: Box::new(extract_expression(
                        node.child_by_field_name("right").unwrap(),
                        source,
                    )),
                },
            )
        }
        "true" => Expr::new(
            to_location(node.start_position()),
            to_location(node.end_position()),
            ExprKind::Constant {
                value: Constant::Bool(true),
                kind: None,
            },
        ),
        "false" => Expr::new(
            to_location(node.start_position()),
            to_location(node.end_position()),
            ExprKind::Constant {
                value: Constant::Bool(false),
                kind: None,
            },
        ),
        "ellipsis" => Expr::new(
            to_location(node.start_position()),
            to_location(node.end_position()),
            ExprKind::Constant {
                value: Constant::Ellipsis,
                kind: None,
            },
        ),
        "yield" => match node.child(1) {
            None => Expr::new(
                to_location(node.start_position()),
                to_location(node.end_position()),
                ExprKind::Yield { value: None },
            ),
            Some(node) => match node.kind() {
                "from" => Expr::new(
                    to_location(node.start_position()),
                    to_location(node.end_position()),
                    ExprKind::YieldFrom {
                        value: Box::new(extract_expression(node.next_sibling().unwrap(), source)),
                    },
                ),
                _ => Expr::new(
                    to_location(node.start_position()),
                    to_location(node.end_position()),
                    ExprKind::Yield {
                        value: Some(Box::new(extract_expression(node, source))),
                    },
                ),
            },
        },
        _ => {
            print_node(node, source);
            panic!("Unhandled node: {}", node.kind())
        }
    }
}

fn main() -> Result<()> {
    let src = r#"
def double(x):
    # Return a double.
    return x * 2

x = (double(500), double(2, z=1))
x += 1

class Foo:
    pass

for x in range(5):
    yield x
    yield from x
    x = True
    x = b"abc"

while True:
    pass

with (
    foo as bar,
baz as wop):
    pass
"#;
    let mut parser = Parser::new();
    parser
        .set_language(tree_sitter_python::language())
        .expect("Error loading Python grammar");
    let parse_tree = parser.parse(src.as_bytes(), None);

    if let Some(parse_tree) = &parse_tree {
        let _ = extract_module(parse_tree.root_node(), src.as_bytes());
        // println!(
        //     "{:#?}",
        //     extract_module(parse_tree.root_node(), src.as_bytes())
        // );
    }

    Ok(())
}
