use derive_more::{Constructor, From, IntoIterator};
use itertools::Itertools;
use ruff_python_ast as ast;
use ruff_text_size::TextRange;
use std::iter::FromIterator;
use std::slice;

use crate::types::{
    dedicated::pytest_argnames::{
        Argname, Argnames, MultipleArgnames, SequenceArgnames, SingleArgname, StringLiteralArgnames,
    },
    diagnostic::{PYTEST_INVALID_ARGNAMES_LITERAL, PYTEST_REQUEST_KEYWORD},
    infer::TypeInferenceBuilder,
};

#[derive(Debug)]
pub(crate) struct SingleKnownArgname {
    name: String,
    range: TextRange,
}

impl SingleKnownArgname {
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
pub(crate) struct MultipleKnownArgnames {
    argnames: Vec<SingleKnownArgname>,
}

impl MultipleKnownArgnames {
    pub(crate) fn iter(&self) -> <&Self as IntoIterator>::IntoIter {
        self.into_iter()
    }
}

impl FromIterator<SingleKnownArgname> for MultipleKnownArgnames {
    fn from_iter<I: IntoIterator<Item = SingleKnownArgname>>(iter: I) -> Self {
        Self::new(Vec::from_iter(iter))
    }
}

#[derive(Debug, From)]
pub(crate) enum KnownArgnames {
    Single(SingleKnownArgname),
    Multiple(MultipleKnownArgnames),
}

impl<'a> IntoIterator for &'a KnownArgnames {
    type Item = &'a SingleKnownArgname;

    type IntoIter = slice::Iter<'a, SingleKnownArgname>;

    fn into_iter(self) -> Self::IntoIter {
        match self {
            KnownArgnames::Single(argname) => slice::from_ref(argname).iter(),
            KnownArgnames::Multiple(argnames) => argnames.iter(),
        }
    }
}

// todo: could take context
impl TypeInferenceBuilder<'_, '_> {
    pub(crate) fn parse_argnames_expression(&self, argnames: &ast::Expr) -> Option<KnownArgnames> {
        let argnames = Argnames::from_expr(argnames);
        self.convert_and_check_argnames(argnames)
    }

    /// Convert argnames from the model (which may have errors) into `KnownArgnames` if possible.
    /// While doing this, any errors from invalid argnames are raised.
    /// If `None` is returned, it may be because of an error or because the argnames are unknown.
    pub(crate) fn convert_and_check_argnames(&self, argnames: Argnames) -> Option<KnownArgnames> {
        match &argnames {
            Argnames::StringLiteral(StringLiteralArgnames::Single(argname)) => {
                Some(self.convert_and_check_single_argname(argname)?.into())
            }
            Argnames::StringLiteral(StringLiteralArgnames::Multiple(argnames))
            | Argnames::Sequence(SequenceArgnames::Multiple(argnames)) => {
                Some(self.convert_and_check_multiple_argnames(argnames)?.into())
            }
            Argnames::Unknown => None,
        }
    }

    fn convert_and_check_multiple_argnames(
        &self,
        argnames: &MultipleArgnames,
    ) -> Option<MultipleKnownArgnames> {
        // Collect to Vec to process all diagnostics.
        let converted_argnames = argnames
            .iter()
            .map(|argname| self.convert_and_check_single_argname(argname))
            .collect_vec();
        FromIterator::from_iter(converted_argnames)
    }

    fn convert_and_check_single_argname(
        &self,
        argname: &SingleArgname,
    ) -> Option<SingleKnownArgname> {
        let range = argname.range;
        match &argname.argname {
            Argname::Valid(argname) => Some(SingleKnownArgname::new(argname, range)),
            Argname::Request => {
                self.generate_request_keyword_diagnostic(range);
                None
            }
            Argname::Error(invalid_argname) => {
                self.generate_invalid_argname_diagnostic(invalid_argname, range);
                None
            }
            Argname::Unknown => None,
        }
    }

    fn generate_request_keyword_diagnostic(&self, range: TextRange) {
        if let Some(builder) = self.context.report_lint(&PYTEST_REQUEST_KEYWORD, range) {
            builder.into_diagnostic(
                "`request` is a reserved Pytest keyword and cannot be used during parametrization.",
            );
        }
    }

    fn generate_invalid_argname_diagnostic(&self, name: &str, range: TextRange) {
        if let Some(builder) = self
            .context
            .report_lint(&PYTEST_INVALID_ARGNAMES_LITERAL, range)
        {
            builder.into_diagnostic(format!("`{name}` is not a valid Python identifier."));
        }
    }
}
