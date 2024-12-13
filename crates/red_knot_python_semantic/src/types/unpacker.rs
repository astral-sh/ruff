use std::borrow::Cow;

use ruff_db::files::File;
use ruff_python_ast::{self as ast, AnyNodeRef};
use rustc_hash::FxHashMap;

use crate::semantic_index::ast_ids::{HasScopedExpressionId, ScopedExpressionId};
use crate::semantic_index::symbol::ScopeId;
use crate::types::{todo_type, Type, TypeCheckDiagnostics};
use crate::Db;

use super::context::{InferContext, WithDiagnostics};

/// Unpacks the value expression type to their respective targets.
pub(crate) struct Unpacker<'db> {
    context: InferContext<'db>,
    targets: FxHashMap<ScopedExpressionId, Type<'db>>,
}

impl<'db> Unpacker<'db> {
    pub(crate) fn new(db: &'db dyn Db, file: File) -> Self {
        Self {
            context: InferContext::new(db, file),
            targets: FxHashMap::default(),
        }
    }

    fn db(&self) -> &'db dyn Db {
        self.context.db()
    }

    pub(crate) fn unpack(&mut self, target: &ast::Expr, value_ty: Type<'db>, scope: ScopeId<'db>) {
        match target {
            ast::Expr::Name(target_name) => {
                self.targets
                    .insert(target_name.scoped_expression_id(self.db(), scope), value_ty);
            }
            ast::Expr::Starred(ast::ExprStarred { value, .. }) => {
                self.unpack(value, value_ty, scope);
            }
            ast::Expr::List(ast::ExprList { elts, .. })
            | ast::Expr::Tuple(ast::ExprTuple { elts, .. }) => match value_ty {
                Type::Tuple(tuple_ty) => {
                    let starred_index = elts.iter().position(ast::Expr::is_starred_expr);

                    let element_types = if let Some(starred_index) = starred_index {
                        if tuple_ty.len(self.db()) >= elts.len() - 1 {
                            let mut element_types = Vec::with_capacity(elts.len());
                            element_types.extend_from_slice(
                                // SAFETY: Safe because of the length check above.
                                &tuple_ty.elements(self.db())[..starred_index],
                            );

                            // E.g., in `(a, *b, c, d) = ...`, the index of starred element `b`
                            // is 1 and the remaining elements after that are 2.
                            let remaining = elts.len() - (starred_index + 1);
                            // This index represents the type of the last element that belongs
                            // to the starred expression, in an exclusive manner.
                            let starred_end_index = tuple_ty.len(self.db()) - remaining;
                            // SAFETY: Safe because of the length check above.
                            let _starred_element_types =
                                &tuple_ty.elements(self.db())[starred_index..starred_end_index];
                            // TODO: Combine the types into a list type. If the
                            // starred_element_types is empty, then it should be `List[Any]`.
                            // combine_types(starred_element_types);
                            element_types.push(todo_type!("starred unpacking"));

                            element_types.extend_from_slice(
                                // SAFETY: Safe because of the length check above.
                                &tuple_ty.elements(self.db())[starred_end_index..],
                            );
                            Cow::Owned(element_types)
                        } else {
                            let mut element_types = tuple_ty.elements(self.db()).to_vec();
                            // Subtract 1 to insert the starred expression type at the correct
                            // index.
                            element_types.resize(elts.len() - 1, Type::Unknown);
                            // TODO: This should be `list[Unknown]`
                            element_types.insert(starred_index, todo_type!("starred unpacking"));
                            Cow::Owned(element_types)
                        }
                    } else {
                        Cow::Borrowed(tuple_ty.elements(self.db()).as_ref())
                    };

                    for (index, element) in elts.iter().enumerate() {
                        self.unpack(
                            element,
                            element_types.get(index).copied().unwrap_or(Type::Unknown),
                            scope,
                        );
                    }
                }
                Type::StringLiteral(string_literal_ty) => {
                    // Deconstruct the string literal to delegate the inference back to the
                    // tuple type for correct handling of starred expressions. We could go
                    // further and deconstruct to an array of `StringLiteral` with each
                    // individual character, instead of just an array of `LiteralString`, but
                    // there would be a cost and it's not clear that it's worth it.
                    let value_ty = Type::tuple(
                        self.db(),
                        std::iter::repeat(Type::LiteralString)
                            .take(string_literal_ty.python_len(self.db())),
                    );
                    self.unpack(target, value_ty, scope);
                }
                _ => {
                    let value_ty = if value_ty.is_literal_string() {
                        Type::LiteralString
                    } else {
                        value_ty
                            .iterate(self.db())
                            .unwrap_with_diagnostic(&self.context, AnyNodeRef::from(target))
                    };
                    for element in elts {
                        self.unpack(element, value_ty, scope);
                    }
                }
            },
            _ => {}
        }
    }

    pub(crate) fn finish(mut self) -> UnpackResult<'db> {
        self.targets.shrink_to_fit();
        UnpackResult {
            diagnostics: self.context.finish(),
            targets: self.targets,
        }
    }
}

#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct UnpackResult<'db> {
    targets: FxHashMap<ScopedExpressionId, Type<'db>>,
    diagnostics: TypeCheckDiagnostics,
}

impl<'db> UnpackResult<'db> {
    pub(crate) fn get(&self, expr_id: ScopedExpressionId) -> Option<Type<'db>> {
        self.targets.get(&expr_id).copied()
    }
}

impl WithDiagnostics for UnpackResult<'_> {
    fn diagnostics(&self) -> &TypeCheckDiagnostics {
        &self.diagnostics
    }
}
