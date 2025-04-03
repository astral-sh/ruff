use std::collections::BTreeMap;
use std::fmt;
use std::num::{NonZeroU16, NonZeroUsize};
use std::ops::{RangeFrom, RangeInclusive};
use std::str::FromStr;

use ruff_python_ast::PythonVersion;
use rustc_hash::FxHashMap;

use crate::db::Db;
use crate::module_name::ModuleName;
use crate::Program;

pub(in crate::module_resolver) fn vendored_typeshed_versions(db: &dyn Db) -> TypeshedVersions {
    TypeshedVersions::from_str(
        &db.vendored()
            .read_to_string("stdlib/VERSIONS")
            .expect("The vendored typeshed stubs should contain a VERSIONS file"),
    )
    .expect("The VERSIONS file in the vendored typeshed stubs should be well-formed")
}

pub(crate) fn typeshed_versions(db: &dyn Db) -> &TypeshedVersions {
    Program::get(db).search_paths(db).typeshed_versions()
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub(crate) struct TypeshedVersionsParseError {
    line_number: Option<NonZeroU16>,
    reason: TypeshedVersionsParseErrorKind,
}

impl fmt::Display for TypeshedVersionsParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let TypeshedVersionsParseError {
            line_number,
            reason,
        } = self;
        if let Some(line_number) = line_number {
            write!(
                f,
                "Error while parsing line {line_number} of typeshed's VERSIONS file: {reason}"
            )
        } else {
            write!(f, "Error while parsing typeshed's VERSIONS file: {reason}")
        }
    }
}

impl std::error::Error for TypeshedVersionsParseError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        if let TypeshedVersionsParseErrorKind::IntegerParsingFailure { err, .. } = &self.reason {
            Some(err)
        } else {
            None
        }
    }
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub(super) enum TypeshedVersionsParseErrorKind {
    TooManyLines(NonZeroUsize),
    UnexpectedNumberOfColons,
    InvalidModuleName(String),
    UnexpectedNumberOfHyphens,
    UnexpectedNumberOfPeriods(String),
    IntegerParsingFailure {
        version: String,
        err: std::num::ParseIntError,
    },
}

impl fmt::Display for TypeshedVersionsParseErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooManyLines(num_lines) => write!(
                f,
                "File has too many lines ({num_lines}); maximum allowed is {}",
                NonZeroU16::MAX
            ),
            Self::UnexpectedNumberOfColons => {
                f.write_str("Expected every non-comment line to have exactly one colon")
            }
            Self::InvalidModuleName(name) => write!(
                f,
                "Expected all components of '{name}' to be valid Python identifiers"
            ),
            Self::UnexpectedNumberOfHyphens => {
                f.write_str("Expected every non-comment line to have exactly one '-' character")
            }
            Self::UnexpectedNumberOfPeriods(format) => write!(
                f,
                "Expected all versions to be in the form {{MAJOR}}.{{MINOR}}; got '{format}'"
            ),
            Self::IntegerParsingFailure { version, err } => write!(
                f,
                "Failed to convert '{version}' to a pair of integers due to {err}",
            ),
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct TypeshedVersions(FxHashMap<ModuleName, PyVersionRange>);

impl TypeshedVersions {
    #[must_use]
    fn exact(&self, module_name: &ModuleName) -> Option<&PyVersionRange> {
        self.0.get(module_name)
    }

    #[must_use]
    pub(in crate::module_resolver) fn query_module(
        &self,
        module: &ModuleName,
        python_version: PythonVersion,
    ) -> TypeshedVersionsQueryResult {
        if let Some(range) = self.exact(module) {
            if range.contains(python_version) {
                TypeshedVersionsQueryResult::Exists
            } else {
                TypeshedVersionsQueryResult::DoesNotExist
            }
        } else {
            let mut module = module.parent();
            while let Some(module_to_try) = module {
                if let Some(range) = self.exact(&module_to_try) {
                    return {
                        if range.contains(python_version) {
                            TypeshedVersionsQueryResult::MaybeExists
                        } else {
                            TypeshedVersionsQueryResult::DoesNotExist
                        }
                    };
                }
                module = module_to_try.parent();
            }
            TypeshedVersionsQueryResult::DoesNotExist
        }
    }
}

