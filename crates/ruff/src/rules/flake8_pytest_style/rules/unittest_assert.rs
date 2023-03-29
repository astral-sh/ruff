use std::hash::BuildHasherDefault;

use anyhow::{anyhow, bail, Result};
use rustc_hash::FxHashMap;
use rustpython_parser::ast::{
    Cmpop, Constant, Expr, ExprContext, ExprKind, Keyword, Stmt, StmtKind, Unaryop,
};

use ruff_python_ast::helpers::{create_expr, create_stmt};

/// An enum to represent the different types of assertions present in the
/// `unittest` module. Note: any variants that can't be replaced with plain
/// `assert` statements are commented out.
#[derive(Copy, Clone)]
pub enum UnittestAssert {
    AlmostEqual,
    AlmostEquals,
    CountEqual,
    DictContainsSubset,
    DictEqual,
    Equal,
    Equals,
    False,
    Greater,
    GreaterEqual,
    In,
    Is,
    IsInstance,
    IsNone,
    IsNot,
    IsNotNone,
    Less,
    LessEqual,
    ListEqual,
    // Logs,
    MultiLineEqual,
    // NoLogs,
    NotAlmostEqual,
    NotAlmostEquals,
    NotEqual,
    NotEquals,
    NotIn,
    NotIsInstance,
    NotRegex,
    NotRegexpMatches,
    // Raises,
    // RaisesRegex,
    // RaisesRegexp,
    Regex,
    RegexpMatches,
    SequenceEqual,
    SetEqual,
    True,
    TupleEqual,
    Underscore,
    // Warns,
    // WarnsRegex,
}

impl std::fmt::Display for UnittestAssert {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            UnittestAssert::AlmostEqual => write!(f, "assertAlmostEqual"),
            UnittestAssert::AlmostEquals => write!(f, "assertAlmostEquals"),
            UnittestAssert::CountEqual => write!(f, "assertCountEqual"),
            UnittestAssert::DictEqual => write!(f, "assertDictEqual"),
            UnittestAssert::DictContainsSubset => write!(f, "assertDictContainsSubset"),
            UnittestAssert::Equal => write!(f, "assertEqual"),
            UnittestAssert::Equals => write!(f, "assertEquals"),
            UnittestAssert::False => write!(f, "assertFalse"),
            UnittestAssert::Greater => write!(f, "assertGreater"),
            UnittestAssert::GreaterEqual => write!(f, "assertGreaterEqual"),
            UnittestAssert::In => write!(f, "assertIn"),
            UnittestAssert::Is => write!(f, "assertIs"),
            UnittestAssert::IsInstance => write!(f, "assertIsInstance"),
            UnittestAssert::IsNone => write!(f, "assertIsNone"),
            UnittestAssert::IsNot => write!(f, "assertIsNot"),
            UnittestAssert::IsNotNone => write!(f, "assertIsNotNone"),
            UnittestAssert::Less => write!(f, "assertLess"),
            UnittestAssert::LessEqual => write!(f, "assertLessEqual"),
            UnittestAssert::ListEqual => write!(f, "assertListEqual"),
            UnittestAssert::MultiLineEqual => write!(f, "assertMultiLineEqual"),
            UnittestAssert::NotAlmostEqual => write!(f, "assertNotAlmostEqual"),
            UnittestAssert::NotAlmostEquals => write!(f, "assertNotAlmostEquals"),
            UnittestAssert::NotEqual => write!(f, "assertNotEqual"),
            UnittestAssert::NotEquals => write!(f, "assertNotEquals"),
            UnittestAssert::NotIn => write!(f, "assertNotIn"),
            UnittestAssert::NotIsInstance => write!(f, "assertNotIsInstance"),
            UnittestAssert::NotRegex => write!(f, "assertNotRegex"),
            UnittestAssert::NotRegexpMatches => write!(f, "assertNotRegexpMatches"),
            UnittestAssert::Regex => write!(f, "assertRegex"),
            UnittestAssert::RegexpMatches => write!(f, "assertRegexpMatches"),
            UnittestAssert::SequenceEqual => write!(f, "assertSequenceEqual"),
            UnittestAssert::SetEqual => write!(f, "assertSetEqual"),
            UnittestAssert::True => write!(f, "assertTrue"),
            UnittestAssert::TupleEqual => write!(f, "assertTupleEqual"),
            UnittestAssert::Underscore => write!(f, "assert_"),
        }
    }
}

