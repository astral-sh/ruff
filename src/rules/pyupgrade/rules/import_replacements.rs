use rustpython_ast::{Alias, AliasData, Stmt};

use ruff_macros::derive_message_formats;

use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::define_violation;
use crate::fix::Fix;
use crate::registry::{Diagnostic, Rule};
use crate::rules::pyupgrade::helpers::{format_import_from, ImportFormatting};
use crate::settings::types::PythonVersion;
use crate::source_code::Stylist;
use crate::violation::AlwaysAutofixableViolation;

define_violation!(
    pub struct ImportReplacements {
        pub existing: String,
        pub replacement: String,
    }
);
impl AlwaysAutofixableViolation for ImportReplacements {
    #[derive_message_formats]
    fn message(&self) -> String {
        let ImportReplacements {
            existing,
            replacement,
        } = self;
        format!("Import `{existing}` from `{replacement}`")
    }

    fn autofix_title(&self) -> String {
        let ImportReplacements {
            existing: _,
            replacement,
        } = self;
        format!("Replace with `{replacement}`")
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
    "SupportsInded",
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

fn has_match(set1: &[&str], set2: &[AliasData]) -> bool {
    set2.iter().any(|x| set1.contains(&x.name.as_str()))
}

struct FixImports<'a> {
    module: &'a str,
    multi_line: bool,
    names: &'a [AliasData],
    // The indent level of the first named import.
    member_indent: &'a str,
    // The indent of the import statement.
    stmt_indent: &'a str,
    version: PythonVersion,
    stylist: &'a Stylist<'a>,
}

impl<'a> FixImports<'a> {
    fn new(
        module: &'a str,
        names: &'a [AliasData],
        multi_line: bool,
        member_indent: &'a str,
        stmt_indent: &'a str,
        version: PythonVersion,
        stylist: &'a Stylist,
    ) -> Self {
        Self {
            module,
            multi_line,
            names,
            member_indent,
            stmt_indent,
            version,
            stylist,
        }
    }

    fn check_replacement(&self) -> Option<String> {
        match self.module {
            "collections" => self.create_new_str(COLLECTIONS_TO_ABC, "collections.abc"),
            "pipes" => self.create_new_str(PIPES_TO_SHLEX, "shlex"),
            "typing_extensions" => {
                if has_match(TYPING_EXTENSIONS_TO_TYPING, self.names) {
                    self.create_new_str(TYPING_EXTENSIONS_TO_TYPING, "typing")
                } else if has_match(TYPING_EXTENSIONS_TO_TYPING_37, self.names)
                    && self.version >= PythonVersion::Py37
                {
                    self.create_new_str(TYPING_EXTENSIONS_TO_TYPING_37, "typing")
                } else if has_match(TYPING_EXTENSIONS_TO_TYPING_38, self.names)
                    && self.version >= PythonVersion::Py38
                {
                    self.create_new_str(TYPING_EXTENSIONS_TO_TYPING_38, "typing")
                } else if has_match(TYPING_EXTENSIONS_TO_TYPING_39, self.names)
                    && self.version >= PythonVersion::Py39
                {
                    self.create_new_str(TYPING_EXTENSIONS_TO_TYPING_39, "typing")
                } else if has_match(TYPING_EXTENSIONS_TO_TYPING_310, self.names)
                    && self.version >= PythonVersion::Py310
                {
                    self.create_new_str(TYPING_EXTENSIONS_TO_TYPING_310, "typing")
                } else if has_match(TYPING_EXTENSIONS_TO_TYPING_311, self.names)
                    && self.version >= PythonVersion::Py311
                {
                    self.create_new_str(TYPING_EXTENSIONS_TO_TYPING_311, "typing")
                } else {
                    None
                }
            }
            "mypy_extensions" => {
                if has_match(MYPY_EXTENSIONS_TO_TYPING_37, self.names)
                    && self.version >= PythonVersion::Py37
                {
                    self.create_new_str(MYPY_EXTENSIONS_TO_TYPING_37, "typing")
                } else if has_match(MYPY_EXTENSIONS_TO_TYPING_38, self.names)
                    && self.version >= PythonVersion::Py38
                {
                    self.create_new_str(MYPY_EXTENSIONS_TO_TYPING_38, "typing")
                } else {
                    None
                }
            }
            "typing" => {
                if has_match(TYPING_TO_COLLECTIONS_ABC_39, self.names)
                    && self.version >= PythonVersion::Py39
                {
                    self.create_new_str(TYPING_TO_COLLECTIONS_ABC_39, "collections.abc")
                } else if has_match(TYPING_TO_RE_39, self.names)
                    && self.version >= PythonVersion::Py39
                {
                    self.create_new_str(TYPING_TO_RE_39, "re")
                } else if has_match(TYPING_TO_COLLECTIONS_ABC_310, self.names)
                    && self.version >= PythonVersion::Py310
                {
                    self.create_new_str(TYPING_TO_COLLECTIONS_ABC_310, "collections.abc")
                } else {
                    None
                }
            }
            "typing.re" if self.version >= PythonVersion::Py39 => {
                self.create_new_str(TYPING_RE_TO_RE_39, "re")
            }
            _ => None,
        }
    }

    /// Converts the string of imports into new one
    fn create_new_str(&self, matches: &[&str], replace: &str) -> Option<String> {
        let (matching_names, unmatching_names) = self.split_imports(matches);
        if matching_names.is_empty() {
            return None;
        }

        let matching = format_import_from(
            &matching_names,
            replace,
            self.multi_line,
            self.member_indent,
            self.stmt_indent,
            self.stylist,
        );

        if unmatching_names.is_empty() {
            return Some(matching);
        }

        let unmatching = format_import_from(
            &unmatching_names,
            self.module,
            self.multi_line,
            self.member_indent,
            self.stmt_indent,
            self.stylist,
        );
        Some(format!("{unmatching}\n{}{matching}", self.stmt_indent))
    }

    /// Returns a list of imports that does and does not have a match in the
    /// given list of matches
    fn split_imports(&self, matches: &[&str]) -> (Vec<AliasData>, Vec<AliasData>) {
        let mut unmatching_names: Vec<AliasData> = vec![];
        let mut matching_names: Vec<AliasData> = vec![];

        for name in self.names {
            if matches.contains(&name.name.as_str()) {
                matching_names.push(name.clone());
            } else {
                unmatching_names.push(name.clone());
            }
        }
        (matching_names, unmatching_names)
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
    // Avoid relative imports.
    if level.map_or(false, |level| *level > 0) {
        return;
    }
    let Some(module) = module else {
        return;
    };

    if !RELEVANT_MODULES.contains(&module) {
        return;
    }

    let formatting = ImportFormatting::new(checker.locator, checker.stylist, stmt, names);
    let names: Vec<AliasData> = names.iter().map(|alias| alias.node.clone()).collect();
    let fixer = FixImports::new(
        module,
        &names,
        formatting.multi_line,
        &formatting.member_indent,
        &formatting.stmt_indent,
        checker.settings.target_version,
        checker.stylist,
    );
    let Some(content) = fixer.check_replacement() else {
        return;
    };

    let mut diagnostic = Diagnostic::new(
        ImportReplacements {
            existing: module.to_string(),
            replacement: content.to_string(),
        },
        Range::from_located(stmt),
    );
    if checker.patch(&Rule::ImportReplacements) {
        diagnostic.amend(Fix::replacement(
            content,
            stmt.location,
            stmt.end_location.unwrap(),
        ));
    }
    checker.diagnostics.push(diagnostic);
}
