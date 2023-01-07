use rustc_hash::FxHashMap;

use rustpython_ast::ExprContext::Load;
use rustpython_ast::{Cmpop, Constant, Expr, ExprKind, Keyword, Location, Stmt, StmtKind, Unaryop};

pub enum UnittestAssert {
    AlmostEqual,
    AlmostEquals,
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
    ItemsEqual,
    Less,
    LessEqual,
    MultiLineEqual,
    NotAlmostEqual,
    NotAlmostEquals,
    NotContains,
    NotEqual,
    NotEquals,
    NotIn,
    NotIsInstance,
    NotRegex,
    NotRegexpMatches,
    Raises,
    RaisesMessage,
    RaisesRegexp,
    Regex,
    RegexpMatches,
    SetEqual,
    True,
    Underscore,
}

impl std::fmt::Display for UnittestAssert {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use UnittestAssert::*;
        match self {
            AlmostEqual => write!(f, "assertAlmostEqual"),
            AlmostEquals => write!(f, "assertAlmostEquals"),
            DictEqual => write!(f, "assertDictEqual"),
            Equal => write!(f, "assertEqual"),
            Equals => write!(f, "assertEquals"),
            False => write!(f, "assertFalse"),
            Greater => write!(f, "assertGreater"),
            GreaterEqual => write!(f, "assertGreaterEqual"),
            In => write!(f, "assertIn"),
            Is => write!(f, "assertIs"),
            IsInstance => write!(f, "assertIsInstance"),
            IsNone => write!(f, "assertIsNone"),
            IsNot => write!(f, "assertIsNot"),
            IsNotNone => write!(f, "assertIsNotNone"),
            ItemsEqual => write!(f, "assertItemsEqual"),
            Less => write!(f, "assertLess"),
            LessEqual => write!(f, "assertLessEqual"),
            MultiLineEqual => write!(f, "assertMultiLineEqual"),
            NotAlmostEqual => write!(f, "assertNotAlmostEqual"),
            NotAlmostEquals => write!(f, "assertNotAlmostEquals"),
            NotContains => write!(f, "assertNotContains"),
            NotEqual => write!(f, "assertNotEqual"),
            NotEquals => write!(f, "assertNotEquals"),
            NotIn => write!(f, "assertNotIn"),
            NotIsInstance => write!(f, "assertNotIsInstance"),
            NotRegex => write!(f, "assertNotRegex"),
            NotRegexpMatches => write!(f, "assertNotRegexpMatches"),
            Raises => write!(f, "assertRaises"),
            RaisesMessage => write!(f, "assertRaisesMessage"),
            RaisesRegexp => write!(f, "assertRaisesRegexp"),
            Regex => write!(f, "assertRegex"),
            RegexpMatches => write!(f, "assertRegexpMatches"),
            SetEqual => write!(f, "assertSetEqual"),
            True => write!(f, "assertTrue"),
            Underscore => write!(f, "assert_"),
        }
    }
}

