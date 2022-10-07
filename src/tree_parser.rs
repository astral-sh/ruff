use anyhow::Result;
use itertools::any;
use num_bigint::BigInt;
use num_traits::Num;
use rustpython_ast::{
    Alias, AliasData, Arg, ArgData, Arguments, Boolop, Cmpop, Comprehension, Constant, Expr,
    ExprContext, ExprKind, Keyword, KeywordData, Location, Operator, Stmt, StmtKind, Unaryop,
    Withitem,
};
use tree_sitter::{Node, Point};

#[allow(dead_code)]
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

fn to_location(point: Point) -> Location {
    Location::new(point.row + 1, point.column + 1)
}

pub fn extract_module(node: Node, source: &[u8]) -> Result<Vec<Stmt>> {
    let mut cursor = node.walk();
    node.named_children(&mut cursor)
        .filter(|node| node.kind() != "comment")
        .map(|node| extract_statement(node, source))
        .collect()
}

fn extract_suite(node: Node, source: &[u8]) -> Result<Vec<Stmt>> {
    let mut cursor = node.walk();
    node.named_children(&mut cursor)
        .filter(|node| node.kind() != "comment")
        .map(|node| extract_statement(node, source))
        .collect()
}

fn extract_text(node: Node, source: &[u8]) -> String {
    let range = node.range();
    let text = &source[range.start_byte..range.end_byte];
    std::str::from_utf8(text).unwrap().to_string()
}

