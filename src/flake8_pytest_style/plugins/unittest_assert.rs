use rustc_hash::FxHashMap;

use rustpython_ast::ExprContext::Load;
use rustpython_ast::{Cmpop, Constant, Expr, ExprKind, Keyword, Location, Stmt, StmtKind, Unaryop};

pub enum UnittestAssert {
    AssertAlmostEqual,
    AssertAlmostEquals,
    AssertDictEqual,
    AssertEqual,
    AssertEquals,
    AssertFalse,
    AssertGreater,
    AssertGreaterEqual,
    AssertIn,
    AssertIs,
    AssertIsInstance,
    AssertIsNone,
    AssertIsNot,
    AssertIsNotNone,
    AssertItemsEqual,
    AssertLess,
    AssertLessEqual,
    AssertMultiLineEqual,
    AssertNotAlmostEqual,
    AssertNotAlmostEquals,
    AssertNotContains,
    AssertNotEqual,
    AssertNotEquals,
    AssertNotIn,
    AssertNotIsInstance,
    AssertNotRegex,
    AssertNotRegexpMatches,
    AssertRaises,
    AssertRaisesMessage,
    AssertRaisesRegexp,
    AssertRegex,
    AssertRegexpMatches,
    AssertSetEqual,
    AssertTrue,
    AssertUnderscore,
}

impl std::fmt::Display for UnittestAssert {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use UnittestAssert::*;
        match self {
            AssertAlmostEqual => write!(f, "assertAlmostEqual"),
            AssertAlmostEquals => write!(f, "assertAlmostEquals"),
            AssertDictEqual => write!(f, "assertDictEqual"),
            AssertEqual => write!(f, "assertEqual"),
            AssertEquals => write!(f, "assertEquals"),
            AssertFalse => write!(f, "assertFalse"),
            AssertGreater => write!(f, "assertGreater"),
            AssertGreaterEqual => write!(f, "assertGreaterEqual"),
            AssertIn => write!(f, "assertIn"),
            AssertIs => write!(f, "assertIs"),
            AssertIsInstance => write!(f, "assertIsInstance"),
            AssertIsNone => write!(f, "assertIsNone"),
            AssertIsNot => write!(f, "assertIsNot"),
            AssertIsNotNone => write!(f, "assertIsNotNone"),
            AssertItemsEqual => write!(f, "assertItemsEqual"),
            AssertLess => write!(f, "assertLess"),
            AssertLessEqual => write!(f, "assertLessEqual"),
            AssertMultiLineEqual => write!(f, "assertMultiLineEqual"),
            AssertNotAlmostEqual => write!(f, "assertNotAlmostEqual"),
            AssertNotAlmostEquals => write!(f, "assertNotAlmostEquals"),
            AssertNotContains => write!(f, "assertNotContains"),
            AssertNotEqual => write!(f, "assertNotEqual"),
            AssertNotEquals => write!(f, "assertNotEquals"),
            AssertNotIn => write!(f, "assertNotIn"),
            AssertNotIsInstance => write!(f, "assertNotIsInstance"),
            AssertNotRegex => write!(f, "assertNotRegex"),
            AssertNotRegexpMatches => write!(f, "assertNotRegexpMatches"),
            AssertRaises => write!(f, "assertRaises"),
            AssertRaisesMessage => write!(f, "assertRaisesMessage"),
            AssertRaisesRegexp => write!(f, "assertRaisesRegexp"),
            AssertRegex => write!(f, "assertRegex"),
            AssertRegexpMatches => write!(f, "assertRegexpMatches"),
            AssertSetEqual => write!(f, "assertSetEqual"),
            AssertTrue => write!(f, "assertTrue"),
            AssertUnderscore => write!(f, "assert_"),
        }
    }
}

