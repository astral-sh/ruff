use itertools::Itertools;
use ruff_macros::{define_violation, derive_message_formats};
use rustpython_parser::ast::{Alias, AliasData, Stmt};

use crate::ast::types::Range;
use crate::ast::whitespace::indentation;
use crate::checkers::ast::Checker;
use crate::fix::Fix;
use crate::registry::{Diagnostic, Rule};
use crate::rules::pyupgrade::fixes;
use crate::settings::types::PythonVersion;
use crate::source_code::{Locator, Stylist};
use crate::violation::{Availability, Violation};
use crate::AutofixKind;

define_violation!(
    pub struct ImportReplacements {
        pub module: String,
        pub members: Vec<String>,
        pub fixable: bool,
    }
);
impl Violation for ImportReplacements {
    const AUTOFIX: Option<AutofixKind> = Some(AutofixKind::new(Availability::Sometimes));

    #[derive_message_formats]
    fn message(&self) -> String {
        let ImportReplacements {
            module, members, ..
        } = self;
        let names = members.iter().map(|name| format!("`{name}`")).join(", ");
        format!("Import from `{module}` instead: {names}")
    }

    fn autofix_title_formatter(&self) -> Option<fn(&Self) -> String> {
        let ImportReplacements { fixable, .. } = self;
        if *fixable {
            Some(|ImportReplacements { module, .. }| format!("Import from `{module}`"))
        } else {
            None
        }
    }
}

// A list of modules that may involve import rewrites.
const RELEVANT_MODULES: &[&str] = &[
    "collections",
    "pipes",
    "mypy_extensions",
    "typing_extensions",
    "typing",
    "typing.re",
];

// Members of `collections` that have been moved to `collections.abc`.
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

// Members of `pipes` that have been moved to `shlex`.
const PIPES_TO_SHLEX: &[&str] = &["quote"];

// Members of `typing_extensions` that have been moved to `typing`.
const TYPING_EXTENSIONS_TO_TYPING: &[&str] = &[
    "AsyncIterable",
    "AsyncIterator",
    "Awaitable",
    "ClassVar",
    "ContextManager",
    "Coroutine",
    "DefaultDict",
    "NewType",
    "TYPE_CHECKING",
    "Text",
    "Type",
];

// Python 3.7+

// Members of `mypy_extensions` that have been moved to `typing`.
const MYPY_EXTENSIONS_TO_TYPING_37: &[&str] = &["NoReturn"];

// Members of `typing_extensions` that have been moved to `typing`.
const TYPING_EXTENSIONS_TO_TYPING_37: &[&str] = &[
    "AsyncContextManager",
    "AsyncGenerator",
    "ChainMap",
    "Counter",
    "Deque",
    "NoReturn",
];

// Python 3.8+

// Members of `mypy_extensions` that have been moved to `typing`.
const MYPY_EXTENSIONS_TO_TYPING_38: &[&str] = &["TypedDict"];

// Members of `typing_extensions` that have been moved to `typing`.
const TYPING_EXTENSIONS_TO_TYPING_38: &[&str] = &[
    "Final",
    "Literal",
    "OrderedDict",
    "Protocol",
    "SupportsIndex",
    "runtime_checkable",
];

// Python 3.9+

// Members of `typing` that have been moved to `collections.abc`.
const TYPING_TO_COLLECTIONS_ABC_39: &[&str] = &[
    "AsyncGenerator",
    "AsyncIterable",
    "AsyncIterator",
    "Awaitable",
    "ByteString",
    "ChainMap",
    "Collection",
    "Container",
    "Coroutine",
    "Counter",
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

// Members of `typing` that have been moved to `typing.re`.
const TYPING_TO_RE_39: &[&str] = &["Match", "Pattern"];

// Members of `typing.re` that have been moved to `re`.
const TYPING_RE_TO_RE_39: &[&str] = &["Match", "Pattern"];

// Members of `typing_extensions` that have been moved to `typing`.
const TYPING_EXTENSIONS_TO_TYPING_39: &[&str] = &["Annotated", "get_type_hints"];

// Python 3.10+

// Members of `typing` that have been moved to `collections.abc`.
const TYPING_TO_COLLECTIONS_ABC_310: &[&str] = &["Callable"];

// Members of `typing_extensions` that have been moved to `typing`.
const TYPING_EXTENSIONS_TO_TYPING_310: &[&str] = &[
    "Concatenate",
    "ParamSpecArgs",
    "ParamSpecKwargs",
    "TypeAlias",
    "TypeGuard",
    "get_args",
    "get_origin",
    "is_typeddict",
];

// Python 3.11+

// Members of `typing_extensions` that have been moved to `typing`.
const TYPING_EXTENSIONS_TO_TYPING_311: &[&str] = &[
    "Any",
    "LiteralString",
    "NamedTuple",
    "Never",
    "NotRequired",
    "Required",
    "Self",
    "TypedDict",
    "Unpack",
    "assert_never",
    "assert_type",
    "clear_overloads",
    "dataclass_transform",
    "final",
    "get_overloads",
    "overload",
    "reveal_type",
];

struct Replacement<'a> {
    module: &'a str,
    members: Vec<&'a AliasData>,
    content: Option<String>,
}

