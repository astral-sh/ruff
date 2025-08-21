//! Implements logic used by the document symbol provider, workspace symbol
//! provider, and auto-import feature of the completion provider.

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
    pub query_string: Option<String>,
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

pub(crate) fn symbols_for_file(
    db: &dyn Db,
    file: File,
    options: &SymbolsOptions,
) -> Vec<SymbolInfo> {
    assert!(
        !options.hierarchical || options.query_string.is_none(),
        "Cannot use hierarchical mode with a query string"
    );

    let parsed = parsed_module(db, file);
    let module = parsed.load(db);

    let mut visitor = SymbolVisitor::new(options);
    visitor.visit_body(&module.syntax().body);
    visitor.symbols
}

struct SymbolVisitor<'a> {
    symbols: Vec<SymbolInfo>,
    symbol_stack: Vec<SymbolInfo>,
    /// Track if we're currently inside a function (to exclude local variables)
    in_function: bool,
    /// Options controlling symbol collection
    options: &'a SymbolsOptions,
}

impl<'a> SymbolVisitor<'a> {
    fn new(options: &'a SymbolsOptions) -> Self {
        Self {
            symbols: Vec::new(),
            symbol_stack: Vec::new(),
            in_function: false,
            options,
        }
    }

    fn visit_body(&mut self, body: &[Stmt]) {
        for stmt in body {
            self.visit_stmt(stmt);
        }
    }

    fn add_symbol(&mut self, symbol: SymbolInfo) {
        // Filter by query string if provided
        if let Some(ref query) = self.options.query_string {
            if !Self::is_pattern_in_symbol(query, &symbol.name) {
                return;
            }
        }

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

    /// Returns true if symbol name contains all characters in the query
    /// string in order. The comparison is case insensitive.
    fn is_pattern_in_symbol(query_string: &str, symbol_name: &str) -> bool {
        let typed_lower = query_string.to_lowercase();
        let symbol_lower = symbol_name.to_lowercase();
        let typed_chars: Vec<char> = typed_lower.chars().collect();
        let symbol_chars: Vec<char> = symbol_lower.chars().collect();

        let mut typed_pos = 0;
        let mut symbol_pos = 0;

        while typed_pos < typed_chars.len() && symbol_pos < symbol_chars.len() {
            if typed_chars[typed_pos] == symbol_chars[symbol_pos] {
                typed_pos += 1;
            }
            symbol_pos += 1;
        }

        typed_pos == typed_chars.len()
    }
}

impl SourceOrderVisitor<'_> for SymbolVisitor<'_> {
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
