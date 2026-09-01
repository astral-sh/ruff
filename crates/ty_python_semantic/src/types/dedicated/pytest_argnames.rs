use derive_more::{Constructor, From, IntoIterator};
use itertools::Itertools;
use ruff_python_ast as ast;
use ruff_python_stdlib::identifiers::is_identifier;
use ruff_text_size::{Ranged, TextRange};

#[derive(Debug, Clone)]
/// An argname that may or may not be known.
/// It may come from a sequence of strings or a string literal.
pub(crate) enum Argname {
    Error(String),
    Request,
    Valid(String),
    Unknown,
}

#[derive(Debug, Constructor)]
/// A special case where argnames is a string literal with a single argname.
pub(crate) struct SingleArgname {
    pub(crate) argname: Argname,
    pub(crate) range: TextRange,
}

#[derive(Debug, Constructor, IntoIterator)]
#[into_iterator(ref)]
/// Argnames passed either by comma separated values or a sequence of strings.
/// This handles all cases except the special case above.
pub(crate) struct MultipleArgnames {
    pub(crate) argnames: Vec<SingleArgname>,
}

impl MultipleArgnames {
    pub(crate) fn iter(&self) -> <&Self as IntoIterator>::IntoIter {
        self.into_iter()
    }
}

impl FromIterator<SingleArgname> for MultipleArgnames {
    fn from_iter<T: IntoIterator<Item = SingleArgname>>(iter: T) -> Self {
        Self::new(Vec::from_iter(iter))
    }
}

#[derive(Debug, From)]
/// Argnames that are formed from a string literal.
pub(crate) enum StringLiteralArgnames {
    Single(SingleArgname),
    Multiple(MultipleArgnames),
}

#[derive(Debug, From)]
/// Argnames that are formed by a sequence of strings.
/// This does not cover the `SingleArgname` case, because it is not possible.
/// As a sequence, a single argname is treated differently:
// ```python
// @pytest.mark.parametrize(["name"], [("first",), ("second",)])
// def test_name(name: str) -> None: ...
//
// ```
pub(crate) enum SequenceArgnames {
    Multiple(MultipleArgnames),
}

#[derive(Debug, From)]
/// Argnames that are read off a `pytest.mark.parametrize` decorator.
pub(crate) enum Argnames {
    Unknown,
    StringLiteral(StringLiteralArgnames),
    Sequence(SequenceArgnames),
}

impl Argnames {
    pub(crate) fn from_expr(expr: &ast::Expr) -> Self {
        if let Some(literal) = expr.as_string_literal_expr() {
            StringLiteralArgnames::from_literal(literal).into()
        } else if let Some(list_expr) = expr.as_list_expr() {
            SequenceArgnames::from_elts(&list_expr.elts).into()
        } else if let Some(tuple_expr) = expr.as_tuple_expr() {
            SequenceArgnames::from_elts(&tuple_expr.elts).into()
        } else {
            Argnames::Unknown
        }
    }
}

impl StringLiteralArgnames {
    fn from_literal(literal: &ast::ExprStringLiteral) -> Self {
        let separated_names = literal.value.to_str().split(',').map(str::trim);
        let filtered_names = separated_names
            .filter(|name| !name.is_empty())
            .collect_vec();
        let range = literal.range();
        if let [name] = &filtered_names[..] {
            SingleArgname::from_str(name, range).into()
        } else {
            MultipleArgnames::from_strs(filtered_names.into_iter().map(|name| (name, range))).into()
        }
    }
}

impl SequenceArgnames {
    fn from_elts(elts: &[ast::Expr]) -> Self {
        MultipleArgnames::from_exprs(elts).into()
    }
}

impl MultipleArgnames {
    fn from_exprs<'a>(exprs: impl IntoIterator<Item = &'a ast::Expr>) -> Self {
        exprs.into_iter().map(SingleArgname::from_expr).collect()
    }

    fn from_strs<'a>(argnames: impl IntoIterator<Item = (&'a str, TextRange)>) -> Self {
        argnames
            .into_iter()
            .map(|(name, range)| SingleArgname::from_str(name, range))
            .collect()
    }
}

impl SingleArgname {
    fn from_expr(expr: &ast::Expr) -> Self {
        if let Some(literal) = expr.as_string_literal_expr() {
            Self::from_literal(literal)
        } else {
            Self::unknown(expr.range())
        }
    }

    fn unknown(range: TextRange) -> Self {
        Self::new(Argname::Unknown, range)
    }

    fn from_literal(literal: &ast::ExprStringLiteral) -> Self {
        Self::from_str(literal.value.to_str(), literal.range())
    }

    fn from_str(name: &str, range: TextRange) -> Self {
        Self::new(Argname::from_str(name), range)
    }
}

impl Argname {
    fn from_str(name: &str) -> Self {
        if name == "request" {
            Self::Request
        } else if is_identifier(name) {
            Self::Valid(name.to_owned())
        } else {
            Self::Error(name.to_owned())
        }
    }
}