impl TryFrom<&str> for UnittestAssert {
    type Error = String;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        use UnittestAssert::*;
        match value {
            "assertAlmostEqual" => Ok(AssertAlmostEqual),
            "assertAlmostEquals" => Ok(AssertAlmostEquals),
            "assertDictEqual" => Ok(AssertDictEqual),
            "assertEqual" => Ok(AssertEqual),
            "assertEquals" => Ok(AssertEquals),
            "assertFalse" => Ok(AssertFalse),
            "assertGreater" => Ok(AssertGreater),
            "assertGreaterEqual" => Ok(AssertGreaterEqual),
            "assertIn" => Ok(AssertIn),
            "assertIs" => Ok(AssertIs),
            "assertIsInstance" => Ok(AssertIsInstance),
            "assertIsNone" => Ok(AssertIsNone),
            "assertIsNot" => Ok(AssertIsNot),
            "assertIsNotNone" => Ok(AssertIsNotNone),
            "assertItemsEqual" => Ok(AssertItemsEqual),
            "assertLess" => Ok(AssertLess),
            "assertLessEqual" => Ok(AssertLessEqual),
            "assertMultiLineEqual" => Ok(AssertMultiLineEqual),
            "assertNotAlmostEqual" => Ok(AssertNotAlmostEqual),
            "assertNotAlmostEquals" => Ok(AssertNotAlmostEquals),
            "assertNotContains" => Ok(AssertNotContains),
            "assertNotEqual" => Ok(AssertNotEqual),
            "assertNotEquals" => Ok(AssertNotEquals),
            "assertNotIn" => Ok(AssertNotIn),
            "assertNotIsInstance" => Ok(AssertNotIsInstance),
            "assertNotRegex" => Ok(AssertNotRegex),
            "assertNotRegexpMatches" => Ok(AssertNotRegexpMatches),
            "assertRaises" => Ok(AssertRaises),
            "assertRaisesMessage" => Ok(AssertRaisesMessage),
            "assertRaisesRegexp" => Ok(AssertRaisesRegexp),
            "assertRegex" => Ok(AssertRegex),
            "assertRegexpMatches" => Ok(AssertRegexpMatches),
            "assertSetEqual" => Ok(AssertSetEqual),
            "assertTrue" => Ok(AssertTrue),
            "assert_" => Ok(AssertUnderscore),
            _ => Err(format!("Unknown unittest assert name: {}", value)),
        }
    }
}

fn assert(expr: &Expr, msg: Option<&Expr>) -> Stmt {
    Stmt::new(
        Location::default(),
        Location::default(),
        StmtKind::Assert {
            test: Box::new(expr.clone()),
            msg: msg.map(|msg| Box::new(msg.clone())),
        },
    )
}

fn compare(left: &Expr, cmpop: Cmpop, right: &Expr) -> Expr {
    Expr::new(
        Location::default(),
        Location::default(),
        ExprKind::Compare {
            left: Box::new(left.clone()),
            ops: vec![cmpop],
            comparators: vec![right.clone()],
        },
    )
}

/// Represents the signature of an assert method.
/// For example, `assertTrue` signature is expressed as
/// `Signature::new( args: vec!["expr"], keywords: vec!["msg"])`.
pub struct Signature<'a> {
    pos_args: Vec<&'a str>,
    kw_args: Vec<&'a str>,
}

impl<'a> Signature<'a> {
    pub fn new(pos_args: Vec<&'a str>, kw_args: Vec<&'a str>) -> Self {
        Self { pos_args, kw_args }
    }

    pub fn is_valid_arg(&self, arg: &str) -> bool {
        self.pos_args.contains(&arg) || self.kw_args.contains(&arg)
    }
}

