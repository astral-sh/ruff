use std::fmt::Display;

use ast::{ExprContext, Ranged};
use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::identifier::Identifier;
use ruff_python_ast::{self as ast, Stmt};
use ruff_text_size::TextRange;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for the use of non-ASCII unicode character in identifiers.
///
/// ## Why is this bad?
/// Some non-ASCII unicode characters looks identical to ASCII characters,
/// that may cause confusion and hard to catch bugs in your code.
///
/// For example, the uppercase version of the Latin `b`, Greek `β` (Beta),
/// and Cyrillic `в` (Ve) often look identical: `B`, `Β` and `В`, respectively.
///
/// This allows identifiers to look the same for you, but not for Python. For example,
/// the following identifiers are all distinc:
///     - `scope` (Latin, ASCII-only)
///     - `scоpe` (with a Cyrillic `о`)
///     - `scοpe` (with a Greek `ο`)
///
/// ## Example
/// ```python
/// héllõ = 42
/// βeta = "beta"
/// ```
///
/// Use instead:
/// ```python
/// hello = 42
/// beta = "beta"
/// ```
/// ## References:
/// - https://peps.python.org/pep-0672/#confusing-features
#[violation]
pub struct NonAsciiName {
    node_type: NodeType,
    name: String,
}

impl Violation for NonAsciiName {
    #[derive_message_formats]
    fn message(&self) -> String {
        let NonAsciiName { node_type, name } = self;
        format!("{node_type} name `{name}` contains non-ASCII character, consider renaming it. (non-ascii-name)")
    }
}

/// PLC2401
pub(crate) fn non_ascii_name(checker: &mut Checker, stmt: &Stmt) {
    for non_ascii_name in get_stmt_names(stmt)
        .iter()
        .filter(|name| !name.name.is_ascii())
    {
        checker.diagnostics.push(Diagnostic::new(
            NonAsciiName {
                name: non_ascii_name.name.to_string(),
                node_type: non_ascii_name.node_type,
            },
            non_ascii_name.range,
        ));
    }
}

/// Return a `Vec<Name>` containing the identifiers present in any of the following
/// statements: `ClassDef`, `FunctionDef`, `Assign`, `AugAssing`, `AnnAssign`
/// or `Global`. Otherwise, return an empty `Vec`.
fn get_stmt_names(stmt: &Stmt) -> Vec<Name> {
    let mut stmt_names = Vec::new();

    match stmt {
        Stmt::ClassDef(ast::StmtClassDef { name, .. }) => stmt_names.push(Name {
            name: name.as_str(),
            range: stmt.identifier(),
            node_type: NodeType::Class,
        }),
        Stmt::FunctionDef(ast::StmtFunctionDef {
            name, parameters, ..
        }) => {
            stmt_names.push(Name {
                name: name.as_str(),
                range: stmt.identifier(),
                node_type: NodeType::Function,
            });

            stmt_names.extend(
                parameters
                    .args
                    .iter()
                    .chain(parameters.kwonlyargs.iter())
                    .chain(parameters.posonlyargs.iter())
                    .map(|arg| {
                        let param = arg.as_parameter();
                        Name {
                            name: param.name.as_str(),
                            range: param.range,
                            node_type: NodeType::Argument,
                        }
                    }),
            );

            if let Some(kwarg) = &parameters.kwarg {
                stmt_names.push(Name {
                    name: kwarg.name.as_str(),
                    range: kwarg.range,
                    node_type: NodeType::Argument,
                });
            }

            if let Some(vararg) = &parameters.vararg {
                stmt_names.push(Name {
                    name: vararg.name.as_str(),
                    range: vararg.range,
                    node_type: NodeType::Argument,
                });
            }
        }
        Stmt::Assign(ast::StmtAssign { targets, .. }) => {
            for target in targets {
                if let Some(ast::ExprAttribute { attr, ctx, .. }) = target.as_attribute_expr() {
                    if *ctx != ExprContext::Store {
                        continue;
                    }

                    stmt_names.push(Name {
                        name: attr.as_str(),
                        range: attr.range(),
                        node_type: NodeType::Attribute,
                    });
                }
                if let Some(tuple) = target.as_tuple_expr() {
                    stmt_names.extend(tuple.elts.iter().filter_map(|element| {
                        element.as_name_expr().map(|name| Name {
                            name: &name.id,
                            range: name.range,
                            node_type: NodeType::Assign,
                        })
                    }));
                }
                if let Some(name) = target.as_name_expr() {
                    stmt_names.push(Name {
                        name: &name.id,
                        range: name.range,
                        node_type: NodeType::Assign,
                    });
                }
            }
        }
        Stmt::AugAssign(ast::StmtAugAssign { target, .. })
        | Stmt::AnnAssign(ast::StmtAnnAssign { target, .. }) => {
            let Some(name) = target.as_name_expr() else {
                return Vec::new();
            };

            stmt_names.push(Name {
                name: &name.id,
                range: name.range,
                node_type: NodeType::Assign,
            });
        }
        Stmt::Global(ast::StmtGlobal { names, .. }) => {
            stmt_names.extend(names.iter().map(|name| Name {
                name: name.as_str(),
                range: name.range(),
                node_type: NodeType::Constant,
            }));
        }
        _ => return Vec::new(),
    }

    stmt_names
}

struct Name<'a> {
    name: &'a str,
    range: TextRange,
    node_type: NodeType,
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
enum NodeType {
    Assign,
    Class,
    Function,
    Argument,
    Attribute,
    Constant,
}

impl Display for NodeType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NodeType::Assign => write!(f, "Variable"),
            NodeType::Class => write!(f, "Class"),
            NodeType::Function => write!(f, "Function"),
            NodeType::Argument => write!(f, "Argument"),
            NodeType::Attribute => write!(f, "Attribute"),
            NodeType::Constant => write!(f, "Constant"),
        }
    }
}