fn extract_augmented_operator(node: Node, _source: &[u8]) -> Operator {
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

fn extract_operator(node: Node, _source: &[u8]) -> Operator {
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

fn extract_parameters(node: Node, source: &[u8]) -> Result<Arguments> {
    let mut defaults = vec![];
    let mut kw_defaults = vec![];
    let mut kwonlyargs = vec![];
    let mut posonlyargs = vec![];
    let mut args = vec![];
    let mut vararg = None;
    let mut kwarg = None;
    let mut is_kwonly = false;
    for node in node.named_children(&mut node.walk()) {
        match node.kind() {
            "identifier" => {
                let arg = Arg::new(
                    to_location(node.start_position()),
                    to_location(node.end_position()),
                    ArgData {
                        arg: extract_text(node, source),
                        annotation: None,
                        type_comment: None,
                    },
                );
                if is_kwonly {
                    kwonlyargs.push(arg)
                } else {
                    args.push(arg)
                }
            }
            "default_parameter" => {
                let arg = node.named_child(0).unwrap();
                let default = node.named_child(1).unwrap();

                let arg = Arg::new(
                    to_location(arg.start_position()),
                    to_location(arg.end_position()),
                    ArgData {
                        arg: extract_text(arg, source),
                        annotation: None,
                        type_comment: None,
                    },
                );
                let default = extract_expression(default, source)?;

                if is_kwonly {
                    kwonlyargs.push(arg);
                    kw_defaults.push(default);
                } else {
                    args.push(arg);
                    defaults.push(default);
                }
            }
            "typed_parameter" => {
                let arg = node.named_child(0).unwrap();
                let _type = node.named_child(1).unwrap();

                let arg = Arg::new(
                    to_location(node.start_position()),
                    to_location(node.end_position()),
                    ArgData {
                        arg: extract_text(arg, source),
                        annotation: Some(Box::new(extract_expression(_type, source)?)),
                        type_comment: None,
                    },
                );

                if is_kwonly {
                    kwonlyargs.push(arg);
                } else {
                    args.push(arg);
                }
            }
            "typed_default_parameter" => {
                let arg = node.named_child(0).unwrap();
                let _type = node.named_child(1).unwrap();
                let default = node.named_child(2).unwrap();

                let arg = Arg::new(
                    to_location(node.start_position()),
                    to_location(node.end_position()),
                    ArgData {
                        arg: extract_text(arg, source),
                        annotation: Some(Box::new(extract_expression(_type, source)?)),
                        type_comment: None,
                    },
                );
                let default = extract_expression(default, source)?;

                if is_kwonly {
                    kwonlyargs.push(arg);
                    kw_defaults.push(default);
                } else {
                    args.push(arg);
                    defaults.push(default);
                }
            }
            "positional_separator" => {
                // Shift the positional arguments over to positional-only.
                while let Some(arg) = args.pop() {
                    posonlyargs.push(arg);
                }
                posonlyargs.reverse();
            }
            "keyword_separator" => {
                is_kwonly = true;
            }
            "list_splat_pattern" => {
                let arg = node.named_child(0).unwrap();
                let arg = Arg::new(
                    to_location(arg.start_position()),
                    to_location(arg.end_position()),
                    ArgData {
                        arg: extract_text(arg, source),
                        annotation: None,
                        type_comment: None,
                    },
                );
                vararg = Some(Box::new(arg));
            }
            "dictionary_splat_pattern" => {
                let arg = node.named_child(0).unwrap();
                let arg = Arg::new(
                    to_location(arg.start_position()),
                    to_location(arg.end_position()),
                    ArgData {
                        arg: extract_text(arg, source),
                        annotation: None,
                        type_comment: None,
                    },
                );
                kwarg = Some(Box::new(arg));
            }
            kind => {
                return Err(anyhow::anyhow!("Unexpected parameter kind: {}.", kind));
            }
        }
    }

    Ok(Arguments {
        posonlyargs,
        args,
        vararg,
        kwonlyargs,
        kw_defaults,
        kwarg,
        defaults,
    })
}

fn extract_with_clause(_node: Node, _source: &[u8]) -> Vec<Withitem> {
    vec![]
}

fn extract_import_list(node: Node, source: &[u8]) -> Vec<Alias> {
    let mut aliases = vec![];
    for node in node.children_by_field_name("name", &mut node.walk()) {
        // Alias.
        if let Some(asname) = node.child_by_field_name("alias") {
            let name = node.child_by_field_name("name").unwrap();
            aliases.push(Alias::new(
                to_location(node.start_position()),
                to_location(node.end_position()),
                AliasData {
                    name: extract_text(name, source),
                    asname: Some(extract_text(asname, source)),
                },
            ));
        } else {
            let name = node.named_child(0).unwrap();
            aliases.push(Alias::new(
                to_location(node.start_position()),
                to_location(node.end_position()),
                AliasData {
                    name: extract_text(name, source),
                    asname: None,
                },
            ));
        }
    }
    aliases
}

fn extract_statement(node: Node, source: &[u8]) -> Result<Stmt> {
    match node.kind() {
        "for_statement" => Ok(if node.child(0).unwrap().kind() == "async" {
            Stmt::new(
                to_location(node.start_position()),
                to_location(node.end_position()),
                StmtKind::AsyncFor {
                    target: Box::new(extract_expression(
                        node.child_by_field_name("left").unwrap(),
                        source,
                    )?),
                    iter: Box::new(extract_expression(
                        node.child_by_field_name("right").unwrap(),
                        source,
                    )?),
                    body: extract_suite(node.child_by_field_name("body").unwrap(), source)?,
                    orelse: node
                        .child_by_field_name("alternative")
                        .map(|node| {
                            extract_suite(node.child_by_field_name("body").unwrap(), source)
                        })
                        .transpose()?
                        .unwrap_or_default(),
                    type_comment: None,
                },
            )
        } else {
            Stmt::new(
                to_location(node.start_position()),
                to_location(node.end_position()),
                StmtKind::For {
                    target: Box::new(extract_expression(
                        node.child_by_field_name("left").unwrap(),
                        source,
                    )?),
                    iter: Box::new(extract_expression(
                        node.child_by_field_name("right").unwrap(),
                        source,
                    )?),
                    body: extract_suite(node.child_by_field_name("body").unwrap(), source)?,
                    orelse: node
                        .child_by_field_name("alternative")
                        .map(|node| {
                            extract_suite(node.child_by_field_name("body").unwrap(), source)
                        })
                        .transpose()?
                        .unwrap_or_default(),
                    type_comment: None,
                },
            )
        }),
        "while_statement" => Ok(Stmt::new(
            to_location(node.start_position()),
            to_location(node.end_position()),
            StmtKind::While {
                test: Box::new(extract_expression(
                    node.child_by_field_name("condition").unwrap(),
                    source,
                )?),
                body: extract_suite(node.child_by_field_name("body").unwrap(), source)?,
                orelse: node
                    .child_by_field_name("alternative")
                    .map(|node| extract_suite(node.child_by_field_name("body").unwrap(), source))
                    .transpose()?
                    .unwrap_or_default(),
            },
        )),
        "with_statement" => Ok(if node.child(0).unwrap().kind() == "async" {
            Stmt::new(
                to_location(node.start_position()),
                to_location(node.end_position()),
                StmtKind::AsyncWith {
                    items: extract_with_clause(node.named_child(1).unwrap(), source),
                    body: extract_suite(node.child_by_field_name("body").unwrap(), source)?,
                    type_comment: None,
                },
            )
        } else {
            Stmt::new(
                to_location(node.start_position()),
                to_location(node.end_position()),
                StmtKind::With {
                    items: extract_with_clause(node.named_child(1).unwrap(), source),
                    body: extract_suite(node.child_by_field_name("body").unwrap(), source)?,
                    type_comment: None,
                },
            )
        }),
        "if_statement" => Ok(Stmt::new(
            to_location(node.start_position()),
            to_location(node.end_position()),
            StmtKind::If {
                test: Box::new(extract_expression(
                    node.child_by_field_name("condition").unwrap(),
                    source,
                )?),
                body: extract_suite(node.child_by_field_name("consequence").unwrap(), source)?,
                // TODO(charlie): Unimplemented.
                orelse: vec![],
            },
        )),
        "class_definition" => {
            let (bases, keywords) = node
                .child_by_field_name("superclasses")
                .map(|node| extract_argument_list(node, source))
                .transpose()?
                .unwrap_or_default();

            Ok(Stmt::new(
                to_location(node.start_position()),
                to_location(node.end_position()),
                StmtKind::ClassDef {
                    name: extract_text(node.child_by_field_name("name").unwrap(), source),
                    bases,
                    keywords,
                    body: extract_suite(node.child_by_field_name("body").unwrap(), source)?,
                    // TODO(charlie): Unimplemented.
                    decorator_list: vec![],
                },
            ))
        }
        "function_definition" => Ok(Stmt::new(
            to_location(node.start_position()),
            to_location(node.end_position()),
            StmtKind::FunctionDef {
                name: extract_text(node.named_child(0).unwrap(), source),
                args: Box::new(extract_parameters(node.named_child(1).unwrap(), source)?),
                body: extract_suite(node.child_by_field_name("body").unwrap(), source)?,
                // TODO(charlie): Unimplemented.
                decorator_list: vec![],
                returns: None,
                type_comment: None,
            },
        )),
        "return_statement" => Ok(Stmt::new(
            to_location(node.start_position()),
            to_location(node.end_position()),
            StmtKind::Return {
                value: node
                    .child(1)
                    .map(|node| extract_expression(node, source))
                    .transpose()?
                    .map(Box::new),
            },
        )),
        "pass_statement" => Ok(Stmt::new(
            to_location(node.start_position()),
            to_location(node.end_position()),
            StmtKind::Pass,
        )),
        "continue_statement" => Ok(Stmt::new(
            to_location(node.start_position()),
            to_location(node.end_position()),
            StmtKind::Continue,
        )),
        "break_statement" => Ok(Stmt::new(
            to_location(node.start_position()),
            to_location(node.end_position()),
            StmtKind::Break,
        )),
        "import_statement" => Ok(Stmt::new(
            to_location(node.start_position()),
            to_location(node.end_position()),
            StmtKind::Import {
                names: extract_import_list(node, source),
            },
        )),
        "import_from_statement" | "future_import_statement" => {
            let mut cursor = node.walk();

            // Find the module name.
            let module: Option<String>;
            let level: Option<usize>;
            let child = node.named_child(0).unwrap();
            match child.kind() {
                "relative_import" => {
                    level = Some(extract_text(child.named_child(0).unwrap(), source).len());
                    module = child.named_child(1).map(|node| extract_text(node, source));
                }
                "dotted_name" => {
                    level = None;
                    module = Some(extract_text(child, source));
                }
                kind => {
                    return Err(anyhow::anyhow!(
                        "Expected relative_import or dotted_name; got: {}.",
                        kind
                    ));
                }
            }

            // Find the imports.
            let mut names: Vec<Alias> = vec![];
            for node in node.named_children(&mut cursor).skip(1) {
                match node.kind() {
                    "wildcard_import" => names.push(Alias::new(
                        to_location(node.start_position()),
                        to_location(node.end_position()),
                        AliasData {
                            name: "*".to_string(),
                            asname: None,
                        },
                    )),
                    "aliased_import" => names.push(Alias::new(
                        to_location(node.start_position()),
                        to_location(node.end_position()),
                        AliasData {
                            name: extract_text(node.child_by_field_name("name").unwrap(), source),
                            asname: Some(extract_text(
                                node.child_by_field_name("alias").unwrap(),
                                source,
                            )),
                        },
                    )),
                    "dotted_name" => names.push(Alias::new(
                        to_location(node.start_position()),
                        to_location(node.end_position()),
                        AliasData {
                            name: extract_text(node, source),
                            asname: None,
                        },
                    )),
                    kind => {
                        return Err(anyhow::anyhow!(
                            "Expected relative_import or dotted_name; got: {}.",
                            kind
                        ));
                    }
                }
            }

            Ok(Stmt::new(
                to_location(node.start_position()),
                to_location(node.end_position()),
                StmtKind::ImportFrom {
                    module,
                    names,
                    level,
                },
            ))
        }
        "expression_statement" => {
            let node = node.named_child(0).unwrap();
            match node.kind() {
                "assignment" => Ok(if let Some(_type) = node.child_by_field_name("type") {
                    Stmt::new(
                        to_location(node.start_position()),
                        to_location(node.end_position()),
                        StmtKind::AnnAssign {
                            target: Box::new(extract_expression(
                                node.child_by_field_name("left").unwrap(),
                                source,
                            )?),
                            annotation: Box::new(extract_expression(_type, source)?),
                            value: node
                                .child_by_field_name("right")
                                .map(|node| extract_expression(node, source))
                                .transpose()?
                                .map(Box::new),
                            // TODO(charlie): Unimplemented.
                            simple: 0,
                        },
                    )
                } else {
                    let mut targets = vec![extract_expression(
                        node.child_by_field_name("left").unwrap(),
                        source,
                    )?];
                    targets.extend(extract_expression_or_list(
                        node.child_by_field_name("right").unwrap(),
                        source,
                    )?);
                    let value = Box::new(targets.pop().unwrap());
                    Stmt::new(
                        to_location(node.start_position()),
                        to_location(node.end_position()),
                        StmtKind::Assign {
                            targets,
                            value,
                            type_comment: None,
                        },
                    )
                }),
                "augmented_assignment" => Ok(Stmt::new(
                    to_location(node.start_position()),
                    to_location(node.end_position()),
                    StmtKind::AugAssign {
                        target: Box::new(extract_expression(
                            node.child_by_field_name("left").unwrap(),
                            source,
                        )?),
                        value: Box::new(extract_expression(
                            node.child_by_field_name("right").unwrap(),
                            source,
                        )?),
                        op: extract_augmented_operator(
                            node.child_by_field_name("operator").unwrap(),
                            source,
                        ),
                    },
                )),
                _ => Ok(Stmt::new(
                    to_location(node.start_position()),
                    to_location(node.end_position()),
                    StmtKind::Expr {
                        value: Box::new(extract_expression(node, source)?),
                    },
                )),
            }
        }
        "try_statement" => Ok(Stmt::new(
            to_location(node.start_position()),
            to_location(node.end_position()),
            StmtKind::Try {
                body: extract_suite(node.child_by_field_name("body").unwrap(), source)?,
                // TODO(charlie): Unimplemented.
                handlers: vec![],
                orelse: vec![],
                finalbody: vec![],
            },
        )),
        "raise_statement" => Ok(Stmt::new(
            to_location(node.start_position()),
            to_location(node.end_position()),
            StmtKind::Raise {
                exc: node
                    .named_child(0)
                    .map(|node| extract_expression(node, source))
                    .transpose()?
                    .map(Box::new),
                cause: node
                    .named_child(1)
                    .map(|node| extract_expression(node, source))
                    .transpose()?
                    .map(Box::new),
            },
        )),
        "decorated_definition" => {
            extract_statement(node.child_by_field_name("definition").unwrap(), source)
        }
        "global_statement" => Ok(Stmt::new(
            to_location(node.start_position()),
            to_location(node.end_position()),
            StmtKind::Global {
                names: node
                    .named_children(&mut node.walk())
                    .map(|node| extract_text(node, source))
                    .collect(),
            },
        )),
        "nonlocal_statement" => Ok(Stmt::new(
            to_location(node.start_position()),
            to_location(node.end_position()),
            StmtKind::Nonlocal {
                names: node
                    .named_children(&mut node.walk())
                    .map(|node| extract_text(node, source))
                    .collect(),
            },
        )),
        "delete_statement" => Ok(Stmt::new(
            to_location(node.start_position()),
            to_location(node.end_position()),
            StmtKind::Delete {
                targets: node
                    .named_children(&mut node.walk())
                    .map(|node| extract_expression(node, source))
                    .collect::<Result<Vec<Expr>>>()?,
            },
        )),
        "assert_statement" => Ok(Stmt::new(
            to_location(node.start_position()),
            to_location(node.end_position()),
            StmtKind::Assert {
                test: Box::new(extract_expression(node.named_child(0).unwrap(), source)?),
                msg: node
                    .named_child(1)
                    .map(|node| Box::new(extract_expression(node, source).unwrap())),
            },
        )),
        "print_statement" => Ok(Stmt::new(
            to_location(node.start_position()),
            to_location(node.end_position()),
            StmtKind::Expr {
                value: Box::new(Expr::new(
                    to_location(node.start_position()),
                    to_location(node.end_position()),
                    ExprKind::Call {
                        func: Box::new(Expr::new(
                            to_location(node.start_position()),
                            to_location(node.end_position()),
                            ExprKind::Name {
                                id: "print".to_string(),
                                // TODO(charlie): Track context.
                                ctx: ExprContext::Load,
                            },
                        )),
                        // TODO(charlie): Unimplemented.
                        args: vec![],
                        keywords: vec![],
                    },
                )),
            },
        )),
        kind => Err(anyhow::anyhow!("Unhandled statement kind: {}", kind)),
    }
}

fn extract_expression_list(node: Node, source: &[u8]) -> Result<Vec<Expr>> {
    let mut cursor = node.walk();
    node.named_children(&mut cursor)
        .filter(|node| node.kind() != "comment")
        .map(|node| extract_expression(node, source))
        .collect()
}

fn extract_expression_or_list(node: Node, source: &[u8]) -> Result<Vec<Expr>> {
    match node.kind() {
        "expression_list"
        | "pattern_list"
        | "tuple_pattern"
        | "named_expression"
        | "integer"
        | "float"
        | "concatenated_string"
        | "string"
        | "tuple"
        | "identifier"
        | "call"
        | "generator_expression"
        | "binary_operator"
        | "unary_operator"
        | "true"
        | "false"
        | "none"
        | "not_operator"
        | "boolean_operator"
        | "comparison_operator"
        | "ellipsis"
        | "yield"
        | "await"
        | "list_comprehension"
        | "set_comprehension"
        | "dictionary_comprehension"
        | "list"
        | "set"
        | "list_pattern"
        | "list_splat"
        | "list_splat_pattern"
        | "dictionary"
        | "type"
        | "subscript"
        | "attribute"
        | "lambda"
        | "slice"
        | "parenthesized_expression"
        | "conditional_expression" => Ok(vec![extract_expression(node, source)?]),
        _ => extract_expression_list(node, source),
    }
}

fn extract_keyword_argument(node: Node, source: &[u8]) -> Result<Keyword> {
    Ok(Keyword::new(
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
            )?),
        },
    ))
}