impl TryFrom<&str> for UnittestAssert {
    type Error = String;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        use UnittestAssert::*;
        match value {
            "assertAlmostEqual" => Ok(AlmostEqual),
            "assertAlmostEquals" => Ok(AlmostEquals),
            "assertDictEqual" => Ok(DictEqual),
            "assertEqual" => Ok(Equal),
            "assertEquals" => Ok(Equals),
            "assertFalse" => Ok(False),
            "assertGreater" => Ok(Greater),
            "assertGreaterEqual" => Ok(GreaterEqual),
            "assertIn" => Ok(In),
            "assertIs" => Ok(Is),
            "assertIsInstance" => Ok(IsInstance),
            "assertIsNone" => Ok(IsNone),
            "assertIsNot" => Ok(IsNot),
            "assertIsNotNone" => Ok(IsNotNone),
            "assertItemsEqual" => Ok(ItemsEqual),
            "assertLess" => Ok(Less),
            "assertLessEqual" => Ok(LessEqual),
            "assertMultiLineEqual" => Ok(MultiLineEqual),
            "assertNotAlmostEqual" => Ok(NotAlmostEqual),
            "assertNotAlmostEquals" => Ok(NotAlmostEquals),
            "assertNotContains" => Ok(NotContains),
            "assertNotEqual" => Ok(NotEqual),
            "assertNotEquals" => Ok(NotEquals),
            "assertNotIn" => Ok(NotIn),
            "assertNotIsInstance" => Ok(NotIsInstance),
            "assertNotRegex" => Ok(NotRegex),
            "assertNotRegexpMatches" => Ok(NotRegexpMatches),
            "assertRaises" => Ok(Raises),
            "assertRaisesMessage" => Ok(RaisesMessage),
            "assertRaisesRegexp" => Ok(RaisesRegexp),
            "assertRegex" => Ok(Regex),
            "assertRegexpMatches" => Ok(RegexpMatches),
            "assertSetEqual" => Ok(SetEqual),
            "assertTrue" => Ok(True),
            "assert_" => Ok(Underscore),
            _ => Err(format!("Unknown unittest assert name: {value}")),
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
            AlmostEqual => Signature::new(vec!["first", "second"], vec!["places", "msg", "delta"]),
            AlmostEquals => Signature::new(vec!["first", "second"], vec!["places", "msg", "delta"]),
            DictEqual => Signature::new(vec!["d1", "d2"], vec!["msg"]),
            Equal => Signature::new(vec!["first", "second"], vec!["msg"]),
            Equals => Signature::new(vec!["first", "second"], vec!["msg"]),
            False => Signature::new(vec!["expr"], vec!["msg"]),
            Greater => Signature::new(vec!["first", "second"], vec!["msg"]),
            GreaterEqual => Signature::new(vec!["first", "second"], vec!["msg"]),
            In => Signature::new(vec!["member", "container"], vec!["msg"]),
            Is => Signature::new(vec!["expr1", "expr2"], vec!["msg"]),
            IsInstance => Signature::new(vec!["obj", "cls"], vec!["msg"]),
            IsNone => Signature::new(vec!["expr"], vec!["msg"]),
            IsNot => Signature::new(vec!["expr1", "expr2"], vec!["msg"]),
            IsNotNone => Signature::new(vec!["expr"], vec!["msg"]),
            ItemsEqual => Signature::new(vec!["first", "second"], vec!["msg"]),
            Less => Signature::new(vec!["first", "second"], vec!["msg"]),
            LessEqual => Signature::new(vec!["first", "second"], vec!["msg"]),
            MultiLineEqual => Signature::new(vec!["first", "second"], vec!["msg"]),
            NotAlmostEqual => Signature::new(vec!["first", "second"], vec!["msg"]),
            NotAlmostEquals => Signature::new(vec!["first", "second"], vec!["msg"]),
            NotContains => Signature::new(vec!["container", "member"], vec!["msg"]),
            NotEqual => Signature::new(vec!["first", "second"], vec!["msg"]),
            NotEquals => Signature::new(vec!["first", "second"], vec!["msg"]),
            NotIn => Signature::new(vec!["member", "container"], vec!["msg"]),
            NotIsInstance => Signature::new(vec!["obj", "cls"], vec!["msg"]),
            NotRegex => Signature::new(vec!["text", "regex"], vec!["msg"]),
            NotRegexpMatches => Signature::new(vec!["text", "regex"], vec!["msg"]),
            Raises => Signature::new(vec!["exception"], vec!["msg"]),
            RaisesMessage => Signature::new(vec!["exception", "msg"], vec!["msg"]),
            RaisesRegexp => Signature::new(vec!["exception", "regex"], vec!["msg"]),
            Regex => Signature::new(vec!["text", "regex"], vec!["msg"]),
            RegexpMatches => Signature::new(vec!["text", "regex"], vec!["msg"]),
            SetEqual => Signature::new(vec!["set1", "set2"], vec!["msg"]),
            True => Signature::new(vec!["expr"], vec!["msg"]),
            Underscore => Signature::new(vec!["expr"], vec!["msg"]),
        }
    }

    pub fn arg_hashmap<'a>(
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
        let mut arg_hashmap: FxHashMap<&str, &Expr> = FxHashMap::default();
        for (arg, value) in sig.pos_args.iter().zip(args.iter()) {
            arg_hashmap.insert(arg, value);
        }
        for kw in keywords {
            let arg = kw.node.arg.as_ref().unwrap();
            if !sig.is_valid_arg((*arg).as_str()) {
                return Err(format!("Unexpected keyword argument `{arg}`"));
            }
            arg_hashmap.insert(kw.node.arg.as_ref().unwrap().as_str(), &kw.node.value);
        }
        Ok(arg_hashmap)
    }

    pub fn generate_assert(&self, args: &[Expr], keywords: &[Keyword]) -> Result<Stmt, String> {
        use UnittestAssert::*;
        match self {
            True | False => {
                let args = self.arg_hashmap(args, keywords)?;
                let expr = args.get("expr").ok_or("Missing argument `expr`")?;
                let msg = args.get("msg").copied();
                let bool = Expr::new(
                    Location::default(),
                    Location::default(),
                    ExprKind::Constant {
                        value: Constant::Bool(matches!(self, True)),
                        kind: None,
                    },
                );
                let expr = compare(expr, Cmpop::Is, &bool);
                Ok(assert(&expr, msg))
            }
            Equal | Equals | NotEqual | NotEquals | Greater | GreaterEqual | Less | LessEqual => {
                let args = self.arg_hashmap(args, keywords)?;
                let first = args.get("first").ok_or("Missing argument `first`")?;
                let second = args.get("second").ok_or("Missing argument `second`")?;
                let msg = args.get("msg").copied();
                let cmpop = match self {
                    Equal | Equals => Cmpop::Eq,
                    NotEqual | NotEquals => Cmpop::NotEq,
                    Greater => Cmpop::Gt,
                    GreaterEqual => Cmpop::GtE,
                    Less => Cmpop::Lt,
                    LessEqual => Cmpop::LtE,
                    _ => unreachable!(),
                };
                let expr = compare(first, cmpop, second);
                Ok(assert(&expr, msg))
            }
            Is | IsNot => {
                let args = self.arg_hashmap(args, keywords)?;
                let expr1 = args.get("expr1").ok_or("Missing argument `expr1`")?;
                let expr2 = args.get("expr2").ok_or("Missing argument `expr2`")?;
                let msg = args.get("msg").copied();
                let cmpop = if matches!(self, Is) {
                    Cmpop::Is
                } else {
                    Cmpop::IsNot
                };
                let expr = compare(expr1, cmpop, expr2);
                Ok(assert(&expr, msg))
            }
            In | NotIn => {
                let args = self.arg_hashmap(args, keywords)?;
                let member = args.get("member").ok_or("Missing argument `member`")?;
                let container = args
                    .get("container")
                    .ok_or("Missing argument `container`")?;
                let msg = args.get("msg").copied();
                let cmpop = if matches!(self, In) {
                    Cmpop::In
                } else {
                    Cmpop::NotIn
                };
                let expr = compare(member, cmpop, container);
                Ok(assert(&expr, msg))
            }
            IsNone | IsNotNone => {
                let args = self.arg_hashmap(args, keywords)?;
                let expr = args.get("expr").ok_or("Missing argument `expr`")?;
                let msg = args.get("msg").copied();
                let cmpop = if matches!(self, IsNone) {
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
            IsInstance | NotIsInstance => {
                let args = self.arg_hashmap(args, keywords)?;
                let obj = args.get("obj").ok_or("Missing argument `obj`")?;
                let cls = args.get("cls").ok_or("Missing argument `cls`")?;
                let msg = args.get("msg").copied();
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
                if matches!(self, IsInstance) {
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
            Regex | RegexpMatches | NotRegex | NotRegexpMatches => {
                let args = self.arg_hashmap(args, keywords)?;
                let regex = args.get("regex").ok_or("Missing argument `regex`")?;
                let text = args.get("text").ok_or("Missing argument `text`")?;
                let msg = args.get("msg").copied();
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
                if matches!(self, Regex | RegexpMatches) {
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
