use itertools::Itertools;
use ruff_python_ast::{Alias, Stmt};

use ruff_diagnostics::{AutofixKind, Diagnostic, Edit, Fix, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::whitespace::indentation;
use ruff_python_codegen::Stylist;
use ruff_source_file::Locator;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::registry::Rule;
use crate::rules::pyupgrade::fixes;
use crate::settings::types::PythonVersion;

/// An import was moved and renamed as part of a deprecation.
/// For example, `typing.AbstractSet` was moved to `collections.abc.Set`.
#[derive(Debug, PartialEq, Eq)]
struct WithRename {
    module: String,
    member: String,
    target: String,
}

/// A series of imports from the same module were moved to another module,
/// but retain their original names.
#[derive(Debug, PartialEq, Eq)]
struct WithoutRename {
    target: String,
    members: Vec<String>,
    fixable: bool,
}

#[derive(Debug, PartialEq, Eq)]
enum Deprecation {
    WithRename(WithRename),
    WithoutRename(WithoutRename),
}

/// ## What it does
/// Checks for uses of deprecated imports based on the minimum supported
/// Python version.
///
/// ## Why is this bad?
/// Deprecated imports may be removed in future versions of Python, and
/// should be replaced with their new equivalents.
///
/// Note that, in some cases, it may be preferable to continue importing
/// members from `typing_extensions` even after they're added to the Python
/// standard library, as `typing_extensions` can backport bugfixes and
/// optimizations from later Python versions. This rule thus avoids flagging
/// imports from `typing_extensions` in such cases.
///
/// ## Example
/// ```python
/// from collections import Sequence
/// ```
///
/// Use instead:
/// ```python
/// from collections.abc import Sequence
/// ```
#[violation]
pub struct DeprecatedImport {
    deprecation: Deprecation,
}

impl Violation for DeprecatedImport {
    const AUTOFIX: AutofixKind = AutofixKind::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        match &self.deprecation {
            Deprecation::WithoutRename(WithoutRename {
                members, target, ..
            }) => {
                let names = members.iter().map(|name| format!("`{name}`")).join(", ");
                format!("Import from `{target}` instead: {names}")
            }
            Deprecation::WithRename(WithRename {
                module,
                member,
                target,
            }) => {
                format!("`{module}.{member}` is deprecated, use `{target}` instead")
            }
        }
    }

    fn autofix_title(&self) -> Option<String> {
        if let Deprecation::WithoutRename(WithoutRename { target, .. }) = &self.deprecation {
            Some(format!("Import from `{target}`"))
        } else {
            None
        }
    }
}

/// Returns `true` if the module may contain deprecated imports.
fn is_relevant_module(module: &str) -> bool {
    matches!(
        module,
        "collections" | "pipes" | "mypy_extensions" | "typing_extensions" | "typing" | "typing.re"
    )
}

// Members of `collections` that were moved to `collections.abc`.
const COLLECTIONS_TO_ABC: &[&str] = &[
    "AsyncGenerator",
    "AsyncIterable",
    "AsyncIterator",
    "Awaitable",
    "ByteString",
    "Callable",
    "Collection",
    "Container",
    "Coroutine",
    "Generator",
    "Hashable",
    "ItemsView",
    "Iterable",
    "Iterator",
    "KeysView",
    "Mapping",
    "MappingView",
    "MutableMapping",
    "MutableSequence",
    "MutableSet",
    "Reversible",
    "Sequence",
    "Set",
    "Sized",
    "ValuesView",
];

// Members of `pipes` that were moved to `shlex`.
const PIPES_TO_SHLEX: &[&str] = &["quote"];

// Members of `typing_extensions` that were moved to `typing`.
const TYPING_EXTENSIONS_TO_TYPING: &[&str] = &[
    "AbstractSet",
    "AnyStr",
    "AsyncIterable",
    "AsyncIterator",
    "Awaitable",
    "BinaryIO",
    "Callable",
    "ClassVar",
    "Collection",
    "Container",
    "ContextManager",
    "Coroutine",
    "DefaultDict",
    "Dict",
    "FrozenSet",
    "Generator",
    "Generic",
    "Hashable",
    "IO",
    "ItemsView",
    "Iterable",
    "Iterator",
    "KeysView",
    "List",
    "Mapping",
    "MappingView",
    "Match",
    "MutableMapping",
    "MutableSequence",
    "MutableSet",
    "Optional",
    "Pattern",
    "Reversible",
    "Sequence",
    "Set",
    "Sized",
    "TYPE_CHECKING",
    "Text",
    "TextIO",
    "Tuple",
    "Type",
    "Union",
    "ValuesView",
    "cast",
    "no_type_check",
    "no_type_check_decorator",
    // Introduced in Python 3.5.2, but `typing_extensions` contains backported bugfixes and
    // optimizations,
    // "NewType",
];