/// Possible answers [`TypeshedVersions::query_module()`] could give to the question:
/// "Does this module exist in the stdlib at runtime on a certain target version?"
#[derive(Debug, Copy, PartialEq, Eq, Clone, Hash)]
pub(crate) enum TypeshedVersionsQueryResult {
    /// The module definitely exists in the stdlib at runtime on the user-specified target version.
    ///
    /// For example:
    /// - The target version is Python 3.8
    /// - We're querying whether the `asyncio.tasks` module exists in the stdlib
    /// - The VERSIONS file contains the line `asyncio.tasks: 3.8-`
    Exists,

    /// The module definitely does not exist in the stdlib on the user-specified target version.
    ///
    /// For example:
    /// - We're querying whether the `foo` module exists in the stdlib
    /// - There is no top-level `foo` module in VERSIONS
    ///
    /// OR:
    /// - The target version is Python 3.8
    /// - We're querying whether the module `importlib.abc` exists in the stdlib
    /// - The VERSIONS file contains the line `importlib.abc: 3.10-`,
    ///   indicating that the module was added in 3.10
    ///
    /// OR:
    /// - The target version is Python 3.8
    /// - We're querying whether the module `collections.abc` exists in the stdlib
    /// - The VERSIONS file does not contain any information about the `collections.abc` submodule,
    ///   but *does* contain the line `collections: 3.10-`,
    ///   indicating that the entire `collections` package was added in Python 3.10.
    DoesNotExist,

    /// The module potentially exists in the stdlib and, if it does,
    /// it definitely exists on the user-specified target version.
    ///
    /// This variant is only relevant for submodules,
    /// for which the typeshed VERSIONS file does not provide comprehensive information.
    /// (The VERSIONS file is guaranteed to provide information about all top-level stdlib modules and packages,
    /// but not necessarily about all submodules within each top-level package.)
    ///
    /// For example:
    /// - The target version is Python 3.8
    /// - We're querying whether the `asyncio.staggered` module exists in the stdlib
    /// - The typeshed VERSIONS file contains the line `asyncio: 3.8`,
    ///   indicating that the `asyncio` package was added in Python 3.8,
    ///   but does not contain any explicit information about the `asyncio.staggered` submodule.
    MaybeExists,
}

impl FromStr for TypeshedVersions {
    type Err = TypeshedVersionsParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut map = FxHashMap::default();

        for (line_index, line) in s.lines().enumerate() {
            // humans expect line numbers to be 1-indexed
            let line_number = NonZeroUsize::new(line_index.saturating_add(1)).unwrap();

            let Ok(line_number) = NonZeroU16::try_from(line_number) else {
                return Err(TypeshedVersionsParseError {
                    line_number: None,
                    reason: TypeshedVersionsParseErrorKind::TooManyLines(line_number),
                });
            };

            let Some(content) = line.split('#').map(str::trim).next() else {
                continue;
            };
            if content.is_empty() {
                continue;
            }

            let mut parts = content.split(':').map(str::trim);
            let (Some(module_name), Some(rest), None) = (parts.next(), parts.next(), parts.next())
            else {
                return Err(TypeshedVersionsParseError {
                    line_number: Some(line_number),
                    reason: TypeshedVersionsParseErrorKind::UnexpectedNumberOfColons,
                });
            };

            let Some(module_name) = ModuleName::new(module_name) else {
                return Err(TypeshedVersionsParseError {
                    line_number: Some(line_number),
                    reason: TypeshedVersionsParseErrorKind::InvalidModuleName(
                        module_name.to_string(),
                    ),
                });
            };

            match PyVersionRange::from_str(rest) {
                Ok(version) => map.insert(module_name, version),
                Err(reason) => {
                    return Err(TypeshedVersionsParseError {
                        line_number: Some(line_number),
                        reason,
                    })
                }
            };
        }

