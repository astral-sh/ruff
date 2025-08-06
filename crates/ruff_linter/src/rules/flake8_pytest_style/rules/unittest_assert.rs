use anyhow::{Result, anyhow, bail};
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
            Self::AlmostEqual => write!(f, "assertAlmostEqual"),
            Self::AlmostEquals => write!(f, "assertAlmostEquals"),
            Self::CountEqual => write!(f, "assertCountEqual"),
            Self::DictContainsSubset => write!(f, "assertDictContainsSubset"),
            Self::DictEqual => write!(f, "assertDictEqual"),
            Self::Equal => write!(f, "assertEqual"),
            Self::Equals => write!(f, "assertEquals"),
            Self::FailIf => write!(f, "failIf"),
            Self::FailIfAlmostEqual => write!(f, "failIfAlmostEqual"),
            Self::FailIfEqual => write!(f, "failIfEqual"),
            Self::FailUnless => write!(f, "failUnless"),
            Self::FailUnlessAlmostEqual => write!(f, "failUnlessAlmostEqual"),
            Self::FailUnlessEqual => write!(f, "failUnlessEqual"),
            Self::False => write!(f, "assertFalse"),
            Self::Greater => write!(f, "assertGreater"),
            Self::GreaterEqual => write!(f, "assertGreaterEqual"),
            Self::In => write!(f, "assertIn"),
            Self::Is => write!(f, "assertIs"),
            Self::IsInstance => write!(f, "assertIsInstance"),
            Self::IsNone => write!(f, "assertIsNone"),
            Self::IsNot => write!(f, "assertIsNot"),
            Self::IsNotNone => write!(f, "assertIsNotNone"),
            Self::Less => write!(f, "assertLess"),
            Self::LessEqual => write!(f, "assertLessEqual"),
            Self::ListEqual => write!(f, "assertListEqual"),
            Self::MultiLineEqual => write!(f, "assertMultiLineEqual"),
            Self::NotAlmostEqual => write!(f, "assertNotAlmostEqual"),
            Self::NotAlmostEquals => write!(f, "assertNotAlmostEquals"),
            Self::NotEqual => write!(f, "assertNotEqual"),
            Self::NotEquals => write!(f, "assertNotEquals"),
            Self::NotIn => write!(f, "assertNotIn"),
            Self::NotIsInstance => write!(f, "assertNotIsInstance"),
            Self::NotRegex => write!(f, "assertNotRegex"),
            Self::NotRegexpMatches => write!(f, "assertNotRegexpMatches"),
            Self::Regex => write!(f, "assertRegex"),
            Self::RegexpMatches => write!(f, "assertRegexpMatches"),
            Self::SequenceEqual => write!(f, "assertSequenceEqual"),
            Self::SetEqual => write!(f, "assertSetEqual"),
            Self::True => write!(f, "assertTrue"),
            Self::TupleEqual => write!(f, "assertTupleEqual"),
            Self::Underscore => write!(f, "assert_"),
        }
    }
}

impl TryFrom<&str> for UnittestAssert {
    type Error = String;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "assertAlmostEqual" => Ok(Self::AlmostEqual),
            "assertAlmostEquals" => Ok(Self::AlmostEquals),
            "assertCountEqual" => Ok(Self::CountEqual),
            "assertDictContainsSubset" => Ok(Self::DictContainsSubset),
            "assertDictEqual" => Ok(Self::DictEqual),
            "assertEqual" => Ok(Self::Equal),
            "assertEquals" => Ok(Self::Equals),
            "failIf" => Ok(Self::FailIf),
            "failIfAlmostEqual" => Ok(Self::FailIfAlmostEqual),
            "failIfEqual" => Ok(Self::FailIfEqual),
            "failUnless" => Ok(Self::FailUnless),
            "failUnlessAlmostEqual" => Ok(Self::FailUnlessAlmostEqual),
            "failUnlessEqual" => Ok(Self::FailUnlessEqual),
            "assertFalse" => Ok(Self::False),
            "assertGreater" => Ok(Self::Greater),
            "assertGreaterEqual" => Ok(Self::GreaterEqual),
            "assertIn" => Ok(Self::In),
            "assertIs" => Ok(Self::Is),
            "assertIsInstance" => Ok(Self::IsInstance),
            "assertIsNone" => Ok(Self::IsNone),
            "assertIsNot" => Ok(Self::IsNot),
            "assertIsNotNone" => Ok(Self::IsNotNone),
            "assertLess" => Ok(Self::Less),
            "assertLessEqual" => Ok(Self::LessEqual),
            "assertListEqual" => Ok(Self::ListEqual),
            "assertMultiLineEqual" => Ok(Self::MultiLineEqual),
            "assertNotAlmostEqual" => Ok(Self::NotAlmostEqual),
            "assertNotAlmostEquals" => Ok(Self::NotAlmostEquals),
            "assertNotEqual" => Ok(Self::NotEqual),
            "assertNotEquals" => Ok(Self::NotEquals),
            "assertNotIn" => Ok(Self::NotIn),
            "assertNotIsInstance" => Ok(Self::NotIsInstance),
            "assertNotRegex" => Ok(Self::NotRegex),
            "assertNotRegexpMatches" => Ok(Self::NotRegexpMatches),
            "assertRegex" => Ok(Self::Regex),
            "assertRegexpMatches" => Ok(Self::RegexpMatches),
            "assertSequenceEqual" => Ok(Self::SequenceEqual),
            "assertSetEqual" => Ok(Self::SetEqual),
            "assertTrue" => Ok(Self::True),
            "assertTupleEqual" => Ok(Self::TupleEqual),
            "assert_" => Ok(Self::Underscore),
            _ => Err(format!("Unknown unittest assert method: {value}")),
        }
    }
}

