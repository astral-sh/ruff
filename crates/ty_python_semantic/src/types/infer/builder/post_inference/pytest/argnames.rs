use std::{convert, slice};

use derive_more::{Constructor, From, IntoIterator};
use itertools::Itertools;
use ruff_python_ast::{self as ast};
use ruff_python_stdlib::identifiers::is_identifier;
use ruff_text_size::{Ranged, TextRange};

use crate::types::{
    diagnostic::{PYTEST_INVALID_ARGNAMES_LITERAL, PYTEST_REQUEST_KEYWORD},
    infer::TypeInferenceBuilder,
};

#[derive(Debug)]
pub(crate) struct SingleArgname {
    name: String,
    range: TextRange,
}

impl SingleArgname {
    pub(crate) fn new(argname: impl Into<String>, range: TextRange) -> Self {
        Self {
            name: argname.into(),
            range,
        }
    }

    pub(crate) fn name(&self) -> &str {
        &self.name
    }

    pub(crate) fn range(&self) -> TextRange {
        self.range
    }
}

// Even though it is called multiple, it make contain a single argname.
// This may occurs when argnames passed as a sequence. For example,
// ```python
// @pytest.mark.parametrize(["name"], [("first",), ("second",)])
// def test_name(name: str) -> None: ...
//
// ```
#[derive(Debug, From, Constructor, IntoIterator)]
#[into_iterator(ref)]
pub(crate) struct MultipleArgnames {
    argnames: Vec<SingleArgname>,
}

impl MultipleArgnames {
    pub(crate) fn iter(&self) -> <&Self as IntoIterator>::IntoIter {
        self.into_iter()
    }
}

impl FromIterator<SingleArgname> for MultipleArgnames {
    fn from_iter<I: IntoIterator<Item = SingleArgname>>(iter: I) -> Self {
        Self::new(Vec::from_iter(iter))
    }
}

#[derive(Debug, From)]
pub(crate) enum KnownArgnames {
    Single(SingleArgname),
    Multiple(MultipleArgnames),
}

impl<'a> IntoIterator for &'a KnownArgnames {
    type Item = &'a SingleArgname;

    type IntoIter = slice::Iter<'a, SingleArgname>;

    fn into_iter(self) -> Self::IntoIter {
        match self {
            KnownArgnames::Single(argname) => slice::from_ref(argname).iter(),
            KnownArgnames::Multiple(argnames) => argnames.iter(),
        }
    }
}

#[derive(Debug, Default)]
pub(crate) enum Argnames {
    Known(KnownArgnames),
    #[default]
    Unknown,
}

impl<T: Into<KnownArgnames>> From<T> for Argnames {
    fn from(value: T) -> Self {
        Self::Known(value.into())
    }
}

impl TypeInferenceBuilder<'_, '_> {
    pub(crate) fn parse_argnames_expression(&self, argnames_argument: &ast::Expr) -> Argnames {
        let db = self.db();
        if let Some(literal) = self.expression_type(argnames_argument).as_string_literal() {
            self.parse_argnames_string(literal.value(db), argnames_argument.range())
        } else if let Some(list_expr) = argnames_argument.as_list_expr() {
            self.parse_argnames_sequence(&list_expr.elts)
        } else if let Some(tuple_expr) = argnames_argument.as_tuple_expr() {
            self.parse_argnames_sequence(&tuple_expr.elts)
        } else {
            Argnames::Unknown
        }
    }

    /// Converts a known string into `Argnames`, generating a diagnostic if it is not valid.
    fn parse_argnames_string(&self, value: &str, range: TextRange) -> Argnames {
        let separated_names = value.split(',').map(str::trim);
        let filtered_names = separated_names
            .filter(|name| !name.is_empty())
            .collect_vec();
        if self.contains_invalid_identifiers(&filtered_names, range) {
            return Argnames::Unknown;
        }
        match &filtered_names[..] {
            [name] => SingleArgname::new(*name, range).into(),
            _ => filtered_names
                .into_iter()
                .map(|name| SingleArgname::new(name, range))
                .collect::<MultipleArgnames>()
                .into(),
        }
    }

    fn contains_invalid_identifiers(&self, filtered_names: &Vec<&str>, range: TextRange) -> bool {
        // Collect checks to ensure that multiple errors are reported.
        let checks = filtered_names
            .iter()
            .map(|name| !self.check_valid_identifier(name, range))
            .collect_vec();
        checks.into_iter().any(convert::identity)
    }

    /// Converts the sequence (list or tuple elements) into multiple argnames.
    /// If there is an error, `Argnames::Unknown` is returned and a diagnostic is generated.
    fn parse_argnames_sequence(&self, sequence: &[ast::Expr]) -> Argnames {
        self.parse_multiple_argnames_sequence(sequence)
            .map(Into::into)
            .unwrap_or_default()
    }

    fn parse_multiple_argnames_sequence(&self, sequence: &[ast::Expr]) -> Option<MultipleArgnames> {
        // Collect so that all errors are reported.
        let identifiers = sequence
            .iter()
            .map(|element| {
                let range = element.range();
                if let Some(literal_type) = self.expression_type(element).as_string_literal() {
                    let name = literal_type.value(self.db());
                    self.check_valid_identifier(name, range)
                        .then(|| SingleArgname::new(name, range))
                } else {
                    None
                }
            })
            .collect_vec();
        Option::from_iter(identifiers)
    }

    /// Checks whether an individual argname is valid as a Python identifier.
    /// It generates a diagnostic if this is not the case.
    /// `request` is not treated as a valid identifier.
    fn check_valid_identifier(&self, name: &str, range: TextRange) -> bool {
        let is_identifier = is_identifier(name);
        if !is_identifier
            && let Some(builder) = self
                .context
                .report_lint(&PYTEST_INVALID_ARGNAMES_LITERAL, range)
        {
            builder.into_diagnostic(format!("`{name}` is not a valid Python identifier."));
        }
        if name == "request" {
            if let Some(builder) = self.context.report_lint(&PYTEST_REQUEST_KEYWORD, range) {
                builder.into_diagnostic(
                    "`request` is a reserved Pytest keyword and cannot be used during parametrization.",
                );
            }
            return false;
        }
        is_identifier
    }
}
