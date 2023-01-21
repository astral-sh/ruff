use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::fix::Fix;
use crate::registry::Diagnostic;
use crate::violations;
use rustpython_ast::{AliasData, Located, Stmt, StmtKind};

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

struct FixImports<'a> {
    module: &'a str,
    multi_line: bool,
    names: &'a [AliasData],
    indent: &'a str,
}

impl<'a> FixImports<'a> {
    fn new(module: &'a str, multi_line: bool, names: &'a [AliasData], indent: &'a str) -> Self {
        Self {
            module,
            multi_line,
            names,
            indent,
        }
    }

    fn check_replacement(&self) -> Option<String> {
        match self.module {
            "collections" => self.create_new_str(COLLECTIONS_TO_ABC, "collections.abc"),
            _ => return None,
        }
    }

    /// Converts the string of imports into new one
    fn create_new_str(&self, matches: &[&str], replace: &str) -> Option<String> {
        let (matching_names, unmatching_names) = self.get_import_lists(matches);
        let unmatching = self.get_str(&unmatching_names, self.module);
        let matching = self.get_str(&matching_names, replace);
        if !unmatching.is_empty() && !matching.is_empty() {
            Some(format!("{unmatching}\n{matching}"))
        } else if !unmatching.is_empty() {
            Some(unmatching)
        } else if !matching.is_empty() {
            Some(matching)
        } else {
            None
        }
    }

    /// Returns a list of imports that does and does not have a match in the given list of matches
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
            let short_indent = if self.indent.len() > 3 {
                &self.indent[3..]
            } else {
                ""
            };
            format!("\n{short_indent})")
        } else {
            "".to_string()
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
    // Pyupgrade only works with import_from statements, so this linter does that as well
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
    let indent = match names.get(0) {
        None => return,
        Some(item) => item
    };
    let is_mulit_line = module_text.contains('\n');
    let fixer = FixImports::new(clean_mod, is_mulit_line, &clean_names, "");
    let clean_result = fixer.check_replacement();
    println!("{:?}", clean_result);
}
