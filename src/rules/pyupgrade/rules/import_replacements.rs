use ruff_macros::derive_message_formats;
use rustpython_ast::{AliasData, Located, Stmt};

use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::define_violation;
use crate::fix::Fix;
use crate::registry::{Diagnostic, Rule};
use crate::rules::pyupgrade::helpers::{get_fromimport_str, ImportFormatting};
use crate::settings::types::PythonVersion;
use crate::violation::AlwaysAutofixableViolation;

define_violation!(
    pub struct ImportReplacements;
);
impl AlwaysAutofixableViolation for ImportReplacements {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Replace old formatting imports with their new versions")
    }

    fn autofix_title(&self) -> String {
        "Updated the import".to_string()
    }
}

const BAD_MODULES: &[&str] = &[
    "collections",
    "pipes",
    "mypy_extensions",
    "typing_extensions",
    "typing",
    "typing.re",
];

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

const PIPES_TO_SHLEX: &[&str] = &["quote"];

const TYPINGEXTENSIONS_TO_TYPING: &[&str] = &[
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

// Items below this require python 3.7 or higher

const MYPYEXTENSIONS_TO_TYPING_37: &[&str] = &["NoReturn"];

const TYPINGEXTENSIONS_TO_TYPING_37: &[&str] = &[
    "AsyncContextManager",
    "AsyncGenerator",
    "ChainMap",
    "Counter",
    "Deque",
    "NoReturn",
];

// Items below this require python 3.8 or higher

const MYPYEXTENSIONS_TO_TYPING_38: &[&str] = &["TypedDict"];

const TYPINGEXTENSIONS_TO_TYPING_38: &[&str] = &[
    "Final",
    "Literal",
    "OrderedDict",
    "Protocol",
    "SupportsInded",
    "runtime_checkable",
];

// Items below this require python 3.9 or higher

const TYPING_TO_COLLECTIONSABC_39: &[&str] = &[
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

const TYPING_TO_RE_39: &[&str] = &["Match", "Pattern"];

const TYPINGRE_TO_RE_39: &[&str] = &["Match", "Pattern"];

const TYPINGEXTENSIONS_TO_TYPING_39: &[&str] = &["Annotated", "get_type_hints"];

// Items below this require python 3.10 or higher

const TYPING_TO_COLLECTIONSABC_310: &[&str] = &["Callable"];

const TYPINGEXTENSIONS_TO_TYPING_310: &[&str] = &[
    "Concatenate",
    "ParamSpecArgs",
    "ParamSpecKwargs",
    "TypeAlias",
    "TypeGuard",
    "get_args",
    "get_origin",
    "is_typeddict",
];

// Items below this require python 3.11 or higher

const TYPINGEXTENSIONS_TO_TYPING_311: &[&str] = &[
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
    // This is the indent level of the first named import
    indent: &'a str,
    // This is the indent for the parentheses at the end of a multi-line statement
    short_indent: &'a str,
    // This is the indent of the actual import statement
    starting_indent: &'a str,
    version: PythonVersion,
}

impl<'a> FixImports<'a> {
    fn new(
        module: &'a str,
        multi_line: bool,
        names: &'a [AliasData],
        indent: &'a str,
        version: PythonVersion,
        starting_indent: &'a str,
    ) -> Self {
        let short_indent = if indent.len() > 3 {
            &indent[3..]
        } else {
            indent
        };
        Self {
            module,
            multi_line,
            names,
            indent,
            short_indent,
            starting_indent,
            version,
        }
    }

    fn check_replacement(&self) -> Option<String> {
        match self.module {
            "collections" => self.create_new_str(COLLECTIONS_TO_ABC, "collections.abc"),
            "pipes" => self.create_new_str(PIPES_TO_SHLEX, "shlex"),
            "typing_extensions" => {
                if has_match(TYPINGEXTENSIONS_TO_TYPING, self.names) {
                    self.create_new_str(TYPINGEXTENSIONS_TO_TYPING, "typing")
                } else if has_match(TYPINGEXTENSIONS_TO_TYPING_37, self.names)
                    && self.version >= PythonVersion::Py37
                {
                    self.create_new_str(TYPINGEXTENSIONS_TO_TYPING_37, "typing")
                } else if has_match(TYPINGEXTENSIONS_TO_TYPING_38, self.names)
                    && self.version >= PythonVersion::Py38
                {
                    self.create_new_str(TYPINGEXTENSIONS_TO_TYPING_38, "typing")
                } else if has_match(TYPINGEXTENSIONS_TO_TYPING_39, self.names)
                    && self.version >= PythonVersion::Py39
                {
                    self.create_new_str(TYPINGEXTENSIONS_TO_TYPING_39, "typing")
                } else if has_match(TYPINGEXTENSIONS_TO_TYPING_310, self.names)
                    && self.version >= PythonVersion::Py310
                {
                    self.create_new_str(TYPINGEXTENSIONS_TO_TYPING_310, "typing")
                } else if has_match(TYPINGEXTENSIONS_TO_TYPING_311, self.names)
                    && self.version >= PythonVersion::Py311
                {
                    self.create_new_str(TYPINGEXTENSIONS_TO_TYPING_311, "typing")
                } else {
                    None
                }
            }
            "mypy_extensions" => {
                if has_match(MYPYEXTENSIONS_TO_TYPING_37, self.names)
                    && self.version >= PythonVersion::Py37
                {
                    self.create_new_str(MYPYEXTENSIONS_TO_TYPING_37, "typing")
                } else if has_match(MYPYEXTENSIONS_TO_TYPING_38, self.names)
                    && self.version >= PythonVersion::Py38
                {
                    self.create_new_str(MYPYEXTENSIONS_TO_TYPING_38, "typing")
                } else {
                    None
                }
            }
            "typing" => {
                if has_match(TYPING_TO_COLLECTIONSABC_39, self.names)
                    && self.version >= PythonVersion::Py39
                {
                    self.create_new_str(TYPING_TO_COLLECTIONSABC_39, "collections.abc")
                } else if has_match(TYPING_TO_RE_39, self.names)
                    && self.version >= PythonVersion::Py39
                {
                    self.create_new_str(TYPING_TO_RE_39, "re")
                } else if has_match(TYPING_TO_COLLECTIONSABC_310, self.names)
                    && self.version >= PythonVersion::Py310
                {
                    self.create_new_str(TYPING_TO_COLLECTIONSABC_310, "collections.abc")
                } else {
                    None
                }
            }
            "typing.re" if self.version >= PythonVersion::Py39 => {
                self.create_new_str(TYPINGRE_TO_RE_39, "re")
            }
            _ => None,
        }
    }

    /// Converts the string of imports into new one
    fn create_new_str(&self, matches: &[&str], replace: &str) -> Option<String> {
        let (matching_names, unmatching_names) = self.get_import_lists(matches);
        let unmatching = get_fromimport_str(
            &unmatching_names,
            self.module,
            self.multi_line,
            self.indent,
            self.short_indent,
        );
        let matching = get_fromimport_str(
            &matching_names,
            replace,
            self.multi_line,
            self.indent,
            self.short_indent,
        );
        // We don't replace if there is just an unmatching, because then we don't need
        // to refactor
        if !unmatching.is_empty() && !matching.is_empty() {
            Some(format!("{unmatching}\n{}{matching}", self.starting_indent))
        } else if !matching.is_empty() {
            Some(matching)
        } else {
            None
        }
    }

    /// Returns a list of imports that does and does not have a match in the
    /// given list of matches
    fn get_import_lists(&self, matches: &[&str]) -> (Vec<AliasData>, Vec<AliasData>) {
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
    names: &Vec<Located<AliasData>>,
    module: &Option<String>,
) {
    // Pyupgrade only works with import_from statements, so this linter does that as
    // well
    let clean_mod = match module {
        None => return,
        Some(item) => item,
    };
    if !BAD_MODULES.contains(&clean_mod.as_str()) {
        return;
    }
    let mut clean_names: Vec<AliasData> = vec![];
    for name in names {
        clean_names.push(name.node.clone());
    }
    let module_text = checker
        .locator
        .slice_source_code_range(&Range::from_located(stmt));
    let formatting = ImportFormatting::new(checker.locator, stmt, names);
    let fixer = FixImports::new(
        clean_mod,
        formatting.multi_line,
        &clean_names,
        &formatting.indent,
        checker.settings.target_version,
        &formatting.start_indent,
    );
    let clean_result = match fixer.check_replacement() {
        None => return,
        Some(item) => item,
    };
    if clean_result == module_text {
        return;
    }
    let range = Range::from_located(stmt);
    let mut diagnostic = Diagnostic::new(ImportReplacements, range);
    if checker.patch(&Rule::ImportReplacements) {
        diagnostic.amend(Fix::replacement(
            clean_result,
            stmt.location,
            stmt.end_location.unwrap(),
        ));
    }
    checker.diagnostics.push(diagnostic);
}
