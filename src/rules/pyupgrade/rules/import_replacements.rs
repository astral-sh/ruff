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
    names: &'a [AliasData],
    formatting: ImportFormatting<'a>,
    version: PythonVersion,
    stylist: &'a Stylist<'a>,
}

impl<'a> FixImports<'a> {
    fn new(
        module: &'a str,
        names: &'a [AliasData],
        formatting: ImportFormatting<'a>,
        version: PythonVersion,
        stylist: &'a Stylist,
    ) -> Self {
        Self {
            module,
            names,
            formatting,
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

    // TODO(charlie): This needs to return the `replace` module and the list of matched names,
    // to improve the error message.
    fn create_new_str(&self, matches: &[&str], replace: &str) -> Option<String> {
        let (matched_names, unmatched_names) = self.partition_imports(matches);

        // If we have no matched names, we don't need to do anything.
        // If we have matched _and_ unmatched names, but the import is not on its own line, we
        // can't add a statement after it. For example, if we have `if True: import foo`, we can't
        // add a statement to the next line.
        if matched_names.is_empty() || (!unmatched_names.is_empty() && !self.formatting.own_line) {
            return None;
        }

        let matched = format_import_from(
            &matched_names,
            replace,
            self.formatting.multi_line,
            self.formatting.member_indent,
            self.formatting.stmt_indent,
            self.stylist,
        );

        if unmatched_names.is_empty() {
            return Some(matched);
        }

        let unmatched = format_import_from(
            &unmatched_names,
            self.module,
            self.formatting.multi_line,
            self.formatting.member_indent,
            self.formatting.stmt_indent,
            self.stylist,
        );
        Some(format!(
            "{unmatched}{}{}{matched}",
            self.stylist.line_ending().as_str(),
            self.formatting.stmt_indent
        ))
    }

    /// Partitions imports into matched and unmatched names.
    fn partition_imports(&self, matches: &[&str]) -> (Vec<AliasData>, Vec<AliasData>) {
        let mut matched_names: Vec<AliasData> = vec![];
        let mut unmatched_names: Vec<AliasData> = vec![];
        for name in self.names {
            if matches.contains(&name.name.as_str()) {
                matched_names.push(name.clone());
            } else {
                unmatched_names.push(name.clone());
            }
        }
        (matched_names, unmatched_names)
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
        formatting,
        checker.settings.target_version,
        checker.stylist,
    );

    // TODO(charlie): Even if we can't fix this, we should still flag it. Some cases can't be fixed.
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
