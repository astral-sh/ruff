//! Implements logic used by the document symbol provider, workspace symbol
//! provider, and auto-import feature of the completion provider.

use regex::Regex;

use ruff_db::files::File;
use ruff_db::parsed::parsed_module;
use ruff_python_ast::visitor::source_order::{self, SourceOrderVisitor};
use ruff_python_ast::{Expr, Stmt};
use ruff_text_size::{Ranged, TextRange};
use ty_project::Db;

/// Options that control which symbols are returned
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SymbolsOptions {
    /// Return a hierarchy of symbols or a flattened list?
    pub hierarchical: bool,
    /// Include only symbols in the global scope
    pub global_only: bool,
    /// Query string for filtering symbol names
    pub query_string: Option<QueryPattern>,
}

#[derive(Clone, Debug)]
pub struct QueryPattern {
    re: Option<Regex>,
    original: String,
}

impl QueryPattern {
    pub fn new(literal_query_string: &str) -> QueryPattern {
        let mut pattern = "(?i)".to_string();
        for ch in literal_query_string.chars() {
            pattern.push_str(&regex::escape(ch.encode_utf8(&mut [0; 4])));
            pattern.push_str(".*");
        }
        // In theory regex compilation could fail if the pattern string
        // was long enough to exceed the default regex compilation size
        // limit. But this length would be approaching ~10MB or so.
        QueryPattern {
            re: Regex::new(&pattern).ok(),
            original: literal_query_string.to_string(),
        }
    }

    fn is_match(&self, symbol: &SymbolInfo) -> bool {
        self.is_match_symbol_name(&symbol.name)
    }

    fn is_match_symbol_name(&self, symbol_name: &str) -> bool {
        if let Some(ref re) = self.re {
            re.is_match(symbol_name)
        } else {
            // This is a degenerate case. The only way
            // we should get here is if the query string
            // was thousands (or more) characters long.
            symbol_name.contains(&self.original)
        }
    }
}

impl From<&str> for QueryPattern {
    fn from(literal_query_string: &str) -> QueryPattern {
        QueryPattern::new(literal_query_string)
    }
}

impl Eq for QueryPattern {}

impl PartialEq for QueryPattern {
    fn eq(&self, rhs: &QueryPattern) -> bool {
        self.original == rhs.original
    }
}

/// Symbol information for IDE features like document outline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SymbolInfo {
    /// The name of the symbol
    pub name: String,
    /// The kind of symbol (function, class, variable, etc.)
    pub kind: SymbolKind,
    /// The range of the symbol name
    pub name_range: TextRange,
    /// The full range of the symbol (including body)
    pub full_range: TextRange,
    /// Child symbols (e.g., methods in a class)
    pub children: Vec<SymbolInfo>,
}

/// The kind of symbol
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SymbolKind {
    Module,
    Class,
    Method,
    Function,
    Variable,
    Constant,
    Property,
    Field,
    Constructor,
    Parameter,
    TypeParameter,
    Import,
}

impl SymbolKind {
    /// Returns the string representation of the symbol kind.
    pub fn to_string(self) -> &'static str {
        match self {
            SymbolKind::Module => "Module",
            SymbolKind::Class => "Class",
            SymbolKind::Method => "Method",
            SymbolKind::Function => "Function",
            SymbolKind::Variable => "Variable",
            SymbolKind::Constant => "Constant",
            SymbolKind::Property => "Property",
            SymbolKind::Field => "Field",
            SymbolKind::Constructor => "Constructor",
            SymbolKind::Parameter => "Parameter",
            SymbolKind::TypeParameter => "TypeParameter",
            SymbolKind::Import => "Import",
        }
    }
}

pub(crate) fn symbols_for_file<'db>(
    db: &'db dyn Db,
    file: File,
    options: &SymbolsOptions,
) -> impl Iterator<Item = &'db SymbolInfo> {
    assert!(
        !options.hierarchical || options.query_string.is_none(),
        "Cannot use hierarchical mode with a query string"
    );

    let ingredient = SymbolsOptionsWithoutQuery {
        hierarchical: options.hierarchical,
        global_only: options.global_only,
    };
    symbols_for_file_inner(db, file, ingredient)
        .iter()
        .filter(|symbol| {
            let Some(ref query) = options.query_string else {
                return true;
            };
            query.is_match(symbol)
        })
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct SymbolsOptionsWithoutQuery {
    hierarchical: bool,
    global_only: bool,
}

#[salsa::tracked(returns(deref))]
fn symbols_for_file_inner<'db>(
    db: &'db dyn Db,
    file: File,
    options: SymbolsOptionsWithoutQuery,
) -> Vec<SymbolInfo> {
    let parsed = parsed_module(db, file);
    let module = parsed.load(db);

    let mut visitor = SymbolVisitor {
        symbols: vec![],
        symbol_stack: vec![],
        in_function: false,
        options,
    };
    visitor.visit_body(&module.syntax().body);
    visitor.symbols
}

struct SymbolVisitor {
    symbols: Vec<SymbolInfo>,
    symbol_stack: Vec<SymbolInfo>,
    /// Track if we're currently inside a function (to exclude local variables)
    in_function: bool,
    options: SymbolsOptionsWithoutQuery,
}

impl SymbolVisitor {
    fn visit_body(&mut self, body: &[Stmt]) {
        for stmt in body {
            self.visit_stmt(stmt);
        }
    }

