use rustpython_ast::{AliasData, Located, Stmt};

use crate::ast::types::Range;
use crate::ast::whitespace::indentation;
use crate::checkers::ast::Checker;
use crate::fix::Fix;
use crate::registry::{Diagnostic, Rule};
use crate::violations;
use crate::settings::types::PythonVersion;

const BAD_MODULES: &[&str] = &[
    "collections",
    "pipes",
    "six",
    "six.moves",
    "six.moves.urllib",
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

const SIX_TO_IO: &[&str] = &["BytesIO", "StringIO"];

const SIX_TO_FUNCTOOLS: &[&str] = &["wraps"];

const SIXMOVES_TO_IO: &[&str] = &["io"];

const SIXMOVES_TO_COLLECTIONS: &[&str] = &["UserDict", "UserList", "UserString"];

const SIXMOVES_TO_ITERTOOLS: &[&str] = &["filterfalse", "zip_longest"];

const SIXMOVES_TO_OS: &[&str] = &["getcwd", "getcwdb"];

const SIXMOVES_TO_SUBPROCESS: &[&str] = &["getouput"];

const SIXMOVES_TO_SYS: &[&str] = &["intern"];

const SIXMOVES_TO_URLLIB: &[&str] = &["parse", "request", "response", "error", "robotparser"];

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
    indent: &'a str,
    short_indent: &'a str,
    version: PythonVersion,
}