// Python 3.7+

// Members of `mypy_extensions` that were moved to `typing`.
const MYPY_EXTENSIONS_TO_TYPING_37: &[&str] = &["NoReturn"];

// Members of `typing_extensions` that were moved to `typing`.
const TYPING_EXTENSIONS_TO_TYPING_37: &[&str] = &[
    "AsyncContextManager",
    "AsyncGenerator",
    "ChainMap",
    "Counter",
    "Deque",
    "ForwardRef",
    "NoReturn",
];

// Python 3.8+

// Members of `mypy_extensions` that were moved to `typing`.
const MYPY_EXTENSIONS_TO_TYPING_38: &[&str] = &["TypedDict"];

// Members of `typing_extensions` that were moved to `typing`.
const TYPING_EXTENSIONS_TO_TYPING_38: &[&str] = &[
    "Final",
    "OrderedDict",
    "runtime_checkable",
    // Introduced in Python 3.8, but `typing_extensions` contains backported bugfixes and
    // optimizations.
    // "Literal",
    // "Protocol",
    // "SupportsIndex",
];

// Python 3.9+

// Members of `typing` that were moved to `collections.abc`.
const TYPING_TO_COLLECTIONS_ABC_39: &[&str] = &[
    "AsyncGenerator",
    "AsyncIterable",
    "AsyncIterator",
    "Awaitable",
    "ByteString",
    "Collection",
    "Container",
    "Coroutine",
    "Generator",
    "Hashable",
    "ItemsView",
    "Iterable",
    "Iterator",
    "KeysView",
    "Mapping",
    "MappingView",
    "MutableMapping",
    "MutableSequence",
    "MutableSet",
    "Reversible",
    "Sequence",
    "Sized",
    "ValuesView",
];

// Members of `typing` that were moved to `collections`.
const TYPING_TO_COLLECTIONS_39: &[&str] = &["ChainMap", "Counter", "OrderedDict"];

// Members of `typing` that were moved to `typing.re`.
const TYPING_TO_RE_39: &[&str] = &["Match", "Pattern"];

// Members of `typing.re` that were moved to `re`.
const TYPING_RE_TO_RE_39: &[&str] = &["Match", "Pattern"];

// Members of `typing_extensions` that were moved to `typing`.
const TYPING_EXTENSIONS_TO_TYPING_39: &[&str] = &["Annotated", "get_type_hints"];

// Members of `typing` that were moved _and_ renamed (and thus cannot be
// automatically fixed).
const TYPING_TO_RENAME_PY39: &[(&str, &str)] = &[
    (
        "AsyncContextManager",
        "contextlib.AbstractAsyncContextManager",
    ),
    ("ContextManager", "contextlib.AbstractContextManager"),
    ("AbstractSet", "collections.abc.Set"),
    ("Tuple", "tuple"),
    ("List", "list"),
    ("FrozenSet", "frozenset"),
    ("Dict", "dict"),
    ("Type", "type"),
    ("Set", "set"),
    ("Deque", "collections.deque"),
    ("DefaultDict", "collections.defaultdict"),
];

// Python 3.10+

// Members of `typing` that were moved to `collections.abc`.
const TYPING_TO_COLLECTIONS_ABC_310: &[&str] = &["Callable"];

// Members of `typing_extensions` that were moved to `typing`.
const TYPING_EXTENSIONS_TO_TYPING_310: &[&str] = &[
    "Concatenate",
    "Literal",
    "NewType",
    "ParamSpecArgs",
    "ParamSpecKwargs",
    "TypeAlias",
    "TypeGuard",
    "get_args",
    "get_origin",
    "is_typeddict",
];

// Python 3.11+

// Members of `typing_extensions` that were moved to `typing`.
const TYPING_EXTENSIONS_TO_TYPING_311: &[&str] = &[
    "Any",
    "LiteralString",
    "Never",
    "NotRequired",
    "Required",
    "Self",
    "assert_never",
    "assert_type",
    "clear_overloads",
    "final",
    "get_overloads",
    "overload",
    "reveal_type",
];

// Python 3.12+

// Members of `typing_extensions` that were moved to `typing`.
const TYPING_EXTENSIONS_TO_TYPING_312: &[&str] = &[
    "NamedTuple",
    // Introduced in Python 3.8, but `typing_extensions` backports a ton of optimizations that were
    // added in Python 3.12.
    "Protocol",
    "SupportsAbs",
    "SupportsBytes",
    "SupportsComplex",
    "SupportsFloat",
    "SupportsInt",
    "SupportsRound",
    "TypedDict",
    "Unpack",
    // Introduced in Python 3.11, but `typing_extensions` backports the `frozen_default` argument,
    // which was introduced in Python 3.12.
    "dataclass_transform",
];