    fn add_symbol(&mut self, symbol: SymbolInfo) {
        if self.options.hierarchical {
            if let Some(parent) = self.symbol_stack.last_mut() {
                parent.children.push(symbol);
            } else {
                self.symbols.push(symbol);
            }
        } else {
            self.symbols.push(symbol);
        }
    }

    fn push_symbol(&mut self, symbol: SymbolInfo) {
        if self.options.hierarchical {
            self.symbol_stack.push(symbol);
        } else {
            self.add_symbol(symbol);
        }
    }

    fn pop_symbol(&mut self) {
        if self.options.hierarchical {
            if let Some(symbol) = self.symbol_stack.pop() {
                self.add_symbol(symbol);
            }
        }
    }

    fn is_constant_name(name: &str) -> bool {
        name.chars().all(|c| c.is_ascii_uppercase() || c == '_')
    }
}

impl SourceOrderVisitor<'_> for SymbolVisitor {
    fn visit_stmt(&mut self, stmt: &Stmt) {
        match stmt {
            Stmt::FunctionDef(func_def) => {
                let kind = if self
                    .symbol_stack
                    .iter()
                    .any(|s| s.kind == SymbolKind::Class)
                {
                    if func_def.name.as_str() == "__init__" {
                        SymbolKind::Constructor
                    } else {
                        SymbolKind::Method
                    }
                } else {
                    SymbolKind::Function
                };

                let symbol = SymbolInfo {
                    name: func_def.name.to_string(),
                    kind,
                    name_range: func_def.name.range(),
                    full_range: stmt.range(),
                    children: Vec::new(),
                };

                if self.options.global_only {
                    self.add_symbol(symbol);
                    // If global_only, don't walk function bodies
                    return;
                }

                self.push_symbol(symbol);

                // Mark that we're entering a function scope
                let was_in_function = self.in_function;
                self.in_function = true;

                source_order::walk_stmt(self, stmt);

                // Restore the previous function scope state
                self.in_function = was_in_function;

                self.pop_symbol();
            }

            Stmt::ClassDef(class_def) => {
                let symbol = SymbolInfo {
                    name: class_def.name.to_string(),
                    kind: SymbolKind::Class,
                    name_range: class_def.name.range(),
                    full_range: stmt.range(),
                    children: Vec::new(),
                };

                if self.options.global_only {
                    self.add_symbol(symbol);
                    // If global_only, don't walk class bodies
                    return;
                }

                self.push_symbol(symbol);
                source_order::walk_stmt(self, stmt);
                self.pop_symbol();
            }

            Stmt::Assign(assign) => {
                // Include assignments only when we're in global or class scope
                if !self.in_function {
                    for target in &assign.targets {
                        if let Expr::Name(name) = target {
                            let kind = if Self::is_constant_name(name.id.as_str()) {
                                SymbolKind::Constant
                            } else if self
                                .symbol_stack
                                .iter()
                                .any(|s| s.kind == SymbolKind::Class)
                            {
                                SymbolKind::Field
                            } else {
                                SymbolKind::Variable
                            };

                            let symbol = SymbolInfo {
                                name: name.id.to_string(),
                                kind,
                                name_range: name.range(),
                                full_range: stmt.range(),
                                children: Vec::new(),
                            };

                            self.add_symbol(symbol);
                        }
                    }
                }
            }

            Stmt::AnnAssign(ann_assign) => {
                // Include assignments only when we're in global or class scope
                if !self.in_function {
                    if let Expr::Name(name) = &*ann_assign.target {
                        let kind = if Self::is_constant_name(name.id.as_str()) {
                            SymbolKind::Constant
                        } else if self
                            .symbol_stack
                            .iter()
                            .any(|s| s.kind == SymbolKind::Class)
                        {
                            SymbolKind::Field
                        } else {
                            SymbolKind::Variable
                        };

                        let symbol = SymbolInfo {
                            name: name.id.to_string(),
                            kind,
                            name_range: name.range(),
                            full_range: stmt.range(),
                            children: Vec::new(),
                        };

                        self.add_symbol(symbol);
                    }
                }
            }

            _ => {
                source_order::walk_stmt(self, stmt);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    fn matches(query: &str, symbol: &str) -> bool {
        super::QueryPattern::new(query).is_match_symbol_name(symbol)
    }

    #[test]
    fn various_yes() {
        assert!(matches("", ""));
        assert!(matches("", "a"));
        assert!(matches("", "abc"));

        assert!(matches("a", "a"));
        assert!(matches("a", "abc"));
        assert!(matches("a", "xaz"));
        assert!(matches("a", "xza"));

        assert!(matches("abc", "abc"));
        assert!(matches("abc", "axbyc"));
        assert!(matches("abc", "waxbycz"));
        assert!(matches("abc", "WAXBYCZ"));
        assert!(matches("ABC", "waxbycz"));
        assert!(matches("ABC", "WAXBYCZ"));
        assert!(matches("aBc", "wAXbyCZ"));

        assert!(matches("δ", "Δ"));
        assert!(matches("δΘπ", "ΔθΠ"));
    }

    #[test]
    fn various_no() {
        assert!(!matches("a", ""));
        assert!(!matches("abc", "bac"));
        assert!(!matches("abcd", "abc"));
        assert!(!matches("δΘπ", "θΔΠ"));
    }
}