impl TryFrom<&str> for UnittestAssert {
    type Error = String;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "assertAlmostEqual" => Ok(UnittestAssert::AlmostEqual),
            "assertAlmostEquals" => Ok(UnittestAssert::AlmostEquals),
            "assertCountEqual" => Ok(UnittestAssert::CountEqual),
            "assertDictContainsSubset" => Ok(UnittestAssert::DictContainsSubset),
            "assertDictEqual" => Ok(UnittestAssert::DictEqual),
            "assertEqual" => Ok(UnittestAssert::Equal),
            "assertEquals" => Ok(UnittestAssert::Equals),
            "assertFalse" => Ok(UnittestAssert::False),
            "assertGreater" => Ok(UnittestAssert::Greater),
            "assertGreaterEqual" => Ok(UnittestAssert::GreaterEqual),
            "assertIn" => Ok(UnittestAssert::In),
            "assertIs" => Ok(UnittestAssert::Is),
            "assertIsInstance" => Ok(UnittestAssert::IsInstance),
            "assertIsNone" => Ok(UnittestAssert::IsNone),
            "assertIsNot" => Ok(UnittestAssert::IsNot),
            "assertIsNotNone" => Ok(UnittestAssert::IsNotNone),
            "assertLess" => Ok(UnittestAssert::Less),
            "assertLessEqual" => Ok(UnittestAssert::LessEqual),
            "assertListEqual" => Ok(UnittestAssert::ListEqual),
            "assertMultiLineEqual" => Ok(UnittestAssert::MultiLineEqual),
            "assertNotAlmostEqual" => Ok(UnittestAssert::NotAlmostEqual),
            "assertNotAlmostEquals" => Ok(UnittestAssert::NotAlmostEquals),
            "assertNotEqual" => Ok(UnittestAssert::NotEqual),
            "assertNotEquals" => Ok(UnittestAssert::NotEquals),
            "assertNotIn" => Ok(UnittestAssert::NotIn),
            "assertNotIsInstance" => Ok(UnittestAssert::NotIsInstance),
            "assertNotRegex" => Ok(UnittestAssert::NotRegex),
            "assertNotRegexpMatches" => Ok(UnittestAssert::NotRegexpMatches),
            "assertRegex" => Ok(UnittestAssert::Regex),
            "assertRegexpMatches" => Ok(UnittestAssert::RegexpMatches),
            "assertSequenceEqual" => Ok(UnittestAssert::SequenceEqual),
            "assertSetEqual" => Ok(UnittestAssert::SetEqual),
            "assertTrue" => Ok(UnittestAssert::True),
            "assertTupleEqual" => Ok(UnittestAssert::TupleEqual),
            "assert_" => Ok(UnittestAssert::Underscore),
            _ => Err(format!("Unknown unittest assert method: {value}")),
        }
    }
}

fn assert(expr: &Expr, msg: Option<&Expr>) -> Stmt {
    create_stmt(StmtKind::Assert {
        test: Box::new(expr.clone()),
        msg: msg.map(|msg| Box::new(msg.clone())),
    })
}

fn compare(left: &Expr, cmpop: Cmpop, right: &Expr) -> Expr {
    create_expr(ExprKind::Compare {
        left: Box::new(left.clone()),
        ops: vec![cmpop],
        comparators: vec![right.clone()],
    })
}