fn extract_argument_list(node: Node, source: &[u8]) -> Result<(Vec<Expr>, Vec<Keyword>)> {
    let mut args = vec![];
    let mut keywords = vec![];
    for child in node.named_children(&mut node.walk()) {
        match child.kind() {
            "keyword_argument" => {
                keywords.push(extract_keyword_argument(child, source)?);
            }
            _ => args.push(extract_expression(child, source)?),
        }
    }
    Ok((args, keywords))
}

fn extract_pair(node: Node, source: &[u8]) -> Result<(Expr, Expr)> {
    Ok((
        extract_expression(node.child_by_field_name("key").unwrap(), source)?,
        extract_expression(node.child_by_field_name("value").unwrap(), source)?,
    ))
}

fn extract_generators(node: Node, source: &[u8]) -> Result<Vec<Comprehension>> {
    let mut generators: Vec<Comprehension> = vec![];
    for node in node.named_children(&mut node.walk()).skip(1) {
        if node.child_by_field_name("left").is_some() {
            generators.push(Comprehension {
                target: Box::new(extract_expression(
                    node.child_by_field_name("left").unwrap(),
                    source,
                )?),
                iter: Box::new(extract_expression(
                    node.child_by_field_name("right").unwrap(),
                    source,
                )?),
                is_async: if node.child(0).unwrap().kind() == "async" {
                    1
                } else {
                    0
                },
                ifs: vec![],
            });
        } else {
            generators
                .last_mut()
                .unwrap()
                .ifs
                .push(extract_expression(node.named_child(0).unwrap(), source)?);
        }
    }
    Ok(generators)
}