        Ok(Self(map))
    }
}

impl fmt::Display for TypeshedVersions {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let sorted_items: BTreeMap<&ModuleName, &PyVersionRange> = self.0.iter().collect();
        for (module_name, range) in sorted_items {
            writeln!(f, "{module_name}: {range}")?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
enum PyVersionRange {
    AvailableFrom(RangeFrom<PythonVersion>),
    AvailableWithin(RangeInclusive<PythonVersion>),
}

impl PyVersionRange {
    #[must_use]
    fn contains(&self, version: PythonVersion) -> bool {
        match self {
            Self::AvailableFrom(inner) => inner.contains(&version),
            Self::AvailableWithin(inner) => inner.contains(&version),
        }
    }
}

impl FromStr for PyVersionRange {
    type Err = TypeshedVersionsParseErrorKind;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut parts = s.split('-').map(str::trim);
        match (parts.next(), parts.next(), parts.next()) {
            (Some(lower), Some(""), None) => {
                let lower = python_version_from_versions_file_string(lower)?;
                Ok(Self::AvailableFrom(lower..))
            }
            (Some(lower), Some(upper), None) => {
                let lower = python_version_from_versions_file_string(lower)?;
                let upper = python_version_from_versions_file_string(upper)?;
                Ok(Self::AvailableWithin(lower..=upper))
            }
            _ => Err(TypeshedVersionsParseErrorKind::UnexpectedNumberOfHyphens),
        }
    }
}

impl fmt::Display for PyVersionRange {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AvailableFrom(range_from) => write!(f, "{}-", range_from.start),
            Self::AvailableWithin(range_inclusive) => {
                write!(f, "{}-{}", range_inclusive.start(), range_inclusive.end())
            }
        }
    }
}

fn python_version_from_versions_file_string(
    s: &str,
) -> Result<PythonVersion, TypeshedVersionsParseErrorKind> {
    let mut parts = s.split('.').map(str::trim);
    let (Some(major), Some(minor), None) = (parts.next(), parts.next(), parts.next()) else {
        return Err(TypeshedVersionsParseErrorKind::UnexpectedNumberOfPeriods(
            s.to_string(),
        ));
    };
    PythonVersion::try_from((major, minor)).map_err(|int_parse_error| {
        TypeshedVersionsParseErrorKind::IntegerParsingFailure {
            version: s.to_string(),
            err: int_parse_error,
        }
    })
}

#[cfg(test)]
mod tests {
    use std::fmt::Write as _;
    use std::num::{IntErrorKind, NonZeroU16};
    use std::path::Path;

    use insta::assert_snapshot;

    use crate::db::tests::TestDb;

    use super::*;

    const TYPESHED_STDLIB_DIR: &str = "stdlib";

    const ONE: Option<NonZeroU16> = Some(NonZeroU16::new(1).unwrap());

    impl TypeshedVersions {
        #[must_use]
        fn contains_exact(&self, module: &ModuleName) -> bool {
            self.exact(module).is_some()
        }

        #[must_use]
        fn len(&self) -> usize {
            self.0.len()
        }
    }