impl UnittestAssert {
    pub fn signature(&self) -> Signature {
        use UnittestAssert::*;

        match self {
            AssertAlmostEqual => {
                Signature::new(vec!["first", "second"], vec!["places", "msg", "delta"])
            }
            AssertAlmostEquals => {
                Signature::new(vec!["first", "second"], vec!["places", "msg", "delta"])
            }
            AssertDictEqual => Signature::new(vec!["d1", "d2"], vec!["msg"]),
            AssertEqual => Signature::new(vec!["first", "second"], vec!["msg"]),
            AssertEquals => Signature::new(vec!["first", "second"], vec!["msg"]),
            AssertFalse => Signature::new(vec!["expr"], vec!["msg"]),
            AssertGreater => Signature::new(vec!["first", "second"], vec!["msg"]),
            AssertGreaterEqual => Signature::new(vec!["first", "second"], vec!["msg"]),
            AssertIn => Signature::new(vec!["member", "container"], vec!["msg"]),
            AssertIs => Signature::new(vec!["expr1", "expr2"], vec!["msg"]),
            AssertIsInstance => Signature::new(vec!["obj", "cls"], vec!["msg"]),
            AssertIsNone => Signature::new(vec!["expr"], vec!["msg"]),
            AssertIsNot => Signature::new(vec!["expr1", "expr2"], vec!["msg"]),
            AssertIsNotNone => Signature::new(vec!["expr"], vec!["msg"]),
            AssertItemsEqual => Signature::new(vec!["first", "second"], vec!["msg"]),
            AssertLess => Signature::new(vec!["first", "second"], vec!["msg"]),
            AssertLessEqual => Signature::new(vec!["first", "second"], vec!["msg"]),
            AssertMultiLineEqual => Signature::new(vec!["first", "second"], vec!["msg"]),
            AssertNotAlmostEqual => Signature::new(vec!["first", "second"], vec!["msg"]),
            AssertNotAlmostEquals => Signature::new(vec!["first", "second"], vec!["msg"]),
            AssertNotContains => Signature::new(vec!["container", "member"], vec!["msg"]),
            AssertNotEqual => Signature::new(vec!["first", "second"], vec!["msg"]),
            AssertNotEquals => Signature::new(vec!["first", "second"], vec!["msg"]),
            AssertNotIn => Signature::new(vec!["member", "container"], vec!["msg"]),
            AssertNotIsInstance => Signature::new(vec!["obj", "cls"], vec!["msg"]),
            AssertNotRegex => Signature::new(vec!["text", "regex"], vec!["msg"]),
            AssertNotRegexpMatches => Signature::new(vec!["text", "regex"], vec!["msg"]),
            AssertRaises => Signature::new(vec!["exception"], vec!["msg"]),
            AssertRaisesMessage => Signature::new(vec!["exception", "msg"], vec!["msg"]),
            AssertRaisesRegexp => Signature::new(vec!["exception", "regex"], vec!["msg"]),
            AssertRegex => Signature::new(vec!["text", "regex"], vec!["msg"]),
            AssertRegexpMatches => Signature::new(vec!["text", "regex"], vec!["msg"]),
            AssertSetEqual => Signature::new(vec!["set1", "set2"], vec!["msg"]),
            AssertTrue => Signature::new(vec!["expr"], vec!["msg"]),
            AssertUnderscore => Signature::new(vec!["expr"], vec!["msg"]),
        }
    }