struct ImportReplacer<'a> {
    stmt: &'a Stmt,
    module: &'a str,
    members: &'a [AliasData],
    locator: &'a Locator<'a>,
    stylist: &'a Stylist<'a>,
    version: PythonVersion,
}

impl<'a> ImportReplacer<'a> {
    const fn new(
        stmt: &'a Stmt,
        module: &'a str,
        members: &'a [AliasData],
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

    fn replacements(&self) -> Vec<Replacement> {
        let mut replacements = vec![];
        match self.module {
            "collections" => {
                if let Some(replacement) = self.try_replace(COLLECTIONS_TO_ABC, "collections.abc") {
                    replacements.push(replacement);
                }
            }
            "pipes" => {
                if let Some(replacement) = self.try_replace(PIPES_TO_SHLEX, "shlex") {
                    replacements.push(replacement);
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
                if let Some(replacement) = self.try_replace(&typing_extensions_to_typing, "typing")
                {
                    replacements.push(replacement);
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
                if let Some(replacement) = self.try_replace(&mypy_extensions_to_typing, "typing") {
                    replacements.push(replacement);
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
                if let Some(replacement) =
                    self.try_replace(&typing_to_collections_abc, "collections.abc")
                {
                    replacements.push(replacement);
                }

                // `typing` to `re`
                let mut typing_to_re = vec![];
                if self.version >= PythonVersion::Py39 {
                    typing_to_re.extend(TYPING_TO_RE_39);
                }
                if let Some(replacement) = self.try_replace(&typing_to_re, "re") {
                    replacements.push(replacement);
                }
            }
            "typing.re" if self.version >= PythonVersion::Py39 => {
                if let Some(replacement) = self.try_replace(TYPING_RE_TO_RE_39, "re") {
                    replacements.push(replacement);
                }
            }
            _ => {}
        }
        replacements
    }

    fn try_replace(&'a self, candidates: &[&str], target: &'a str) -> Option<Replacement<'a>> {
        let (matched_names, unmatched_names) = self.partition_imports(candidates);

        // If we have no matched names, we don't need to do anything.
        if matched_names.is_empty() {
            return None;
        }

        if unmatched_names.is_empty() {
            let matched = ImportReplacer::format_import_from(&matched_names, target);
            Some(Replacement {
                module: target,
                members: matched_names,
                content: Some(matched),
            })
        } else {
            let indentation = indentation(self.locator, self.stmt);

            // If we have matched _and_ unmatched names, but the import is not on its own
            // line, we can't add a statement after it. For example, if we have
            // `if True: import foo`, we can't add a statement to the next line.
            let Some(indentation) = indentation else {
                return Some(Replacement {
                    module: target,
                    members: matched_names,
                    content: None,
                });
            };

            let matched = ImportReplacer::format_import_from(&matched_names, target);
            let unmatched = fixes::remove_import_members(
                self.locator
                    .slice_source_code_range(&Range::from_located(self.stmt)),
                &matched_names
                    .iter()
                    .map(|name| name.name.as_str())
                    .collect::<Vec<_>>(),
            );

            Some(Replacement {
                module: target,
                members: matched_names,
                content: Some(format!(
                    "{unmatched}{}{}{matched}",
                    self.stylist.line_ending().as_str(),
                    indentation,
                )),
            })
        }
    }

    /// Partitions imports into matched and unmatched names.
    fn partition_imports(&self, candidates: &[&str]) -> (Vec<&AliasData>, Vec<&AliasData>) {
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
    fn format_import_from(names: &[&AliasData], module: &str) -> String {
        // Construct the whitespace strings.
        // Generate the formatted names.
        let full_names: String = names
            .iter()
            .map(|name| match &name.asname {
                Some(asname) => format!("{} as {asname}", name.name),
                None => format!("{}", name.name),
            })
            .join(", ");
        format!("from {module} import {full_names}")
    }
}

/// UP035
pub fn import_replacements(
    checker: &mut Checker,
    stmt: &Stmt,
    names: &[Alias],
    module: Option<&str>,
    level: Option<&usize>,
) {
    // Avoid relative and star imports.
    if level.map_or(false, |level| *level > 0) {
        return;
    }
    if names.first().map_or(false, |name| name.node.name == "*") {
        return;
    }
    let Some(module) = module else {
        return;
    };

    if !RELEVANT_MODULES.contains(&module) {
        return;
    }

    let members: Vec<AliasData> = names.iter().map(|alias| alias.node.clone()).collect();
    let fixer = ImportReplacer::new(
        stmt,
        module,
        &members,
        checker.locator,
        checker.stylist,
        checker.settings.target_version,
    );

    for replacement in fixer.replacements() {
        let mut diagnostic = Diagnostic::new(
            ImportReplacements {
                module: replacement.module.to_string(),
                members: replacement
                    .members
                    .iter()
                    .map(|name| name.name.to_string())
                    .collect(),
                fixable: replacement.content.is_some(),
            },
            Range::from_located(stmt),
        );
        if checker.patch(&Rule::ImportReplacements) {
            if let Some(content) = replacement.content {
                diagnostic.amend(Fix::replacement(
                    content,
                    stmt.location,
                    stmt.end_location.unwrap(),
                ));
            }
        }
        checker.diagnostics.push(diagnostic);
    }
}