impl<'a> FixImports<'a> {
    fn new(module: &'a str, multi_line: bool, names: &'a [AliasData], indent: &'a str, version: PythonVersion) -> Self {
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
            version
        }
    }

    fn check_replacement(&self) -> Option<String> {
        println!("Checking replacement for {}", self.module);
        match self.module {
            "collections" => self.create_new_str(COLLECTIONS_TO_ABC, "collections.abc"),
            "pipes" => self.create_new_str(PIPES_TO_SHLEX, "shlex"),
            "six" => self.create_new_str(SIX_TO_IO, "io"),
            "six.moves" => {
                if has_match(SIXMOVES_TO_IO, self.names) {
                    self.create_new_str(SIXMOVES_TO_IO, "io")
                } else if has_match(SIXMOVES_TO_COLLECTIONS, self.names) {
                    self.create_new_str(SIXMOVES_TO_COLLECTIONS, "collections")
                } else if has_match(SIXMOVES_TO_ITERTOOLS, self.names) {
                    self.create_new_str(SIXMOVES_TO_ITERTOOLS, "itertools")
                } else if has_match(SIXMOVES_TO_OS, self.names) {
                    self.create_new_str(SIXMOVES_TO_OS, "os")
                } else if has_match(SIXMOVES_TO_SUBPROCESS, self.names) {
                    self.create_new_str(SIXMOVES_TO_SUBPROCESS, "subprocess")
                } else if has_match(SIXMOVES_TO_SYS, self.names) {
                    self.create_new_str(SIXMOVES_TO_SYS, "sys")
                } else if has_match(SIXMOVES_TO_URLLIB, self.names) {
                    self.create_new_str(SIXMOVES_TO_URLLIB, "urllib")
                } else if has_match(SIX_TO_FUNCTOOLS, self.names) {
                    self.create_new_str(SIX_TO_FUNCTOOLS, "functools")
                } else {
                    None
                }
                },
            "typing_extensions" => {
                if has_match(TYPINGEXTENSIONS_TO_TYPING, self.names) {
                    self.create_new_str(TYPINGEXTENSIONS_TO_TYPING, "typing")
                } else if has_match(TYPINGEXTENSIONS_TO_TYPING_37, self.names) && self.version >= PythonVersion::Py37 {
                    self.create_new_str(TYPINGEXTENSIONS_TO_TYPING_37, "typing")
                } else if has_match(TYPINGEXTENSIONS_TO_TYPING_38, self.names) && self.version >= PythonVersion::Py38 {
                    self.create_new_str(TYPINGEXTENSIONS_TO_TYPING_38, "typing")
                } else if has_match(TYPINGEXTENSIONS_TO_TYPING_39, self.names) && self.version >= PythonVersion::Py39 {
                    self.create_new_str(TYPINGEXTENSIONS_TO_TYPING_39, "typing")
                } else if has_match(TYPINGEXTENSIONS_TO_TYPING_310, self.names) && self.version >= PythonVersion::Py310 {
                    self.create_new_str(TYPINGEXTENSIONS_TO_TYPING_310, "typing")
                } else if has_match(TYPINGEXTENSIONS_TO_TYPING_311, self.names) && self.version >= PythonVersion::Py311 {
                    self.create_new_str(TYPINGEXTENSIONS_TO_TYPING_311, "typing")
                } else {
                    None
                }
            }
            "mypy_extensions" => {
                if has_match(MYPYEXTENSIONS_TO_TYPING_37, self.names) && self.version >= PythonVersion::Py37 {
                    self.create_new_str(MYPYEXTENSIONS_TO_TYPING_37, "typing")
                } else if has_match(MYPYEXTENSIONS_TO_TYPING_38, self.names) && self.version >= PythonVersion::Py38 {
                    self.create_new_str(MYPYEXTENSIONS_TO_TYPING_38, "typing")
                } else {
                    None
                }
            }
            "typing" => {
                if has_match(TYPING_TO_COLLECTIONSABC_39, self.names) && self.version >= PythonVersion::Py39 {
                    self.create_new_str(TYPING_TO_COLLECTIONSABC_39, "collections.abc")
                } else if has_match(TYPING_TO_RE_39, self.names) && self.version >= PythonVersion::Py39 {
                    self.create_new_str(TYPING_TO_RE_39, "re")
                } else if has_match(TYPING_TO_COLLECTIONSABC_310, self.names) && self.version >= PythonVersion::Py310 {
                    self.create_new_str(TYPING_TO_COLLECTIONSABC_310, "collections.abc")
                } else {
                    None
                }
            }
            "typing.re" if self.version >= PythonVersion::Py39 => self.create_new_str(TYPINGRE_TO_RE_39, "re"),
            _ => None,
        }
    }

    /// Converts the string of imports into new one
    fn create_new_str(&self, matches: &[&str], replace: &str) -> Option<String> {
        let (matching_names, unmatching_names) = self.get_import_lists(matches);
        let unmatching = self.get_str(&unmatching_names, self.module);
        let matching = self.get_str(&matching_names, replace);
        if !unmatching.is_empty() && !matching.is_empty() {
            let shorter_indent = if self.short_indent.len() > 0 {
                self.short_indent[1..].to_string()
            } else {
                String::new()
            };
            Some(format!("{unmatching}\n{shorter_indent}{matching}"))
        } else if !unmatching.is_empty() {
            Some(unmatching)
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

    fn get_str(&self, names: &[AliasData], module: &str) -> String {
        if names.is_empty() {
            return String::new();
        }
        let after_comma = if self.multi_line { '\n' } else { ' ' };
        let start_imps = if self.multi_line { "(\n" } else { "" };
        let after_imps = if self.multi_line {
            format!("\n{})", self.short_indent)
        } else {
            String::new()
        };
        let mut full_names: Vec<String> = vec![];
        for name in names {
            let asname_str = match &name.asname {
                Some(item) => format!(" as {}", item),
                None => String::new(),
            };
            let final_string = format!("{}{}{}", self.indent, name.name, asname_str);
            full_names.push(final_string);
        }
        format!(
            "from {} import {}{}{}",
            module,
            start_imps,
            full_names.join(format!(",{}", after_comma).as_str()),
            after_imps
        )
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
    let is_multi_line = module_text.contains('\n');
    let indent = if is_multi_line {
        match names.get(0) {
            None => return,
            Some(item) => match indentation(checker.locator, item) {
                // This is an opninionated way of formatting import statements
                None => "    ".to_string(),
                Some(item) => item.to_string(),
            },
        }
    } else {
        String::new()
    };
    let fixer = FixImports::new(clean_mod, is_multi_line, &clean_names, &indent, checker.settings.target_version);
    let clean_result = match fixer.check_replacement() {
        None => return,
        Some(item) => item,
    };
    if clean_result == module_text {
        return;
    }
    let range = Range::from_located(stmt);
    let mut diagnostic = Diagnostic::new(violations::ImportReplacements, range);
    if checker.patch(&Rule::ImportReplacements) {
        diagnostic.amend(Fix::replacement(
            clean_result,
            stmt.location,
            stmt.end_location.unwrap(),
        ));
    }
    checker.diagnostics.push(diagnostic);
}
