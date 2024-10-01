use anyhow::{anyhow, bail, Result};
use ruff_python_ast::name::Name;
use ruff_python_ast::{
    self as ast, Arguments, CmpOp, Expr, ExprContext, Identifier, Keyword, Stmt, UnaryOp,
};
use ruff_text_size::TextRange;
use rustc_hash::{FxBuildHasher, FxHashMap};

/// An enum to represent the different types of assertions present in the
/// `unittest` module. Note: any variants that can't be replaced with plain
/// `assert` statements are commented out.
#[derive(Copy, Clone)]
pub(crate) enum UnittestAssert {
    AlmostEqual,
    AlmostEquals,
    CountEqual,
    DictContainsSubset,
    DictEqual,
    Equal,
    Equals,
    FailIf,
    FailIfAlmostEqual,
    FailIfEqual,
    FailUnless,
    FailUnlessAlmostEqual,
    FailUnlessEqual,
    // FailUnlessRaises,
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
            UnittestAssert::DictContainsSubset => write!(f, "assertDictContainsSubset"),
            UnittestAssert::DictEqual => write!(f, "assertDictEqual"),
            UnittestAssert::Equal => write!(f, "assertEqual"),
            UnittestAssert::Equals => write!(f, "assertEquals"),
            UnittestAssert::FailIf => write!(f, "failIf"),
            UnittestAssert::FailIfAlmostEqual => write!(f, "failIfAlmostEqual"),
            UnittestAssert::FailIfEqual => write!(f, "failIfEqual"),
            UnittestAssert::FailUnless => write!(f, "failUnless"),
            UnittestAssert::FailUnlessAlmostEqual => write!(f, "failUnlessAlmostEqual"),
            UnittestAssert::FailUnlessEqual => write!(f, "failUnlessEqual"),
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
            "failIf" => Ok(UnittestAssert::FailIf),
            "failIfAlmostEqual" => Ok(UnittestAssert::FailIfAlmostEqual),
            "failIfEqual" => Ok(UnittestAssert::FailIfEqual),
            "failUnless" => Ok(UnittestAssert::FailUnless),
            "failUnlessAlmostEqual" => Ok(UnittestAssert::FailUnlessAlmostEqual),
            "failUnlessEqual" => Ok(UnittestAssert::FailUnlessEqual),
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
    Stmt::Assert(ast::StmtAssert {
        test: Box::new(expr.clone()),
        msg: msg.map(|msg| Box::new(msg.clone())),
        range: TextRange::default(),
    })
}

