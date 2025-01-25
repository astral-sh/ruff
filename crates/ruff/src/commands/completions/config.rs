use clap::builder::{PossibleValue, TypedValueParser, ValueParserFactory};
use itertools::Itertools;
use std::str::FromStr;

use ruff_workspace::{
    options::Options,
    options_base::{OptionField, OptionSet, OptionsMetadata, Visit},
};

#[derive(Default)]
struct CollectOptionsVisitor {
    values: Vec<(String, String)>,
    parents: Vec<String>,
}

impl IntoIterator for CollectOptionsVisitor {
    type Item = (String, String);
    type IntoIter = std::vec::IntoIter<Self::Item>;

    fn into_iter(self) -> Self::IntoIter {
        self.values.into_iter()
    }
}

impl Visit for CollectOptionsVisitor {
    fn record_set(&mut self, name: &str, group: OptionSet) {
        let fully_qualified_name = self
            .parents
            .iter()
            .map(String::as_str)
            .chain(std::iter::once(name))
            .collect::<Vec<_>>()
            .join(".");

        // Only add the set to completion list if it has it's own documentation.
        self.values.push((
            fully_qualified_name,
            group.documentation().unwrap_or("").to_owned(),
        ));

        self.parents.push(name.to_owned());
        group.record(self);
        self.parents.pop();
    }

    fn record_field(&mut self, name: &str, field: OptionField) {
        let fqn = self
            .parents
            .iter()
            .map(String::as_str)
            .chain(std::iter::once(name))
            .collect::<Vec<_>>()
            .join(".");

        self.values.push((fqn, field.doc.to_owned()));
    }
}

/// Opaque type used solely to enable tab completions
/// for `ruff option [OPTION]` command.
#[derive(Clone, Debug)]
pub struct OptionString(String);

impl From<String> for OptionString {
    fn from(s: String) -> Self {
        OptionString(s)
    }
}

impl From<OptionString> for String {
    fn from(value: OptionString) -> Self {
        value.0
    }
}

impl From<&str> for OptionString {
    fn from(s: &str) -> Self {
        OptionString(s.to_string())
    }
}

impl std::ops::Deref for OptionString {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl FromStr for OptionString {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Options::metadata()
            .has(s)
            .then(|| OptionString(s.to_owned()))
            .ok_or(())
    }
}

#[derive(Clone)]
pub struct OptionStringParser;

impl ValueParserFactory for OptionString {
    type Parser = OptionStringParser;

    fn value_parser() -> Self::Parser {
        OptionStringParser
    }
}

impl TypedValueParser for OptionStringParser {
    type Value = OptionString;

    fn parse_ref(
        &self,
        cmd: &clap::Command,
        arg: Option<&clap::Arg>,
        value: &std::ffi::OsStr,
    ) -> Result<Self::Value, clap::Error> {
        let value = value
            .to_str()
            .ok_or_else(|| clap::Error::new(clap::error::ErrorKind::InvalidUtf8))?;

        value.parse().map_err(|()| {
            let mut error = clap::Error::new(clap::error::ErrorKind::ValueValidation).with_cmd(cmd);
            if let Some(arg) = arg {
                error.insert(
                    clap::error::ContextKind::InvalidArg,
                    clap::error::ContextValue::String(arg.to_string()),
                );
            }
            error.insert(
                clap::error::ContextKind::InvalidValue,
                clap::error::ContextValue::String(value.to_string()),
            );
            error
        })
    }

    fn possible_values(&self) -> Option<Box<dyn Iterator<Item = PossibleValue> + '_>> {
        let mut visitor = CollectOptionsVisitor::default();
        Options::metadata().record(&mut visitor);

        Some(Box::new(visitor.into_iter().map(|(name, doc)| {
            let first_paragraph = doc
                .lines()
                .take_while(|line| !line.trim_end().is_empty())
                // Replace double quotes with single quotes,to avoid clap's lack of escaping
                // when creating zsh completions. This has no security implications, as it only
                // affects the help string, which is never executed
                .map(|s| s.replace('"', "'"))
                .join(" ");

            PossibleValue::new(name).help(first_paragraph)
        })))
    }
}
