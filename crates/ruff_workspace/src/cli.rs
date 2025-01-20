#[cfg(feature = "clap")]
pub mod clap_completion {
    use clap::builder::{PossibleValue, TypedValueParser, ValueParserFactory};
    use std::str::FromStr;

    use crate::{
        options::Options,
        options_base::{OptionField, OptionSet, OptionsMetadata, Visit},
    };

    #[derive(Default)]
    struct CollectOptionsVisitor {
        values: Vec<(String, String)>,
        parents: Vec<String>,
    }

    impl IntoIterator for CollectOptionsVisitor {
        type IntoIter = std::vec::IntoIter<(String, String)>;
        type Item = (String, String);

        fn into_iter(self) -> Self::IntoIter {
            self.values.into_iter()
        }
    }

    impl Visit for CollectOptionsVisitor {
        fn record_set(&mut self, name: &str, group: OptionSet) {
            let fqn = self
                .parents
                .iter()
                .map(String::as_str)
                .chain(std::iter::once(name))
                .collect::<Vec<_>>()
                .join(".");

            self.values
                .push((fqn, group.documentation().unwrap_or_default().to_owned()));

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

    /// Opaque type for option strings on the command line.
    #[derive(Clone, Debug)]
    pub struct OptionString(String);

    impl From<String> for OptionString {
        fn from(s: String) -> Self {
            OptionString(s)
        }
    }

    impl From<OptionString> for String {
        fn from(o: OptionString) -> Self {
            o.0
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
                let mut error =
                    clap::Error::new(clap::error::ErrorKind::ValueValidation).with_cmd(cmd);
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
                let first_line = doc
                    .lines()
                    .next()
                    .unwrap_or("")
                    .chars()
                    .take(80)
                    .collect::<String>();

                PossibleValue::new(name).help(first_line)
            })))
        }
    }
}