fn compare(left: &Expr, cmp_op: CmpOp, right: &Expr) -> Expr {
    Expr::Compare(ast::ExprCompare {
        left: Box::new(left.clone()),
        ops: Box::from([cmp_op]),
        comparators: Box::from([right.clone()]),
        range: TextRange::default(),
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
            UnittestAssert::FailIf => &["expr", "msg"],
            UnittestAssert::FailIfAlmostEqual => &["first", "second", "msg"],
            UnittestAssert::FailIfEqual => &["first", "second", "msg"],
            UnittestAssert::FailUnless => &["expr", "msg"],
            UnittestAssert::FailUnlessAlmostEqual => &["first", "second", "places", "msg", "delta"],
            UnittestAssert::FailUnlessEqual => &["first", "second", "places", "msg", "delta"],
        }
    }

    /// Create a map from argument name to value.
    pub(crate) fn args_map<'a>(
        &'a self,
        args: &'a [Expr],
        keywords: &'a [Keyword],
    ) -> Result<FxHashMap<&'a str, &'a Expr>> {
        // If we have variable-length arguments, abort.
        if args.iter().any(Expr::is_starred_expr) || keywords.iter().any(|kw| kw.arg.is_none()) {
            bail!("Variable-length arguments are not supported");
        }

        let arg_spec = self.arg_spec();

        // If any of the keyword arguments are not in the argument spec, abort.
        if keywords.iter().any(|kw| {
            kw.arg
                .as_ref()
                .is_some_and(|kwarg_name| !arg_spec.contains(&kwarg_name.as_str()))
        }) {
            bail!("Unknown keyword argument");
        }

        // Generate a map from argument name to value.
        let mut args_map: FxHashMap<&str, &Expr> =
            FxHashMap::with_capacity_and_hasher(args.len() + keywords.len(), FxBuildHasher);

        // Process positional arguments.
        for (arg_name, value) in arg_spec.iter().zip(args.iter()) {
            args_map.insert(arg_name, value);
        }

        // Process keyword arguments.
        for arg_name in arg_spec.iter().skip(args.len()) {
            if let Some(value) = keywords.iter().find_map(|keyword| {
                if keyword
                    .arg
                    .as_ref()
                    .is_some_and(|kwarg_name| &kwarg_name == arg_name)
                {
                    Some(&keyword.value)
                } else {
                    None
                }
            }) {
                args_map.insert(arg_name, value);
            }
        }

        Ok(args_map)
    }

    pub(crate) fn generate_assert(self, args: &[Expr], keywords: &[Keyword]) -> Result<Stmt> {
        let args = self.args_map(args, keywords)?;
        match self {
            UnittestAssert::True
            | UnittestAssert::False
            | UnittestAssert::FailUnless
            | UnittestAssert::FailIf => {
                let expr = *args
                    .get("expr")
                    .ok_or_else(|| anyhow!("Missing argument `expr`"))?;
                let msg = args.get("msg").copied();
                Ok(
                    if matches!(self, UnittestAssert::False | UnittestAssert::FailIf) {
                        assert(
                            &Expr::UnaryOp(ast::ExprUnaryOp {
                                op: UnaryOp::Not,
                                operand: Box::new(expr.clone()),
                                range: TextRange::default(),
                            }),
                            msg,
                        )
                    } else {
                        assert(expr, msg)
                    },
                )
            }
            UnittestAssert::Equal
            | UnittestAssert::Equals
            | UnittestAssert::FailUnlessEqual
            | UnittestAssert::NotEqual
            | UnittestAssert::NotEquals
            | UnittestAssert::FailIfEqual
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
                let cmp_op = match self {
                    UnittestAssert::Equal
                    | UnittestAssert::Equals
                    | UnittestAssert::FailUnlessEqual => CmpOp::Eq,
                    UnittestAssert::NotEqual
                    | UnittestAssert::NotEquals
                    | UnittestAssert::FailIfEqual => CmpOp::NotEq,
                    UnittestAssert::Greater => CmpOp::Gt,
                    UnittestAssert::GreaterEqual => CmpOp::GtE,
                    UnittestAssert::Less => CmpOp::Lt,
                    UnittestAssert::LessEqual => CmpOp::LtE,
                    UnittestAssert::Is => CmpOp::Is,
                    UnittestAssert::IsNot => CmpOp::IsNot,
                    _ => unreachable!(),
                };
                let expr = compare(first, cmp_op, second);
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
                let cmp_op = if matches!(self, UnittestAssert::In) {
                    CmpOp::In
                } else {
                    CmpOp::NotIn
                };
                let expr = compare(member, cmp_op, container);
                Ok(assert(&expr, msg))
            }
            UnittestAssert::IsNone | UnittestAssert::IsNotNone => {
                let expr = args
                    .get("expr")
                    .ok_or_else(|| anyhow!("Missing argument `expr`"))?;
                let msg = args.get("msg").copied();
                let cmp_op = if matches!(self, UnittestAssert::IsNone) {
                    CmpOp::Is
                } else {
                    CmpOp::IsNot
                };
                let node = Expr::NoneLiteral(ast::ExprNoneLiteral {
                    range: TextRange::default(),
                });
                let expr = compare(expr, cmp_op, &node);
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
                let node = ast::ExprName {
                    id: Name::new_static("isinstance"),
                    ctx: ExprContext::Load,
                    range: TextRange::default(),
                };
                let node1 = ast::ExprCall {
                    func: Box::new(node.into()),
                    arguments: Arguments {
                        args: Box::from([(**obj).clone(), (**cls).clone()]),
                        keywords: Box::from([]),
                        range: TextRange::default(),
                    },
                    range: TextRange::default(),
                };
                let isinstance = node1.into();
                if matches!(self, UnittestAssert::IsInstance) {
                    Ok(assert(&isinstance, msg))
                } else {
                    let node = ast::ExprUnaryOp {
                        op: UnaryOp::Not,
                        operand: Box::new(isinstance),
                        range: TextRange::default(),
                    };
                    let expr = node.into();
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
                let node = ast::ExprName {
                    id: Name::new_static("re"),
                    ctx: ExprContext::Load,
                    range: TextRange::default(),
                };
                let node1 = ast::ExprAttribute {
                    value: Box::new(node.into()),
                    attr: Identifier::new("search".to_string(), TextRange::default()),
                    ctx: ExprContext::Load,
                    range: TextRange::default(),
                };
                let node2 = ast::ExprCall {
                    func: Box::new(node1.into()),
                    arguments: Arguments {
                        args: Box::from([(**regex).clone(), (**text).clone()]),
                        keywords: Box::from([]),
                        range: TextRange::default(),
                    },
                    range: TextRange::default(),
                };
                let re_search = node2.into();
                if matches!(self, UnittestAssert::Regex | UnittestAssert::RegexpMatches) {
                    Ok(assert(&re_search, msg))
                } else {
                    let node = ast::ExprUnaryOp {
                        op: UnaryOp::Not,
                        operand: Box::new(re_search),
                        range: TextRange::default(),
                    };
                    Ok(assert(&node.into(), msg))
                }
            }
            _ => bail!("Cannot fix `{self}`"),
        }
    }
}