struct ImportReplacer<'a> {
    stmt: &'a Stmt,
    module: &'a str,
    members: &'a [Alias],
    locator: &'a Locator<'a>,
    stylist: &'a Stylist<'a>,
    version: PythonVersion,
}

impl<'a> ImportReplacer<'a> {
    const fn new(
        stmt: &'a Stmt,
        module: &'a str,
        members: &'a [Alias],
        locator: &'a Locator<'a>,
        stylist: &'a Stylist<'a>,
        version: PythonVersion,
    ) -> Self {
        Self {
            stmt,
            module,
            members,
            locator,
            stylist,
            version,
        }
    }

    /// Return a list of deprecated imports whose members were renamed.
    fn with_renames(&self) -> Vec<WithRename> {
        let mut operations = vec![];
        if self.module == "typing" {
            if self.version >= PythonVersion::Py39 {
                for member in self.members {
                    if let Some(target) = TYPING_TO_RENAME_PY39.iter().find_map(|(name, target)| {
                        if &member.name == *name {
                            Some(*target)
                        } else {
                            None
                        }
                    }) {
                        operations.push(WithRename {
                            module: "typing".to_string(),
                            member: member.name.to_string(),
                            target: target.to_string(),
                        });
                    }
                }
            }
        }
        operations
    }

    /// Return a list of deprecated imports whose members were moved, but not renamed.
    fn without_renames(&self) -> Vec<(WithoutRename, Option<String>)> {
        let mut operations = vec![];
        match self.module {
            "collections" => {
                if let Some(operation) = self.try_replace(COLLECTIONS_TO_ABC, "collections.abc") {
                    operations.push(operation);
                }
            }
            "pipes" => {
                if let Some(operation) = self.try_replace(PIPES_TO_SHLEX, "shlex") {
                    operations.push(operation);
                }
            }
            "typing_extensions" => {
                let mut typing_extensions_to_typing = TYPING_EXTENSIONS_TO_TYPING.to_vec();
                if self.version >= PythonVersion::Py37 {
                    typing_extensions_to_typing.extend(TYPING_EXTENSIONS_TO_TYPING_37);
                }
                if self.version >= PythonVersion::Py38 {
                    typing_extensions_to_typing.extend(TYPING_EXTENSIONS_TO_TYPING_38);
                }
                if self.version >= PythonVersion::Py39 {
                    typing_extensions_to_typing.extend(TYPING_EXTENSIONS_TO_TYPING_39);
                }
                if self.version >= PythonVersion::Py310 {
                    typing_extensions_to_typing.extend(TYPING_EXTENSIONS_TO_TYPING_310);
                }
                if self.version >= PythonVersion::Py311 {
                    typing_extensions_to_typing.extend(TYPING_EXTENSIONS_TO_TYPING_311);
                }
                if self.version >= PythonVersion::Py312 {
                    typing_extensions_to_typing.extend(TYPING_EXTENSIONS_TO_TYPING_312);
                }
                if let Some(operation) = self.try_replace(&typing_extensions_to_typing, "typing") {
                    operations.push(operation);
                }
            }
            "mypy_extensions" => {
                let mut mypy_extensions_to_typing = vec![];
                if self.version >= PythonVersion::Py37 {
                    mypy_extensions_to_typing.extend(MYPY_EXTENSIONS_TO_TYPING_37);
                }
                if self.version >= PythonVersion::Py38 {
                    mypy_extensions_to_typing.extend(MYPY_EXTENSIONS_TO_TYPING_38);
                }
                if let Some(operation) = self.try_replace(&mypy_extensions_to_typing, "typing") {
                    operations.push(operation);
                }
            }
            "typing" => {
                // `typing` to `collections.abc`
                let mut typing_to_collections_abc = vec![];
                if self.version >= PythonVersion::Py39 {
                    typing_to_collections_abc.extend(TYPING_TO_COLLECTIONS_ABC_39);
                }
                if self.version >= PythonVersion::Py310 {
                    typing_to_collections_abc.extend(TYPING_TO_COLLECTIONS_ABC_310);
                }
                if let Some(operation) =
                    self.try_replace(&typing_to_collections_abc, "collections.abc")
                {
                    operations.push(operation);
                }

                // `typing` to `collections`
                let mut typing_to_collections = vec![];
                if self.version >= PythonVersion::Py39 {
                    typing_to_collections.extend(TYPING_TO_COLLECTIONS_39);
                }
                if let Some(operation) = self.try_replace(&typing_to_collections, "collections") {
                    operations.push(operation);
                }

                // `typing` to `re`
                let mut typing_to_re = vec![];
                if self.version >= PythonVersion::Py39 {
                    typing_to_re.extend(TYPING_TO_RE_39);
                }
                if let Some(operation) = self.try_replace(&typing_to_re, "re") {
                    operations.push(operation);
                }
            }
            "typing.re" if self.version >= PythonVersion::Py39 => {
                if let Some(operation) = self.try_replace(TYPING_RE_TO_RE_39, "re") {
                    operations.push(operation);
                }
            }
            _ => {}
        }
        operations
    }