    pub fn extract_args<'a>(
        &'a self,
        args: &'a [Expr],
        keywords: &'a [Keyword],
    ) -> Result<FxHashMap<&'a str, &'a Expr>, String> {
        if args
            .iter()
            .any(|arg| matches!(arg.node, ExprKind::Starred { .. }))
            || keywords.iter().any(|kw| kw.node.arg.is_none())
        {
            return Err("Contains variable-length arguments. Cannot autofix.".to_string());
        }

        let sig = self.signature();
        let mut arg_map: FxHashMap<&str, &Expr> = FxHashMap::default();
        for (arg, value) in sig.pos_args.iter().zip(args.iter()) {
            arg_map.insert(arg, value);
        }
        for kw in keywords {
            let arg = kw.node.arg.as_ref().unwrap();
            if !sig.is_valid_arg(&(*arg).as_str()) {
                return Err(format!("Unexpected keyword argument `{}`", arg));
            }
            arg_map.insert(kw.node.arg.as_ref().unwrap().as_str(), &kw.node.value);
        }
        Ok(arg_map)
    }

    pub fn generate_assert(&self, args: &[Expr], keywords: &[Keyword]) -> Result<Stmt, String> {
        use UnittestAssert::*;
        match self {
            AssertTrue | AssertFalse => {
                let args = self.extract_args(args, keywords)?;
                let expr = args.get("expr").ok_or("Missing argument `expr`")?;
                let msg = args.get("msg").map(|msg| *msg);
                let bool = Expr::new(
                    Location::default(),
                    Location::default(),
                    ExprKind::Constant {
                        value: Constant::Bool(matches!(self, UnittestAssert::AssertTrue)),
                        kind: None,
                    },
                );
                let expr = compare(expr, Cmpop::Is, &bool);
                Ok(assert(&expr, msg))
            }
            AssertEqual | AssertEquals | AssertNotEqual | AssertNotEquals | AssertGreater
            | AssertGreaterEqual | AssertLess | AssertLessEqual => {
                let args = self.extract_args(args, keywords)?;
                let first = args.get("first").ok_or("Missing argument `first`")?;
                let second = args.get("second").ok_or("Missing argument `second`")?;
                let msg = args.get("msg").map(|msg| *msg);
                let cmpop = match self {
                    AssertEqual | AssertEquals => Cmpop::Eq,
                    AssertNotEqual | AssertNotEquals => Cmpop::NotEq,
                    AssertGreater => Cmpop::Gt,
                    AssertGreaterEqual => Cmpop::GtE,
                    AssertLess => Cmpop::Lt,
                    AssertLessEqual => Cmpop::LtE,
                    _ => unreachable!(),
                };
                let expr = compare(first, cmpop, second);
                Ok(assert(&expr, msg))
            }
            AssertIs | AssertIsNot => {
                let args = self.extract_args(args, keywords)?;
                let expr1 = args.get("expr1").ok_or("Missing argument `expr1`")?;
                let expr2 = args.get("expr2").ok_or("Missing argument `expr2`")?;
                let msg = args.get("msg").map(|msg| *msg);
                let cmpop = if matches!(self, AssertIs) {
                    Cmpop::Is
                } else {
                    Cmpop::IsNot
                };
                let expr = compare(expr1, cmpop, expr2);
                Ok(assert(&expr, msg))
            }
            AssertIn | AssertNotIn => {
                let args = self.extract_args(args, keywords)?;
                let member = args.get("member").ok_or("Missing argument `member`")?;
                let container = args
                    .get("container")
                    .ok_or("Missing argument `container`")?;
                let msg = args.get("msg").map(|msg| *msg);
                let cmpop = if matches!(self, AssertIn) {
                    Cmpop::In
                } else {
                    Cmpop::NotIn
                };
                let expr = compare(member, cmpop, container);
                Ok(assert(&expr, msg))
            }
            AssertIsNone | AssertIsNotNone => {
                let args = self.extract_args(args, keywords)?;
                let expr = args.get("expr").ok_or("Missing argument `expr`")?;
                let msg = args.get("msg").map(|msg| *msg);
                let cmpop = if matches!(self, AssertIsNone) {
                    Cmpop::Is
                } else {
                    Cmpop::IsNot
                };
                let expr = compare(
                    expr,
                    cmpop,
                    &Expr::new(
                        Location::default(),
                        Location::default(),
                        ExprKind::Constant {
                            value: Constant::None,
                            kind: None,
                        },
                    ),
                );
                Ok(assert(&expr, msg))
            }
            AssertIsInstance | AssertNotIsInstance => {
                let args = self.extract_args(args, keywords)?;
                let obj = args.get("obj").ok_or("Missing argument `obj`")?;
                let cls = args.get("cls").ok_or("Missing argument `cls`")?;
                let msg = args.get("msg").map(|msg| *msg);
                let isinstance = Expr::new(
                    Location::default(),
                    Location::default(),
                    ExprKind::Call {
                        func: Box::new(Expr::new(
                            Location::default(),
                            Location::default(),
                            ExprKind::Name {
                                id: "isinstance".to_string(),
                                ctx: Load,
                            },
                        )),
                        args: vec![(**obj).clone(), (**cls).clone()],
                        keywords: vec![],
                    },
                );
                if matches!(self, AssertIsInstance) {
                    Ok(assert(&isinstance, msg))
                } else {
                    let expr = Expr::new(
                        Location::default(),
                        Location::default(),
                        ExprKind::UnaryOp {
                            op: Unaryop::Not,
                            operand: Box::new(isinstance),
                        },
                    );
                    Ok(assert(&expr, msg))
                }
            }
            AssertRegex | AssertRegexpMatches | AssertNotRegex | AssertNotRegexpMatches => {
                let args = self.extract_args(args, keywords)?;
                let regex = args.get("regex").ok_or("Missing argument `regex`")?;
                let text = args.get("text").ok_or("Missing argument `text`")?;
                let msg = args.get("msg").map(|msg| *msg);
                let re_search = Expr::new(
                    Location::default(),
                    Location::default(),
                    ExprKind::Call {
                        func: Box::new(Expr::new(
                            Location::default(),
                            Location::default(),
                            ExprKind::Attribute {
                                value: Box::new(Expr::new(
                                    Location::default(),
                                    Location::default(),
                                    ExprKind::Name {
                                        id: "re".to_string(),
                                        ctx: Load,
                                    },
                                )),
                                attr: "search".to_string(),
                                ctx: Load,
                            },
                        )),
                        args: vec![(**regex).clone(), (**text).clone()],
                        keywords: vec![],
                    },
                );
                if matches!(self, AssertRegex | AssertRegexpMatches) {
                    Ok(assert(&re_search, msg))
                } else {
                    let expr = Expr::new(
                        Location::default(),
                        Location::default(),
                        ExprKind::UnaryOp {
                            op: Unaryop::Not,
                            operand: Box::new(re_search),
                        },
                    );
                    Ok(assert(&expr, msg))
                }
            }
            _ => Err(format!("Cannot autofix `{self}`")),
        }
    }
}
