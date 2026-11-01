use anyhow::{Result, anyhow, bail};
use ruff_python_ast::{Builder, CmpOp, Expr, NodeList, UnaryOp, build};
use ruff_python_codegen::Generator;
use ruff_text_size::TextRange;
use rustc_hash::{FxBuildHasher, FxHashMap};

use crate::fix::syntax;

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
    fn args_map<'a>(
        &'a self,
        args: NodeList<'a, ruff_python_ast::kind::Expr>,
        keywords: NodeList<'a, ruff_python_ast::kind::Keyword>,
    ) -> Result<FxHashMap<&'a str, Expr<'a>>> {
        // If we have variable-length arguments, abort.
        if args.iter().any(ruff_python_ast::Expr::is_starred_expr)
            || keywords.iter().any(|kw| kw.arg().is_none())
        {
            bail!("Variable-length arguments are not supported");
        }

        let arg_spec = self.arg_spec();

        // If any of the keyword arguments are not in the argument spec, abort.
        if keywords.iter().any(|kw| {
            kw.arg()
                .is_some_and(|kwarg_name| !arg_spec.contains(&kwarg_name.as_str()))
        }) {
            bail!("Unknown keyword argument");
        }

        // Generate a map from argument name to value.
        let mut args_map: FxHashMap<&str, Expr<'_>> =
            FxHashMap::with_capacity_and_hasher(args.len() + keywords.len(), FxBuildHasher);

        // Process positional arguments.
        for (arg_name, value) in arg_spec.iter().zip(args) {
            args_map.insert(arg_name, value);
        }

        // Process keyword arguments.
        for arg_name in arg_spec.iter().skip(args.len()) {
            if let Some(value) = keywords.iter().find_map(|keyword| {
                if keyword
                    .arg()
                    .is_some_and(|kwarg_name| kwarg_name == *arg_name)
                {
                    Some(keyword.value())
                } else {
                    None
                }
            }) {
                args_map.insert(arg_name, value);
            }
        }

        Ok(args_map)
    }

    pub(crate) fn generate_assert(
        self,
        args: NodeList<'_, ruff_python_ast::kind::Expr>,
        keywords: NodeList<'_, ruff_python_ast::kind::Keyword>,
        generator: Generator,
    ) -> Result<String> {
        let args = self.args_map(args, keywords)?;
        let required = |name: &str| {
            args.get(name)
                .copied()
                .ok_or_else(|| anyhow!("Missing argument `{name}`"))
        };
        let mut builder = Builder::default();
        let mut compare = |left: &str, op: CmpOp, right: &str| -> Result<_> {
            let left = builder.copy_node(required(left)?);
            let right = builder.copy_node(required(right)?);
            Ok(syntax::compare(&mut builder, left, op, right))
        };
        let test = match self {
            Self::True | Self::False | Self::FailUnless | Self::FailIf => {
                builder.copy_node(required("expr")?)
            }
            Self::Equal | Self::Equals | Self::FailUnlessEqual => {
                compare("first", CmpOp::Eq, "second")?
            }
            Self::NotEqual | Self::NotEquals | Self::FailIfEqual => {
                compare("first", CmpOp::NotEq, "second")?
            }
            Self::Greater => compare("first", CmpOp::Gt, "second")?,
            Self::GreaterEqual => compare("first", CmpOp::GtE, "second")?,
            Self::Less => compare("first", CmpOp::Lt, "second")?,
            Self::LessEqual => compare("first", CmpOp::LtE, "second")?,
            Self::Is => compare("first", CmpOp::Is, "second")?,
            Self::IsNot => compare("first", CmpOp::IsNot, "second")?,
            Self::In => compare("member", CmpOp::In, "container")?,
            Self::NotIn => compare("member", CmpOp::NotIn, "container")?,
            Self::IsNone | Self::IsNotNone => {
                let expr = builder.copy_node(required("expr")?);
                let none = syntax::none(&mut builder);
                let op = if matches!(self, Self::IsNone) {
                    CmpOp::Is
                } else {
                    CmpOp::IsNot
                };
                syntax::compare(&mut builder, expr, op, none)
            }
            Self::IsInstance | Self::NotIsInstance => {
                let args = builder.copy_nodes([required("obj")?, required("cls")?]);
                syntax::call_name(&mut builder, "isinstance", args)
            }
            Self::Regex | Self::RegexpMatches | Self::NotRegex | Self::NotRegexpMatches => {
                let re = syntax::name(&mut builder, "re");
                let args = builder.copy_nodes([required("regex")?, required("text")?]);
                syntax::call_method(&mut builder, re, "search", args)
            }
            _ => bail!("Cannot fix `{self}`"),
        };
        let test = if matches!(
            self,
            Self::False
                | Self::FailIf
                | Self::NotIsInstance
                | Self::NotRegex
                | Self::NotRegexpMatches
        ) {
            builder
                .add(build::ExprUnaryOp {
                    range: TextRange::default(),
                    op: UnaryOp::Not,
                    operand: test,
                })
                .into()
        } else {
            test
        };
        let msg = args.get("msg").map(|msg| builder.copy_node(*msg));
        let statement = builder.add(build::StmtAssert {
            range: TextRange::default(),
            test,
            msg,
        });
        Ok(generator.stmt(builder.get(statement).into()))
    }
}