fn extract_expression(node: Node, source: &[u8]) -> Result<Expr> {
    match node.kind() {
        "expression_list" | "pattern_list" | "tuple_pattern" => Ok(Expr::new(
            to_location(node.start_position()),
            to_location(node.end_position()),
            ExprKind::Tuple {
                elts: extract_expression_list(node, source)?,
                // TODO(charlie): Track context.
                ctx: ExprContext::Load,
            },
        )),
        "named_expression" => Ok(Expr::new(
            to_location(node.start_position()),
            to_location(node.end_position()),
            ExprKind::NamedExpr {
                target: Box::new(extract_expression(
                    node.child_by_field_name("name").unwrap(),
                    source,
                )?),
                value: Box::new(extract_expression(
                    node.child_by_field_name("value").unwrap(),
                    source,
                )?),
            },
        )),
        "integer" => {
            let text = extract_text(node, source);
            for (pattern, radix) in [
                ("0x", 16),
                ("0X", 16),
                ("0o", 8),
                ("0O", 8),
                ("0b", 2),
                ("0B", 2),
            ] {
                if let Some(remainder) = text.strip_prefix(pattern) {
                    return Ok(Expr::new(
                        to_location(node.start_position()),
                        to_location(node.end_position()),
                        ExprKind::Constant {
                            value: Constant::Int(BigInt::from_str_radix(remainder, radix).unwrap()),
                            kind: None,
                        },
                    ));
                }
            }

            for pattern in ['j', 'J'] {
                if let Some(remainder) = text.strip_suffix(pattern) {
                    return Ok(Expr::new(
                        to_location(node.start_position()),
                        to_location(node.end_position()),
                        ExprKind::Constant {
                            value: Constant::Complex {
                                real: 0.,
                                imag: remainder.parse::<f64>().unwrap(),
                            },
                            kind: None,
                        },
                    ));
                }
            }

            Ok(Expr::new(
                to_location(node.start_position()),
                to_location(node.end_position()),
                ExprKind::Constant {
                    value: Constant::Int(BigInt::from_str_radix(&text, 10).unwrap()),
                    kind: None,
                },
            ))
        }
        "float" => {
            let text = extract_text(node, source);

            for pattern in ['j', 'J'] {
                if let Some(remainder) = text.strip_suffix(pattern) {
                    return Ok(Expr::new(
                        to_location(node.start_position()),
                        to_location(node.end_position()),
                        ExprKind::Constant {
                            value: Constant::Complex {
                                real: 0.,
                                imag: remainder.parse::<f64>().unwrap(),
                            },
                            kind: None,
                        },
                    ));
                }
            }

            Ok(Expr::new(
                to_location(node.start_position()),
                to_location(node.end_position()),
                ExprKind::Constant {
                    value: Constant::Float(text.parse::<f64>().unwrap()),
                    kind: None,
                },
            ))
        }
        "concatenated_string" => Ok(Expr::new(
            to_location(node.start_position()),
            to_location(node.end_position()),
            ExprKind::JoinedStr {
                values: extract_expression_list(node, source)?,
            },
        )),
        "string" => Ok(Expr::new(
            to_location(node.start_position()),
            to_location(node.end_position()),
            ExprKind::Constant {
                value: Constant::Str(extract_text(node, source)),
                // TODO(charlie): Unimplemented.
                kind: None,
            },
        )),
        "tuple" => Ok(Expr::new(
            to_location(node.start_position()),
            to_location(node.end_position()),
            ExprKind::Tuple {
                elts: extract_expression_list(node, source)?,
                // TODO(charlie): Track context.
                ctx: ExprContext::Load,
            },
        )),
        "identifier" => Ok(Expr::new(
            to_location(node.start_position()),
            to_location(node.end_position()),
            ExprKind::Name {
                id: std::str::from_utf8(&source[node.range().start_byte..node.range().end_byte])
                    .unwrap()
                    .to_string(),
                // TODO(charlie): Track context.
                ctx: ExprContext::Load,
            },
        )),
        "call" => {
            let argument_list =
                extract_argument_list(node.child_by_field_name("arguments").unwrap(), source)?;
            Ok(Expr::new(
                to_location(node.start_position()),
                to_location(node.end_position()),
                ExprKind::Call {
                    func: Box::new(extract_expression(
                        node.child_by_field_name("function").unwrap(),
                        source,
                    )?),
                    args: argument_list.0,
                    keywords: argument_list.1,
                },
            ))
        }
        "generator_expression" => Ok(Expr::new(
            to_location(node.start_position()),
            to_location(node.end_position()),
            ExprKind::GeneratorExp {
                elt: Box::new(extract_expression(
                    node.child_by_field_name("body").unwrap(),
                    source,
                )?),
                generators: extract_generators(node, source)?,
            },
        )),
        "binary_operator" => Ok(Expr::new(
            to_location(node.start_position()),
            to_location(node.end_position()),
            ExprKind::BinOp {
                left: Box::new(extract_expression(
                    node.child_by_field_name("left").unwrap(),
                    source,
                )?),
                op: extract_operator(node.child_by_field_name("operator").unwrap(), source),
                right: Box::new(extract_expression(
                    node.child_by_field_name("right").unwrap(),
                    source,
                )?),
            },
        )),
        "unary_operator" => Ok(Expr::new(
            to_location(node.start_position()),
            to_location(node.end_position()),
            ExprKind::UnaryOp {
                op: match node.child_by_field_name("operator").unwrap().kind() {
                    "+" => Unaryop::UAdd,
                    "-" => Unaryop::USub,
                    "~" => Unaryop::Invert,
                    op => panic!("Invalid unary operator: {}", op),
                },
                operand: Box::new(extract_expression(
                    node.child_by_field_name("argument").unwrap(),
                    source,
                )?),
            },
        )),
        "true" => Ok(Expr::new(
            to_location(node.start_position()),
            to_location(node.end_position()),
            ExprKind::Constant {
                value: Constant::Bool(true),
                kind: None,
            },
        )),
        "false" => Ok(Expr::new(
            to_location(node.start_position()),
            to_location(node.end_position()),
            ExprKind::Constant {
                value: Constant::Bool(false),
                kind: None,
            },
        )),
        "none" => Ok(Expr::new(
            to_location(node.start_position()),
            to_location(node.end_position()),
            ExprKind::Constant {
                value: Constant::None,
                kind: None,
            },
        )),
        "not_operator" => Ok(Expr::new(
            to_location(node.start_position()),
            to_location(node.end_position()),
            ExprKind::UnaryOp {
                op: Unaryop::Not,
                operand: Box::new(extract_expression(
                    node.child_by_field_name("argument").unwrap(),
                    source,
                )?),
            },
        )),
        "boolean_operator" => Ok(Expr::new(
            to_location(node.start_position()),
            to_location(node.end_position()),
            ExprKind::BoolOp {
                op: match node.child_by_field_name("operator").unwrap().kind() {
                    "and" => Boolop::And,
                    "or" => Boolop::Or,
                    op => panic!("Invalid boolean operator: {}", op),
                },
                values: vec![
                    extract_expression(node.child_by_field_name("left").unwrap(), source)?,
                    extract_expression(node.child_by_field_name("right").unwrap(), source)?,
                ],
            },
        )),
        "comparison_operator" => {
            let mut cursor = node.walk();

            // Find the left name.
            let left = Box::new(extract_expression(node.named_child(0).unwrap(), source)?);

            // Find the comparators.
            let ops: Vec<Cmpop> = node
                .children_by_field_name("operators", &mut cursor)
                .map(|node| match node.kind() {
                    ">" => Ok(Cmpop::Gt),
                    "<" => Ok(Cmpop::Lt),
                    ">=" => Ok(Cmpop::GtE),
                    "<=" => Ok(Cmpop::LtE),
                    kind => Err(anyhow::anyhow!("Unhandled operator kind: {}", kind)),
                })
                .collect::<Result<Vec<Cmpop>>>()?;

            // Find the other operators.
            let comparators: Vec<Expr> = node
                .named_children(&mut cursor)
                .skip(1)
                .map(|node| extract_expression(node, source))
                .collect::<Result<Vec<Expr>>>()?;

            Ok(Expr::new(
                to_location(node.start_position()),
                to_location(node.end_position()),
                ExprKind::Compare {
                    left,
                    ops,
                    comparators,
                },
            ))
        }
        "ellipsis" => Ok(Expr::new(
            to_location(node.start_position()),
            to_location(node.end_position()),
            ExprKind::Constant {
                value: Constant::Ellipsis,
                kind: None,
            },
        )),
        "yield" => match node.named_child(1) {
            None => Ok(Expr::new(
                to_location(node.start_position()),
                to_location(node.end_position()),
                ExprKind::Yield { value: None },
            )),
            Some(node) => match node.kind() {
                "from" => Ok(Expr::new(
                    to_location(node.start_position()),
                    to_location(node.end_position()),
                    ExprKind::YieldFrom {
                        value: Box::new(extract_expression(node.next_sibling().unwrap(), source)?),
                    },
                )),
                _ => Ok(Expr::new(
                    to_location(node.start_position()),
                    to_location(node.end_position()),
                    ExprKind::Yield {
                        value: Some(Box::new(extract_expression(node, source)?)),
                    },
                )),
            },
        },
        "await" => Ok(Expr::new(
            to_location(node.start_position()),
            to_location(node.end_position()),
            ExprKind::Await {
                value: Box::new(extract_expression(node.named_child(0).unwrap(), source)?),
            },
        )),
        "list_comprehension" => Ok(Expr::new(
            to_location(node.start_position()),
            to_location(node.end_position()),
            ExprKind::ListComp {
                elt: Box::new(extract_expression(
                    node.child_by_field_name("body").unwrap(),
                    source,
                )?),
                generators: extract_generators(node, source)?,
            },
        )),
        "set_comprehension" => Ok(Expr::new(
            to_location(node.start_position()),
            to_location(node.end_position()),
            ExprKind::SetComp {
                elt: Box::new(extract_expression(
                    node.child_by_field_name("body").unwrap(),
                    source,
                )?),
                generators: extract_generators(node, source)?,
            },
        )),
        "dictionary_comprehension" => {
            let (key, value) = extract_pair(node.child_by_field_name("body").unwrap(), source)?;
            Ok(Expr::new(
                to_location(node.start_position()),
                to_location(node.end_position()),
                ExprKind::DictComp {
                    key: Box::new(key),
                    value: Box::new(value),
                    generators: extract_generators(node, source)?,
                },
            ))
        }
        "list" | "set" => Ok(Expr::new(
            to_location(node.start_position()),
            to_location(node.end_position()),
            ExprKind::List {
                elts: node
                    .named_children(&mut node.walk())
                    .map(|node| extract_expression(node, source))
                    .collect::<Result<Vec<Expr>>>()?,
                // TODO(charlie): Track context.
                ctx: ExprContext::Load,
            },
        )),
        "list_pattern" => Ok(Expr::new(
            to_location(node.start_position()),
            to_location(node.end_position()),
            ExprKind::List {
                elts: node
                    .named_children(&mut node.walk())
                    .map(|node| extract_expression(node, source))
                    .collect::<Result<Vec<Expr>>>()?,
                // TODO(charlie): Track context.
                ctx: ExprContext::Load,
            },
        )),
        "list_splat" | "list_splat_pattern" => Ok(Expr::new(
            to_location(node.start_position()),
            to_location(node.end_position()),
            ExprKind::Starred {
                value: Box::new(extract_expression(node.named_child(0).unwrap(), source)?),
                // TODO(charlie): Track context.
                ctx: ExprContext::Load,
            },
        )),
        "dictionary" => Ok(Expr::new(
            to_location(node.start_position()),
            to_location(node.end_position()),
            ExprKind::Dict {
                // TODO(charlie): Unimplemented.
                keys: vec![],
                values: vec![],
            },
        )),
        "type" => Ok(Expr::new(
            to_location(node.start_position()),
            to_location(node.end_position()),
            ExprKind::Name {
                id: extract_text(node, source),
                // TODO(charlie): Track context.
                ctx: ExprContext::Load,
            },
        )),
        "subscript" => Ok(Expr::new(
            to_location(node.start_position()),
            to_location(node.end_position()),
            ExprKind::Subscript {
                value: Box::new(extract_expression(
                    node.child_by_field_name("value").unwrap(),
                    source,
                )?),
                slice: Box::new(extract_expression(
                    node.child_by_field_name("subscript").unwrap(),
                    source,
                )?),
                // TODO(charlie): Track context.
                ctx: ExprContext::Load,
            },
        )),
        "attribute" => Ok(Expr::new(
            to_location(node.start_position()),
            to_location(node.end_position()),
            ExprKind::Attribute {
                value: Box::new(extract_expression(
                    node.child_by_field_name("object").unwrap(),
                    source,
                )?),
                attr: extract_text(node.child_by_field_name("attribute").unwrap(), source),
                // TODO(charlie): Track context.
                ctx: ExprContext::Load,
            },
        )),
        "lambda" => Ok(Expr::new(
            to_location(node.start_position()),
            to_location(node.end_position()),
            ExprKind::Lambda {
                args: node
                    .child_by_field_name("parameters")
                    .map(|node| extract_parameters(node, source))
                    .transpose()?
                    .map(Box::new)
                    .unwrap_or_else(|| {
                        Box::new(Arguments {
                            posonlyargs: vec![],
                            args: vec![],
                            vararg: None,
                            kwonlyargs: vec![],
                            kw_defaults: vec![],
                            kwarg: None,
                            defaults: vec![],
                        })
                    }),
                body: Box::new(extract_expression(
                    node.child_by_field_name("body").unwrap(),
                    source,
                )?),
            },
        )),
        "slice" => Ok(Expr::new(
            to_location(node.start_position()),
            to_location(node.end_position()),
            ExprKind::Slice {
                lower: node
                    .named_child(0)
                    .map(|node| extract_expression(node, source))
                    .transpose()?
                    .map(Box::new),
                upper: node
                    .named_child(1)
                    .map(|node| extract_expression(node, source))
                    .transpose()?
                    .map(Box::new),
                step: node
                    .named_child(2)
                    .map(|node| extract_expression(node, source))
                    .transpose()?
                    .map(Box::new),
            },
        )),
        "parenthesized_expression" => {
            for child in node.named_children(&mut node.walk()) {
                if child.kind() != "comment" {
                    return extract_expression(child, source);
                }
            }
            Err(anyhow::anyhow!(
                "Unable to find expression within parentheses."
            ))
        }
        "conditional_expression" => Ok(Expr::new(
            to_location(node.start_position()),
            to_location(node.end_position()),
            ExprKind::IfExp {
                test: Box::new(extract_expression(node.named_child(0).unwrap(), source)?),
                body: Box::new(extract_expression(node.named_child(1).unwrap(), source)?),
                orelse: Box::new(extract_expression(node.named_child(2).unwrap(), source)?),
            },
        )),
        kind => Err(anyhow::anyhow!("Unhandled expression kind: {}", kind)),
    }
}