    fn try_replace(
        &'a self,
        candidates: &[&str],
        target: &'a str,
    ) -> Option<(WithoutRename, Option<String>)> {
        if candidates.is_empty() {
            return None;
        }

        let (matched_names, unmatched_names) = self.partition_imports(candidates);

        // If we have no matched names, we don't need to do anything.
        if matched_names.is_empty() {
            return None;
        }

        if unmatched_names.is_empty() {
            let matched = ImportReplacer::format_import_from(&matched_names, target);
            let operation = WithoutRename {
                target: target.to_string(),
                members: matched_names
                    .iter()
                    .map(|name| name.name.to_string())
                    .collect(),
                fixable: true,
            };
            let fix = Some(matched);
            Some((operation, fix))
        } else {
            let indentation = indentation(self.locator, self.stmt);

            // If we have matched _and_ unmatched names, but the import is not on its own
            // line, we can't add a statement after it. For example, if we have
            // `if True: import foo`, we can't add a statement to the next line.
            let Some(indentation) = indentation else {
                let operation = WithoutRename {
                    target: target.to_string(),
                    members: matched_names
                        .iter()
                        .map(|name| name.name.to_string())
                        .collect(),
                    fixable: false,
                };
                let fix = None;
                return Some((operation, fix));
            };

            let matched = ImportReplacer::format_import_from(&matched_names, target);
            let unmatched = fixes::remove_import_members(
                self.locator.slice(self.stmt.range()),
                &matched_names
                    .iter()
                    .map(|name| name.name.as_str())
                    .collect::<Vec<_>>(),
            );

            let operation = WithoutRename {
                target: target.to_string(),
                members: matched_names
                    .iter()
                    .map(|name| name.name.to_string())
                    .collect(),
                fixable: true,
            };
            let fix = Some(format!(
                "{unmatched}{}{}{matched}",
                self.stylist.line_ending().as_str(),
                indentation,
            ));
            Some((operation, fix))
        }
    }

    /// Partitions imports into matched and unmatched names.
    fn partition_imports(&self, candidates: &[&str]) -> (Vec<&Alias>, Vec<&Alias>) {
        let mut matched_names = vec![];
        let mut unmatched_names = vec![];
        for name in self.members {
            if candidates.contains(&name.name.as_str()) {
                matched_names.push(name);
            } else {
                unmatched_names.push(name);
            }
        }
        (matched_names, unmatched_names)
    }

    /// Converts a list of names and a module into an `import from`-style
    /// import.
    fn format_import_from(names: &[&Alias], module: &str) -> String {
        // Construct the whitespace strings.
        // Generate the formatted names.
        let qualified_names: String = names
            .iter()
            .map(|name| match &name.asname {
                Some(asname) => format!("{} as {}", name.name, asname),
                None => format!("{}", name.name),
            })
            .join(", ");
        format!("from {module} import {qualified_names}")
    }
}

/// UP035
pub(crate) fn deprecated_import(
    checker: &mut Checker,
    stmt: &Stmt,
    names: &[Alias],
    module: Option<&str>,
    level: Option<u32>,
) {
    // Avoid relative and star imports.
    if level.is_some_and(|level| level > 0) {
        return;
    }
    if names.first().is_some_and(|name| &name.name == "*") {
        return;
    }
    let Some(module) = module else {
        return;
    };

    if !is_relevant_module(module) {
        return;
    }

    let members: Vec<Alias> = names.iter().map(Clone::clone).collect();
    let fixer = ImportReplacer::new(
        stmt,
        module,
        &members,
        checker.locator(),
        checker.stylist(),
        checker.settings.target_version,
    );

    for (operation, fix) in fixer.without_renames() {
        let mut diagnostic = Diagnostic::new(
            DeprecatedImport {
                deprecation: Deprecation::WithoutRename(operation),
            },
            stmt.range(),
        );
        if checker.patch(Rule::DeprecatedImport) {
            if let Some(content) = fix {
                diagnostic.set_fix(Fix::suggested(Edit::range_replacement(
                    content,
                    stmt.range(),
                )));
            }
        }
        checker.diagnostics.push(diagnostic);
    }

    for operation in fixer.with_renames() {
        let diagnostic = Diagnostic::new(
            DeprecatedImport {
                deprecation: Deprecation::WithRename(operation),
            },
            stmt.range(),
        );
        checker.diagnostics.push(diagnostic);
    }
}