    #[test]
    fn can_parse_vendored_versions_file() {
        let db = TestDb::new();

        let versions = vendored_typeshed_versions(&db);
        assert!(versions.len() > 100);
        assert!(versions.len() < 1000);

        let asyncio = ModuleName::new_static("asyncio").unwrap();
        let asyncio_staggered = ModuleName::new_static("asyncio.staggered").unwrap();
        let audioop = ModuleName::new_static("audioop").unwrap();

        assert!(versions.contains_exact(&asyncio));
        assert_eq!(
            versions.query_module(&asyncio, PythonVersion::PY310),
            TypeshedVersionsQueryResult::Exists
        );

        assert!(versions.contains_exact(&asyncio_staggered));
        assert_eq!(
            versions.query_module(&asyncio_staggered, PythonVersion::PY38),
            TypeshedVersionsQueryResult::Exists
        );
        assert_eq!(
            versions.query_module(&asyncio_staggered, PythonVersion::PY37),
            TypeshedVersionsQueryResult::DoesNotExist
        );

        assert!(versions.contains_exact(&audioop));
        assert_eq!(
            versions.query_module(&audioop, PythonVersion::PY312),
            TypeshedVersionsQueryResult::Exists
        );
        assert_eq!(
            versions.query_module(&audioop, PythonVersion::PY313),
            TypeshedVersionsQueryResult::DoesNotExist
        );
    }

    #[test]
    fn typeshed_versions_consistent_with_vendored_stubs() {
        let db = TestDb::new();
        let vendored_typeshed_versions = vendored_typeshed_versions(&db);
        let vendored_typeshed_dir =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../red_knot_vendored/vendor/typeshed");

        let mut empty_iterator = true;

        let stdlib_stubs_path = vendored_typeshed_dir.join(TYPESHED_STDLIB_DIR);

        for entry in std::fs::read_dir(&stdlib_stubs_path).unwrap() {
            empty_iterator = false;
            let entry = entry.unwrap();
            let absolute_path = entry.path();

            let relative_path = absolute_path
                .strip_prefix(&stdlib_stubs_path)
                .unwrap_or_else(|_| panic!("Expected path to be a child of {stdlib_stubs_path:?} but found {absolute_path:?}"));

            let relative_path_str = relative_path.as_os_str().to_str().unwrap_or_else(|| {
                panic!("Expected all typeshed paths to be valid UTF-8; got {relative_path:?}")
            });
            if relative_path_str == "VERSIONS" {
                continue;
            }

            let top_level_module = if let Some(extension) = relative_path.extension() {
                // It was a file; strip off the file extension to get the module name:
                let extension = extension
                    .to_str()
                    .unwrap_or_else(||panic!("Expected all file extensions to be UTF-8; was not true for {relative_path:?}"));

                relative_path_str
                    .strip_suffix(extension)
                    .and_then(|string| string.strip_suffix('.')).unwrap_or_else(|| {
                        panic!("Expected path {relative_path_str:?} to end with computed extension {extension:?}")
                })
            } else {
                // It was a directory; no need to do anything to get the module name
                relative_path_str
            };

            let top_level_module = ModuleName::new(top_level_module)
                .unwrap_or_else(|| panic!("{top_level_module:?} was not a valid module name!"));

            assert!(vendored_typeshed_versions.contains_exact(&top_level_module));
        }

        assert!(
            !empty_iterator,
            "Expected there to be at least one file or directory in the vendored typeshed stubs"
        );
    }

    #[test]
    fn can_parse_mock_versions_file() {
        const VERSIONS: &str = "\
# a comment
    # some more comment
# yet more comment


# and some more comment

bar: 2.7-3.10

# more comment
bar.baz: 3.1-3.9
foo: 3.8-   # trailing comment
";
        let parsed_versions = TypeshedVersions::from_str(VERSIONS).unwrap();
        assert_eq!(parsed_versions.len(), 3);
        assert_snapshot!(parsed_versions.to_string(), @r"
        bar: 2.7-3.10
        bar.baz: 3.1-3.9
        foo: 3.8-
        "
        );
    }

    #[test]
    fn version_within_range_parsed_correctly() {
        let parsed_versions = TypeshedVersions::from_str("bar: 2.7-3.10").unwrap();
        let bar = ModuleName::new_static("bar").unwrap();

        assert!(parsed_versions.contains_exact(&bar));
        assert_eq!(
            parsed_versions.query_module(&bar, PythonVersion::PY37),
            TypeshedVersionsQueryResult::Exists
        );
        assert_eq!(
            parsed_versions.query_module(&bar, PythonVersion::PY310),
            TypeshedVersionsQueryResult::Exists
        );
        assert_eq!(
            parsed_versions.query_module(&bar, PythonVersion::PY311),
            TypeshedVersionsQueryResult::DoesNotExist
        );
    }