impl UnittestAssert {
    fn arg_spec(&self) -> &[&str] {
        match self {
            UnittestAssert::AlmostEqual => &["first", "second", "places", "msg", "delta"],
            UnittestAssert::AlmostEquals => &["first", "second", "places", "msg", "delta"],
            UnittestAssert::CountEqual => &["first", "second", "msg"],
            UnittestAssert::DictContainsSubset => &["subset", "dictionary", "msg"],
            UnittestAssert::DictEqual => &["first", "second", "msg"],
            UnittestAssert::Equal => &["first", "second", "msg"],
            UnittestAssert::Equals => &["first", "second", "msg"],
            UnittestAssert::False => &["expr", "msg"],
            UnittestAssert::Greater => &["first", "second", "msg"],
            UnittestAssert::GreaterEqual => &["first", "second", "msg"],
            UnittestAssert::In => &["member", "container", "msg"],
            UnittestAssert::Is => &["first", "second", "msg"],
            UnittestAssert::IsInstance => &["obj", "cls", "msg"],
            UnittestAssert::IsNone => &["expr", "msg"],
            UnittestAssert::IsNot => &["first", "second", "msg"],
            UnittestAssert::IsNotNone => &["expr", "msg"],
            UnittestAssert::Less => &["first", "second", "msg"],
            UnittestAssert::LessEqual => &["first", "second", "msg"],
            UnittestAssert::ListEqual => &["first", "second", "msg"],
            UnittestAssert::MultiLineEqual => &["first", "second", "msg"],
            UnittestAssert::NotAlmostEqual => &["first", "second", "msg"],
            UnittestAssert::NotAlmostEquals => &["first", "second", "msg"],
            UnittestAssert::NotEqual => &["first", "second", "msg"],
            UnittestAssert::NotEquals => &["first", "second", "msg"],
            UnittestAssert::NotIn => &["member", "container", "msg"],
            UnittestAssert::NotIsInstance => &["obj", "cls", "msg"],
            UnittestAssert::NotRegex => &["text", "regex", "msg"],
            UnittestAssert::NotRegexpMatches => &["text", "regex", "msg"],
            UnittestAssert::Regex => &["text", "regex", "msg"],
            UnittestAssert::RegexpMatches => &["text", "regex", "msg"],
            UnittestAssert::SequenceEqual => &["first", "second", "msg", "seq_type"],
            UnittestAssert::SetEqual => &["first", "second", "msg"],
            UnittestAssert::True => &["expr", "msg"],
            UnittestAssert::TupleEqual => &["first", "second", "msg"],
            UnittestAssert::Underscore => &["expr", "msg"],
        }
    }

