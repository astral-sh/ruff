use std::convert;

use derive_more::{Constructor, From};
use itertools::Itertools;
use ruff_python_ast::{self as ast};
use ruff_python_stdlib::identifiers::is_identifier;
use ruff_text_size::{Ranged, TextRange};

use crate::types::{
    diagnostic::{PYTEST_INVALID_ARGNAMES_LITERAL, PYTEST_REQUEST_KEYWORD},
    infer::TypeInferenceBuilder,
};

#[derive(Debug, From, Constructor)]
pub(crate) struct SingleArgname {
    argname: String,
}

// Even though it is called multiple, it make contain a single argname.
// This occurs when argnames passed as a sequence. For example,
// ```python
// @pytest.mark.parametrize(["name"], [("first",), ("second",)])
// def test_name(name: tuple[str]) -> None: ...
//
// ```
#[derive(Debug, From, Constructor)]
pub(crate) struct MultipleArgnames {
    argnames: Vec<String>,
}

#[derive(Debug, From)]
pub(crate) enum KnownArgnames {
    Single(SingleArgname),
    Multiple(MultipleArgnames),
}

#[derive(Debug)]
pub(crate) enum Argnames {
    Known(KnownArgnames),
    Unknown,
}

impl From<&str> for Argnames {
    fn from(value: &str) -> Self {
        SingleArgname::from(value.to_owned()).into()
    }
}

impl From<Vec<&str>> for Argnames {
    fn from(value: Vec<&str>) -> Self {
        MultipleArgnames::from(value.into_iter().map(ToOwned::to_owned).collect_vec()).into()
    }
}

impl<T: Into<KnownArgnames>> From<T> for Argnames {
    fn from(value: T) -> Self {
        Self::Known(value.into())
    }
}

impl TypeInferenceBuilder<'_, '_> {
    pub(crate) fn parse_argnames_expression(&self, argnames_argument: &ast::Expr) -> Argnames {
        if let Some(literal) = self.expression_type(argnames_argument).as_string_literal() {
            self.parse_argnames_string(literal.value(self.db()), argnames_argument.range())
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
            [name] => (*name).into(),
            _ => filtered_names.into(),
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
        if name == "request"
            && let Some(builder) = self.context.report_lint(&PYTEST_REQUEST_KEYWORD, range)
        {
            builder.into_diagnostic(
                "`request` is a reserved Python keyword and cannot be used during parametrization.",
            );
        }
        !is_identifier
    }
}