fn assert(expr: &Expr, msg: Option<&Expr>) -> Stmt {
    Stmt::Assert(ast::StmtAssert {
        test: Box::new(expr.clone()),
        msg: msg.map(|msg| Box::new(msg.clone())),
        range: TextRange::default(),
        node_index: ruff_python_ast::AtomicNodeIndex::dummy(),
    })
}

fn compare(left: &Expr, cmp_op: CmpOp, right: &Expr) -> Expr {
    Expr::Compare(ast::ExprCompare {
        left: Box::new(left.clone()),
        ops: Box::from([cmp_op]),
        comparators: Box::from([right.clone()]),
        range: TextRange::default(),
        node_index: ruff_python_ast::AtomicNodeIndex::dummy(),
    })
}

impl UnittestAssert {
    fn arg_spec(&self) -> &[&str] {
        match self {
            Self::AlmostEqual => &["first", "second", "places", "msg", "delta"],
            Self::AlmostEquals => &["first", "second", "places", "msg", "delta"],
            Self::CountEqual => &["first", "second", "msg"],
            Self::DictContainsSubset => &["subset", "dictionary", "msg"],
            Self::DictEqual => &["first", "second", "msg"],
            Self::Equal => &["first", "second", "msg"],
            Self::Equals => &["first", "second", "msg"],
            Self::False => &["expr", "msg"],
            Self::Greater => &["first", "second", "msg"],
            Self::GreaterEqual => &["first", "second", "msg"],
            Self::In => &["member", "container", "msg"],
            Self::Is => &["first", "second", "msg"],
            Self::IsInstance => &["obj", "cls", "msg"],
            Self::IsNone => &["expr", "msg"],
            Self::IsNot => &["first", "second", "msg"],
            Self::IsNotNone => &["expr", "msg"],
            Self::Less => &["first", "second", "msg"],
            Self::LessEqual => &["first", "second", "msg"],
            Self::ListEqual => &["first", "second", "msg"],
            Self::MultiLineEqual => &["first", "second", "msg"],
            Self::NotAlmostEqual => &["first", "second", "msg"],
            Self::NotAlmostEquals => &["first", "second", "msg"],
            Self::NotEqual => &["first", "second", "msg"],
            Self::NotEquals => &["first", "second", "msg"],
            Self::NotIn => &["member", "container", "msg"],
            Self::NotIsInstance => &["obj", "cls", "msg"],
            Self::NotRegex => &["text", "regex", "msg"],
            Self::NotRegexpMatches => &["text", "regex", "msg"],
            Self::Regex => &["text", "regex", "msg"],
            Self::RegexpMatches => &["text", "regex", "msg"],
            Self::SequenceEqual => &["first", "second", "msg", "seq_type"],
            Self::SetEqual => &["first", "second", "msg"],
            Self::True => &["expr", "msg"],
            Self::TupleEqual => &["first", "second", "msg"],
            Self::Underscore => &["expr", "msg"],
            Self::FailIf => &["expr", "msg"],
            Self::FailIfAlmostEqual => &["first", "second", "msg"],
            Self::FailIfEqual => &["first", "second", "msg"],
            Self::FailUnless => &["expr", "msg"],
            Self::FailUnlessAlmostEqual => &["first", "second", "places", "msg", "delta"],
            Self::FailUnlessEqual => &["first", "second", "places", "msg", "delta"],
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
        for (arg_name, value) in arg_spec.iter().zip(args) {
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
            Self::True | Self::False | Self::FailUnless | Self::FailIf => {
                let expr = *args
                    .get("expr")
                    .ok_or_else(|| anyhow!("Missing argument `expr`"))?;
                let msg = args.get("msg").copied();
                Ok(if matches!(self, Self::False | Self::FailIf) {
                    assert(
                        &Expr::UnaryOp(ast::ExprUnaryOp {
                            op: UnaryOp::Not,
                            operand: Box::new(expr.clone()),
                            range: TextRange::default(),
                            node_index: ruff_python_ast::AtomicNodeIndex::dummy(),
                        }),
                        msg,
                    )
                } else {
                    assert(expr, msg)
                })
            }
            Self::Equal
            | Self::Equals
            | Self::FailUnlessEqual
            | Self::NotEqual
            | Self::NotEquals
            | Self::FailIfEqual
            | Self::Greater
            | Self::GreaterEqual
            | Self::Less
            | Self::LessEqual
            | Self::Is
            | Self::IsNot => {
                let first = args
                    .get("first")
                    .ok_or_else(|| anyhow!("Missing argument `first`"))?;
                let second = args
                    .get("second")
                    .ok_or_else(|| anyhow!("Missing argument `second`"))?;
                let msg = args.get("msg").copied();
                let cmp_op = match self {
                    Self::Equal | Self::Equals | Self::FailUnlessEqual => CmpOp::Eq,
                    Self::NotEqual | Self::NotEquals | Self::FailIfEqual => CmpOp::NotEq,
                    Self::Greater => CmpOp::Gt,
                    Self::GreaterEqual => CmpOp::GtE,
                    Self::Less => CmpOp::Lt,
                    Self::LessEqual => CmpOp::LtE,
                    Self::Is => CmpOp::Is,
                    Self::IsNot => CmpOp::IsNot,
                    _ => unreachable!(),
                };
                let expr = compare(first, cmp_op, second);
                Ok(assert(&expr, msg))
            }
            Self::In | Self::NotIn => {
                let member = args
                    .get("member")
                    .ok_or_else(|| anyhow!("Missing argument `member`"))?;
                let container = args
                    .get("container")
                    .ok_or_else(|| anyhow!("Missing argument `container`"))?;
                let msg = args.get("msg").copied();
                let cmp_op = if matches!(self, Self::In) {
                    CmpOp::In
                } else {
                    CmpOp::NotIn
                };
                let expr = compare(member, cmp_op, container);
                Ok(assert(&expr, msg))
            }
            Self::IsNone | Self::IsNotNone => {
                let expr = args
                    .get("expr")
                    .ok_or_else(|| anyhow!("Missing argument `expr`"))?;
                let msg = args.get("msg").copied();
                let cmp_op = if matches!(self, Self::IsNone) {
                    CmpOp::Is
                } else {
                    CmpOp::IsNot
                };
                let node = Expr::NoneLiteral(ast::ExprNoneLiteral {
                    range: TextRange::default(),
                    node_index: ruff_python_ast::AtomicNodeIndex::dummy(),
                });
                let expr = compare(expr, cmp_op, &node);
                Ok(assert(&expr, msg))
            }
            Self::IsInstance | Self::NotIsInstance => {
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
                    node_index: ruff_python_ast::AtomicNodeIndex::dummy(),
                };
                let node1 = ast::ExprCall {
                    func: Box::new(node.into()),
                    arguments: Arguments {
                        args: Box::from([(**obj).clone(), (**cls).clone()]),
                        keywords: Box::from([]),
                        range: TextRange::default(),
                        node_index: ruff_python_ast::AtomicNodeIndex::dummy(),
                    },
                    range: TextRange::default(),
                    node_index: ruff_python_ast::AtomicNodeIndex::dummy(),
                };
                let isinstance = node1.into();
                if matches!(self, Self::IsInstance) {
                    Ok(assert(&isinstance, msg))
                } else {
                    let node = ast::ExprUnaryOp {
                        op: UnaryOp::Not,
                        operand: Box::new(isinstance),
                        range: TextRange::default(),
                        node_index: ruff_python_ast::AtomicNodeIndex::dummy(),
                    };
                    let expr = node.into();
                    Ok(assert(&expr, msg))
                }
            }
            Self::Regex | Self::RegexpMatches | Self::NotRegex | Self::NotRegexpMatches => {
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
                    node_index: ruff_python_ast::AtomicNodeIndex::dummy(),
                };
                let node1 = ast::ExprAttribute {
                    value: Box::new(node.into()),
                    attr: Identifier::new("search".to_string(), TextRange::default()),
                    ctx: ExprContext::Load,
                    range: TextRange::default(),
                    node_index: ruff_python_ast::AtomicNodeIndex::dummy(),
                };
                let node2 = ast::ExprCall {
                    func: Box::new(node1.into()),
                    arguments: Arguments {
                        args: Box::from([(**regex).clone(), (**text).clone()]),
                        keywords: Box::from([]),
                        range: TextRange::default(),
                        node_index: ruff_python_ast::AtomicNodeIndex::dummy(),
                    },
                    range: TextRange::default(),
                    node_index: ruff_python_ast::AtomicNodeIndex::dummy(),
                };
                let re_search = node2.into();
                if matches!(self, Self::Regex | Self::RegexpMatches) {
                    Ok(assert(&re_search, msg))
                } else {
                    let node = ast::ExprUnaryOp {
                        op: UnaryOp::Not,
                        operand: Box::new(re_search),
                        range: TextRange::default(),
                        node_index: ruff_python_ast::AtomicNodeIndex::dummy(),
                    };
                    Ok(assert(&node.into(), msg))
                }
            }
            _ => bail!("Cannot fix `{self}`"),
        }
    }
}