    #[test]
    fn version_from_range_parsed_correctly() {
        let parsed_versions = TypeshedVersions::from_str("foo: 3.8-").unwrap();
        let foo = ModuleName::new_static("foo").unwrap();

        assert!(parsed_versions.contains_exact(&foo));
        assert_eq!(
            parsed_versions.query_module(&foo, PythonVersion::PY37),
            TypeshedVersionsQueryResult::DoesNotExist
        );
        assert_eq!(
            parsed_versions.query_module(&foo, PythonVersion::PY38),
            TypeshedVersionsQueryResult::Exists
        );
        assert_eq!(
            parsed_versions.query_module(&foo, PythonVersion::PY311),
            TypeshedVersionsQueryResult::Exists
        );
    }

    #[test]
    fn explicit_submodule_parsed_correctly() {
        let parsed_versions = TypeshedVersions::from_str("bar.baz: 3.1-3.9").unwrap();
        let bar_baz = ModuleName::new_static("bar.baz").unwrap();

        assert!(parsed_versions.contains_exact(&bar_baz));
        assert_eq!(
            parsed_versions.query_module(&bar_baz, PythonVersion::PY37),
            TypeshedVersionsQueryResult::Exists
        );
        assert_eq!(
            parsed_versions.query_module(&bar_baz, PythonVersion::PY39),
            TypeshedVersionsQueryResult::Exists
        );
        assert_eq!(
            parsed_versions.query_module(&bar_baz, PythonVersion::PY310),
            TypeshedVersionsQueryResult::DoesNotExist
        );
    }

    #[test]
    fn implicit_submodule_queried_correctly() {
        let parsed_versions = TypeshedVersions::from_str("bar: 2.7-3.10").unwrap();
        let bar_eggs = ModuleName::new_static("bar.eggs").unwrap();

        assert!(!parsed_versions.contains_exact(&bar_eggs));
        assert_eq!(
            parsed_versions.query_module(&bar_eggs, PythonVersion::PY37),
            TypeshedVersionsQueryResult::MaybeExists
        );
        assert_eq!(
            parsed_versions.query_module(&bar_eggs, PythonVersion::PY310),
            TypeshedVersionsQueryResult::MaybeExists
        );
        assert_eq!(
            parsed_versions.query_module(&bar_eggs, PythonVersion::PY311),
            TypeshedVersionsQueryResult::DoesNotExist
        );
    }

    #[test]
    fn nonexistent_module_queried_correctly() {
        let parsed_versions = TypeshedVersions::from_str("eggs: 3.8-").unwrap();
        let spam = ModuleName::new_static("spam").unwrap();

        assert!(!parsed_versions.contains_exact(&spam));
        assert_eq!(
            parsed_versions.query_module(&spam, PythonVersion::PY37),
            TypeshedVersionsQueryResult::DoesNotExist
        );
        assert_eq!(
            parsed_versions.query_module(&spam, PythonVersion::PY313),
            TypeshedVersionsQueryResult::DoesNotExist
        );
    }

    #[test]
    fn invalid_huge_versions_file() {
        let offset = 100;
        let too_many = u16::MAX as usize + offset;

        let mut massive_versions_file = String::new();
        for i in 0..too_many {
            let _ = writeln!(&mut massive_versions_file, "x{i}: 3.8-");
        }

        assert_eq!(
            TypeshedVersions::from_str(&massive_versions_file),
            Err(TypeshedVersionsParseError {
                line_number: None,
                reason: TypeshedVersionsParseErrorKind::TooManyLines(
                    NonZeroUsize::new(too_many + 1 - offset).unwrap()
                )
            })
        );
    }