    /// Create a map from argument name to value.
    pub fn args_map<'a>(
        &'a self,
        args: &'a [Expr],
        keywords: &'a [Keyword],
    ) -> Result<FxHashMap<&'a str, &'a Expr>> {
        // If we have variable-length arguments, abort.
        if args
            .iter()
            .any(|arg| matches!(arg.node, ExprKind::Starred { .. }))
            || keywords.iter().any(|kw| kw.node.arg.is_none())
        {
            bail!("Variable-length arguments are not supported");
        }

        let arg_spec = self.arg_spec();

        // If any of the keyword arguments are not in the argument spec, abort.
        if keywords.iter().any(|kw| {
            kw.node
                .arg
                .as_ref()
                .map_or(false, |kwarg_name| !arg_spec.contains(&kwarg_name.as_str()))
        }) {
            bail!("Unknown keyword argument");
        }

        // Generate a map from argument name to value.
        let mut args_map: FxHashMap<&str, &Expr> = FxHashMap::with_capacity_and_hasher(
            args.len() + keywords.len(),
            BuildHasherDefault::default(),
        );

        // Process positional arguments.
        for (arg_name, value) in arg_spec.iter().zip(args.iter()) {
            args_map.insert(arg_name, value);
        }

        // Process keyword arguments.
        for arg_name in arg_spec.iter().skip(args.len()) {
            if let Some(value) = keywords.iter().find_map(|keyword| {
                if keyword
                    .node
                    .arg
                    .as_ref()
                    .map_or(false, |kwarg_name| kwarg_name == arg_name)
                {
                    Some(&keyword.node.value)
                } else {
                    None
                }
            }) {
                args_map.insert(arg_name, value);
            }
        }

        Ok(args_map)
    }

    pub fn generate_assert(self, args: &[Expr], keywords: &[Keyword]) -> Result<Stmt> {
        let args = self.args_map(args, keywords)?;
        match self {
            UnittestAssert::True | UnittestAssert::False => {
                let expr = args
                    .get("expr")
                    .ok_or_else(|| anyhow!("Missing argument `expr`"))?;
                let msg = args.get("msg").copied();
                Ok(if matches!(self, UnittestAssert::False) {
                    let unary_expr = create_expr(ExprKind::UnaryOp {
                        op: Unaryop::Not,
                        operand: Box::new(create_expr(expr.node.clone())),
                    });
                    assert(&unary_expr, msg)
                } else {
                    assert(expr, msg)
                })
            }
            UnittestAssert::Equal
            | UnittestAssert::Equals
            | UnittestAssert::NotEqual
            | UnittestAssert::NotEquals
            | UnittestAssert::Greater
            | UnittestAssert::GreaterEqual
            | UnittestAssert::Less
            | UnittestAssert::LessEqual
            | UnittestAssert::Is
            | UnittestAssert::IsNot => {
                let first = args
                    .get("first")
                    .ok_or_else(|| anyhow!("Missing argument `first`"))?;
                let second = args
                    .get("second")
                    .ok_or_else(|| anyhow!("Missing argument `second`"))?;
                let msg = args.get("msg").copied();
                let cmpop = match self {
                    UnittestAssert::Equal | UnittestAssert::Equals => Cmpop::Eq,
                    UnittestAssert::NotEqual | UnittestAssert::NotEquals => Cmpop::NotEq,
                    UnittestAssert::Greater => Cmpop::Gt,
                    UnittestAssert::GreaterEqual => Cmpop::GtE,
                    UnittestAssert::Less => Cmpop::Lt,
                    UnittestAssert::LessEqual => Cmpop::LtE,
                    UnittestAssert::Is => Cmpop::Is,
                    UnittestAssert::IsNot => Cmpop::IsNot,
                    _ => unreachable!(),
                };
                let expr = compare(first, cmpop, second);
                Ok(assert(&expr, msg))
            }
            UnittestAssert::In | UnittestAssert::NotIn => {
                let member = args
                    .get("member")
                    .ok_or_else(|| anyhow!("Missing argument `member`"))?;
                let container = args
                    .get("container")
                    .ok_or_else(|| anyhow!("Missing argument `container`"))?;
                let msg = args.get("msg").copied();
                let cmpop = if matches!(self, UnittestAssert::In) {
                    Cmpop::In
                } else {
                    Cmpop::NotIn
                };
                let expr = compare(member, cmpop, container);
                Ok(assert(&expr, msg))
            }
            UnittestAssert::IsNone | UnittestAssert::IsNotNone => {
                let expr = args
                    .get("expr")
                    .ok_or_else(|| anyhow!("Missing argument `expr`"))?;
                let msg = args.get("msg").copied();
                let cmpop = if matches!(self, UnittestAssert::IsNone) {
                    Cmpop::Is
                } else {
                    Cmpop::IsNot
                };
                let expr = compare(
                    expr,
                    cmpop,
                    &create_expr(ExprKind::Constant {
                        value: Constant::None,
                        kind: None,
                    }),
                );
                Ok(assert(&expr, msg))
            }
            UnittestAssert::IsInstance | UnittestAssert::NotIsInstance => {
                let obj = args
                    .get("obj")
                    .ok_or_else(|| anyhow!("Missing argument `obj`"))?;
                let cls = args
                    .get("cls")
                    .ok_or_else(|| anyhow!("Missing argument `cls`"))?;
                let msg = args.get("msg").copied();
                let isinstance = create_expr(ExprKind::Call {
                    func: Box::new(create_expr(ExprKind::Name {
                        id: "isinstance".to_string(),
                        ctx: ExprContext::Load,
                    })),
                    args: vec![(**obj).clone(), (**cls).clone()],
                    keywords: vec![],
                });
                if matches!(self, UnittestAssert::IsInstance) {
                    Ok(assert(&isinstance, msg))
                } else {
                    let expr = create_expr(ExprKind::UnaryOp {
                        op: Unaryop::Not,
                        operand: Box::new(isinstance),
                    });
                    Ok(assert(&expr, msg))
                }
            }
            UnittestAssert::Regex
            | UnittestAssert::RegexpMatches
            | UnittestAssert::NotRegex
            | UnittestAssert::NotRegexpMatches => {
                let text = args
                    .get("text")
                    .ok_or_else(|| anyhow!("Missing argument `text`"))?;
                let regex = args
                    .get("regex")
                    .ok_or_else(|| anyhow!("Missing argument `regex`"))?;
                let msg = args.get("msg").copied();
                let re_search = create_expr(ExprKind::Call {
                    func: Box::new(create_expr(ExprKind::Attribute {
                        value: Box::new(create_expr(ExprKind::Name {
                            id: "re".to_string(),
                            ctx: ExprContext::Load,
                        })),
                        attr: "search".to_string(),
                        ctx: ExprContext::Load,
                    })),
                    args: vec![(**regex).clone(), (**text).clone()],
                    keywords: vec![],
                });
                if matches!(self, UnittestAssert::Regex | UnittestAssert::RegexpMatches) {
                    Ok(assert(&re_search, msg))
                } else {
                    Ok(assert(
                        &create_expr(ExprKind::UnaryOp {
                            op: Unaryop::Not,
                            operand: Box::new(re_search),
                        }),
                        msg,
                    ))
                }
            }
            _ => bail!("Cannot autofix `{self}`"),
        }
    }
}
