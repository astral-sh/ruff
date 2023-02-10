use std::hash::BuildHasherDefault;

use anyhow::{anyhow, bail, Result};
use rustc_hash::FxHashMap;
use rustpython_parser::ast::{
    Cmpop, Constant, Expr, ExprContext, ExprKind, Keyword, Stmt, StmtKind, Unaryop,
};

use crate::ast::helpers::{create_expr, create_stmt};

/// An enum to represent the different types of assertions present in the
/// `unittest` module. Note: any variants that can't be replaced with plain
/// `assert` statements are commented out.
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

pub struct Arguments<'a> {
    positional: Vec<&'a str>,
    keyword: Vec<&'a str>,
}

impl<'a> Arguments<'a> {
    pub fn new(positional: Vec<&'a str>, keyword: Vec<&'a str>) -> Self {
        Self {
            positional,
            keyword,
        }
    }

    pub fn contains(&self, arg: &str) -> bool {
        self.positional.contains(&arg) || self.keyword.contains(&arg)
    }
}

impl UnittestAssert {
    pub fn arguments(&self) -> Arguments {
        match self {
            UnittestAssert::AlmostEqual => {
                Arguments::new(vec!["first", "second"], vec!["places", "msg", "delta"])
            }
            UnittestAssert::AlmostEquals => {
                Arguments::new(vec!["first", "second"], vec!["places", "msg", "delta"])
            }
            UnittestAssert::CountEqual => Arguments::new(vec!["first", "second"], vec!["msg"]),
            UnittestAssert::DictContainsSubset => {
                Arguments::new(vec!["subset", "dictionary"], vec!["msg"])
            }
            UnittestAssert::DictEqual => Arguments::new(vec!["first", "second"], vec!["msg"]),
            UnittestAssert::Equal => Arguments::new(vec!["first", "second"], vec!["msg"]),
            UnittestAssert::Equals => Arguments::new(vec!["first", "second"], vec!["msg"]),
            UnittestAssert::False => Arguments::new(vec!["expr"], vec!["msg"]),
            UnittestAssert::Greater => Arguments::new(vec!["first", "second"], vec!["msg"]),
            UnittestAssert::GreaterEqual => Arguments::new(vec!["first", "second"], vec!["msg"]),
            UnittestAssert::In => Arguments::new(vec!["member", "container"], vec!["msg"]),
            UnittestAssert::Is => Arguments::new(vec!["first", "second"], vec!["msg"]),
            UnittestAssert::IsInstance => Arguments::new(vec!["obj", "cls"], vec!["msg"]),
            UnittestAssert::IsNone => Arguments::new(vec!["expr"], vec!["msg"]),
            UnittestAssert::IsNot => Arguments::new(vec!["first", "second"], vec!["msg"]),
            UnittestAssert::IsNotNone => Arguments::new(vec!["expr"], vec!["msg"]),
            UnittestAssert::Less => Arguments::new(vec!["first", "second"], vec!["msg"]),
            UnittestAssert::LessEqual => Arguments::new(vec!["first", "second"], vec!["msg"]),
            UnittestAssert::ListEqual => Arguments::new(vec!["first", "second"], vec!["msg"]),
            UnittestAssert::MultiLineEqual => Arguments::new(vec!["first", "second"], vec!["msg"]),
            UnittestAssert::NotAlmostEqual => Arguments::new(vec!["first", "second"], vec!["msg"]),
            UnittestAssert::NotAlmostEquals => Arguments::new(vec!["first", "second"], vec!["msg"]),
            UnittestAssert::NotEqual => Arguments::new(vec!["first", "second"], vec!["msg"]),
            UnittestAssert::NotEquals => Arguments::new(vec!["first", "second"], vec!["msg"]),
            UnittestAssert::NotIn => Arguments::new(vec!["member", "container"], vec!["msg"]),
            UnittestAssert::NotIsInstance => Arguments::new(vec!["obj", "cls"], vec!["msg"]),
            UnittestAssert::NotRegex => Arguments::new(vec!["text", "regex"], vec!["msg"]),
            UnittestAssert::NotRegexpMatches => Arguments::new(vec!["text", "regex"], vec!["msg"]),
            UnittestAssert::Regex => Arguments::new(vec!["text", "regex"], vec!["msg"]),
            UnittestAssert::RegexpMatches => Arguments::new(vec!["text", "regex"], vec!["msg"]),
            UnittestAssert::SequenceEqual => {
                Arguments::new(vec!["first", "second"], vec!["msg", "seq_type"])
            }
            UnittestAssert::SetEqual => Arguments::new(vec!["first", "second"], vec!["msg"]),
            UnittestAssert::True => Arguments::new(vec!["expr"], vec!["msg"]),
            UnittestAssert::TupleEqual => Arguments::new(vec!["first", "second"], vec!["msg"]),
            UnittestAssert::Underscore => Arguments::new(vec!["expr"], vec!["msg"]),
        }
    }

    /// Create a map from argument name to value.
    pub fn args_map<'a>(
        &'a self,
        args: &'a [Expr],
        keywords: &'a [Keyword],
    ) -> Result<FxHashMap<&'a str, &'a Expr>> {
        if args
            .iter()
            .any(|arg| matches!(arg.node, ExprKind::Starred { .. }))
            || keywords.iter().any(|kw| kw.node.arg.is_none())
        {
            bail!("Contains variable-length arguments. Cannot autofix.".to_string());
        }

        let mut args_map: FxHashMap<&str, &Expr> = FxHashMap::with_capacity_and_hasher(
            args.len() + keywords.len(),
            BuildHasherDefault::default(),
        );
        let arguments = self.arguments();
        for (arg, value) in arguments.positional.iter().zip(args.iter()) {
            args_map.insert(arg, value);
        }
        for kw in keywords {
            let arg = kw.node.arg.as_ref().unwrap();
            if !arguments.contains((*arg).as_str()) {
                bail!("Unexpected keyword argument `{arg}`");
            }
            args_map.insert(kw.node.arg.as_ref().unwrap().as_str(), &kw.node.value);
        }
        Ok(args_map)
    }

    pub fn generate_assert(&self, args: &[Expr], keywords: &[Keyword]) -> Result<Stmt> {
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