    #[test]
    fn invalid_typeshed_versions_bad_colon_number() {
        assert_eq!(
            TypeshedVersions::from_str("foo 3.7"),
            Err(TypeshedVersionsParseError {
                line_number: ONE,
                reason: TypeshedVersionsParseErrorKind::UnexpectedNumberOfColons
            })
        );
        assert_eq!(
            TypeshedVersions::from_str("foo:: 3.7"),
            Err(TypeshedVersionsParseError {
                line_number: ONE,
                reason: TypeshedVersionsParseErrorKind::UnexpectedNumberOfColons
            })
        );
    }

    #[test]
    fn invalid_typeshed_versions_non_identifier_modules() {
        assert_eq!(
            TypeshedVersions::from_str("not!an!identifier!: 3.7"),
            Err(TypeshedVersionsParseError {
                line_number: ONE,
                reason: TypeshedVersionsParseErrorKind::InvalidModuleName(
                    "not!an!identifier!".to_string()
                )
            })
        );
        assert_eq!(
            TypeshedVersions::from_str("(also_not).(an_identifier): 3.7"),
            Err(TypeshedVersionsParseError {
                line_number: ONE,
                reason: TypeshedVersionsParseErrorKind::InvalidModuleName(
                    "(also_not).(an_identifier)".to_string()
                )
            })
        );
    }

    #[test]
    fn invalid_typeshed_versions_bad_hyphen_number() {
        assert_eq!(
            TypeshedVersions::from_str("foo: 3.8"),
            Err(TypeshedVersionsParseError {
                line_number: ONE,
                reason: TypeshedVersionsParseErrorKind::UnexpectedNumberOfHyphens
            })
        );
        assert_eq!(
            TypeshedVersions::from_str("foo: 3.8--"),
            Err(TypeshedVersionsParseError {
                line_number: ONE,
                reason: TypeshedVersionsParseErrorKind::UnexpectedNumberOfHyphens
            })
        );
        assert_eq!(
            TypeshedVersions::from_str("foo: 3.8--3.9"),
            Err(TypeshedVersionsParseError {
                line_number: ONE,
                reason: TypeshedVersionsParseErrorKind::UnexpectedNumberOfHyphens
            })
        );
    }

    #[test]
    fn invalid_typeshed_versions_bad_period_number() {
        assert_eq!(
            TypeshedVersions::from_str("foo: 38-"),
            Err(TypeshedVersionsParseError {
                line_number: ONE,
                reason: TypeshedVersionsParseErrorKind::UnexpectedNumberOfPeriods("38".to_string())
            })
        );
        assert_eq!(
            TypeshedVersions::from_str("foo: 3..8-"),
            Err(TypeshedVersionsParseError {
                line_number: ONE,
                reason: TypeshedVersionsParseErrorKind::UnexpectedNumberOfPeriods(
                    "3..8".to_string()
                )
            })
        );
        assert_eq!(
            TypeshedVersions::from_str("foo: 3.8-3..11"),
            Err(TypeshedVersionsParseError {
                line_number: ONE,
                reason: TypeshedVersionsParseErrorKind::UnexpectedNumberOfPeriods(
                    "3..11".to_string()
                )
            })
        );
    }

    #[test]
    fn invalid_typeshed_versions_non_digits() {
        let err = TypeshedVersions::from_str("foo: 1.two-").unwrap_err();
        assert_eq!(err.line_number, ONE);
        let TypeshedVersionsParseErrorKind::IntegerParsingFailure { version, err } = err.reason
        else {
            panic!()
        };
        assert_eq!(version, "1.two".to_string());
        assert_eq!(*err.kind(), IntErrorKind::InvalidDigit);

        let err = TypeshedVersions::from_str("foo: 3.8-four.9").unwrap_err();
        assert_eq!(err.line_number, ONE);
        let TypeshedVersionsParseErrorKind::IntegerParsingFailure { version, err } = err.reason
        else {
            panic!()
        };
        assert_eq!(version, "four.9".to_string());
        assert_eq!(*err.kind(), IntErrorKind::InvalidDigit);
    }
}
