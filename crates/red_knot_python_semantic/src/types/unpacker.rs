use std::borrow::Cow;
use std::cmp::Ordering;

use rustc_hash::FxHashMap;

use ruff_python_ast::{self as ast, AnyNodeRef};

use crate::semantic_index::ast_ids::{HasScopedExpressionId, ScopedExpressionId};
use crate::semantic_index::symbol::ScopeId;
use crate::types::{infer_expression_types, todo_type, Type, TypeCheckDiagnostics};
use crate::unpack::{UnpackKind, UnpackValue};
use crate::Db;

use super::context::InferContext;
use super::diagnostic::INVALID_ASSIGNMENT;
use super::{TupleType, UnionType};

/// Unpacks the value expression type to their respective targets.
pub(crate) struct Unpacker<'db> {
    context: InferContext<'db>,
    scope: ScopeId<'db>,
    targets: FxHashMap<ScopedExpressionId, Type<'db>>,
}

impl<'db> Unpacker<'db> {
    pub(crate) fn new(db: &'db dyn Db, scope: ScopeId<'db>) -> Self {
        Self {
            context: InferContext::new(db, scope),
            targets: FxHashMap::default(),
            scope,
        }
    }

    fn db(&self) -> &'db dyn Db {
        self.context.db()
    }

    /// Unpack the value to the target expression.
    pub(crate) fn unpack(&mut self, target: &ast::Expr, value: UnpackValue<'db>) {
        debug_assert!(
            matches!(target, ast::Expr::List(_) | ast::Expr::Tuple(_)),
            "Unpacking target must be a list or tuple expression"
        );

        let value_type = infer_expression_types(self.db(), value.expression())
            .expression_type(value.scoped_expression_id(self.db(), self.scope));

        let value_type = match value.kind() {
            UnpackKind::Assign => {
                if self.context.in_stub()
                    && value
                        .expression()
                        .node_ref(self.db())
                        .is_ellipsis_literal_expr()
                {
                    Type::unknown()
                } else {
                    value_type
                }
            }
            UnpackKind::Iterable => value_type.try_iterate(self.db()).unwrap_or_else(|err| {
                err.report_diagnostic(&self.context, value_type, value.as_any_node_ref(self.db()));
                err.fallback_element_type(self.db())
            }),
            UnpackKind::ContextManager => value_type.try_enter(self.db()).unwrap_or_else(|err| {
                err.report_diagnostic(&self.context, value_type, value.as_any_node_ref(self.db()));
                err.fallback_enter_type(self.db())
            }),
        };

        self.unpack_inner(target, value.as_any_node_ref(self.db()), value_type);
    }

    fn unpack_inner(
        &mut self,
        target: &ast::Expr,
        value_expr: AnyNodeRef<'db>,
        value_ty: Type<'db>,
    ) {
        match target {
            ast::Expr::Name(_) | ast::Expr::Attribute(_) => {
                self.targets
                    .insert(target.scoped_expression_id(self.db(), self.scope), value_ty);
            }
            ast::Expr::Starred(ast::ExprStarred { value, .. }) => {
                self.unpack_inner(value, value_expr, value_ty);
            }
            ast::Expr::List(ast::ExprList { elts, .. })
            | ast::Expr::Tuple(ast::ExprTuple { elts, .. }) => {
                // Initialize the vector of target types, one for each target.
                //
                // This is mainly useful for the union type where the target type at index `n` is
                // going to be a union of types from every union type element at index `n`.
                //
                // For example, if the type is `tuple[int, int] | tuple[int, str]` and the target
                // has two elements `(a, b)`, then
                // * The type of `a` will be a union of `int` and `int` which are at index 0 in the
                //   first and second tuple respectively which resolves to an `int`.
                // * Similarly, the type of `b` will be a union of `int` and `str` which are at
                //   index 1 in the first and second tuple respectively which will be `int | str`.
                let mut target_types = vec![vec![]; elts.len()];

                let unpack_types = match value_ty {
                    Type::Union(union_ty) => union_ty.elements(self.db()),
                    _ => std::slice::from_ref(&value_ty),
                };

                for ty in unpack_types.iter().copied() {
                    // Deconstruct certain types to delegate the inference back to the tuple type
                    // for correct handling of starred expressions.
                    let ty = match ty {
                        Type::StringLiteral(string_literal_ty) => {
                            // We could go further and deconstruct to an array of `StringLiteral`
                            // with each individual character, instead of just an array of
                            // `LiteralString`, but there would be a cost and it's not clear that
                            // it's worth it.
                            TupleType::from_elements(
                                self.db(),
                                std::iter::repeat_n(
                                    Type::LiteralString,
                                    string_literal_ty.python_len(self.db()),
                                ),
                            )
                        }
                        _ => ty,
                    };

                    if let Some(tuple_ty) = ty.into_tuple() {
                        let tuple_ty_elements = self.tuple_ty_elements(target, elts, tuple_ty);

                        let length_mismatch = match elts.len().cmp(&tuple_ty_elements.len()) {
                            Ordering::Less => {
                                self.context.report_lint(
                                    &INVALID_ASSIGNMENT,
                                    target,
                                    format_args!(
                                        "Too many values to unpack (expected {}, got {})",
                                        elts.len(),
                                        tuple_ty_elements.len()
                                    ),
                                );
                                true
                            }
                            Ordering::Greater => {
                                self.context.report_lint(
                                    &INVALID_ASSIGNMENT,
                                    target,
                                    format_args!(
                                        "Not enough values to unpack (expected {}, got {})",
                                        elts.len(),
                                        tuple_ty_elements.len()
                                    ),
                                );
                                true
                            }
                            Ordering::Equal => false,
                        };

                        for (index, ty) in tuple_ty_elements.iter().enumerate() {
                            if let Some(element_types) = target_types.get_mut(index) {
                                if length_mismatch {
                                    element_types.push(Type::unknown());
                                } else {
                                    element_types.push(*ty);
                                }
                            }
                        }
                    } else {
                        let ty = if ty.is_literal_string() {
                            Type::LiteralString
                        } else {
                            ty.try_iterate(self.db()).unwrap_or_else(|err| {
                                err.report_diagnostic(&self.context, ty, value_expr);
                                err.fallback_element_type(self.db())
                            })
                        };
                        for target_type in &mut target_types {
                            target_type.push(ty);
                        }
                    }
                }

                for (index, element) in elts.iter().enumerate() {
                    // SAFETY: `target_types` is initialized with the same length as `elts`.
                    let element_ty = match target_types[index].as_slice() {
                        [] => Type::unknown(),
                        types => UnionType::from_elements(self.db(), types),
                    };
                    self.unpack_inner(element, value_expr, element_ty);
                }
            }
            _ => {}
        }
    }

    /// Returns the [`Type`] elements inside the given [`TupleType`] taking into account that there
    /// can be a starred expression in the `elements`.
    fn tuple_ty_elements(
        &self,
        expr: &ast::Expr,
        targets: &[ast::Expr],
        tuple_ty: TupleType<'db>,
    ) -> Cow<'_, [Type<'db>]> {
        // If there is a starred expression, it will consume all of the types at that location.
        let Some(starred_index) = targets.iter().position(ast::Expr::is_starred_expr) else {
            // Otherwise, the types will be unpacked 1-1 to the targets.
            return Cow::Borrowed(tuple_ty.elements(self.db()).as_ref());
        };

        if tuple_ty.len(self.db()) >= targets.len() - 1 {
            // This branch is only taken when there are enough elements in the tuple type to
            // combine for the starred expression. So, the arithmetic and indexing operations are
            // safe to perform.
            let mut element_types = Vec::with_capacity(targets.len());

            // Insert all the elements before the starred expression.
            element_types.extend_from_slice(
                // SAFETY: Safe because of the length check above.
                &tuple_ty.elements(self.db())[..starred_index],
            );

            // The number of target expressions that are remaining after the starred expression.
            // For example, in `(a, *b, c, d) = ...`, the index of starred element `b` is 1 and the
            // remaining elements after that are 2.
            let remaining = targets.len() - (starred_index + 1);

            // This index represents the position of the last element that belongs to the starred
            // expression, in an exclusive manner. For example, in `(a, *b, c) = (1, 2, 3, 4)`, the
            // starred expression `b` will consume the elements `Literal[2]` and `Literal[3]` and
            // the index value would be 3.
            let starred_end_index = tuple_ty.len(self.db()) - remaining;

            // SAFETY: Safe because of the length check above.
            let _starred_element_types =
                &tuple_ty.elements(self.db())[starred_index..starred_end_index];
            // TODO: Combine the types into a list type. If the
            // starred_element_types is empty, then it should be `List[Any]`.
            // combine_types(starred_element_types);
            element_types.push(todo_type!("starred unpacking"));

            // Insert the types remaining that aren't consumed by the starred expression.
            element_types.extend_from_slice(
                // SAFETY: Safe because of the length check above.
                &tuple_ty.elements(self.db())[starred_end_index..],
            );

            Cow::Owned(element_types)
        } else {
            self.context.report_lint(
                &INVALID_ASSIGNMENT,
                expr,
                format_args!(
                    "Not enough values to unpack (expected {} or more, got {})",
                    targets.len() - 1,
                    tuple_ty.len(self.db())
                ),
            );

            Cow::Owned(vec![Type::unknown(); targets.len()])
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

#[derive(Debug, Default, PartialEq, Eq, salsa::Update)]
pub(crate) struct UnpackResult<'db> {
    targets: FxHashMap<ScopedExpressionId, Type<'db>>,
    diagnostics: TypeCheckDiagnostics,
}

impl<'db> UnpackResult<'db> {
    /// Returns the inferred type for a given sub-expression of the left-hand side target
    /// of an unpacking assignment.
    ///
    /// Panics if a scoped expression ID is passed in that does not correspond to a sub-
    /// expression of the target.
    #[track_caller]
    pub(crate) fn expression_type(&self, expr_id: ScopedExpressionId) -> Type<'db> {
        self.targets[&expr_id]
    }

    /// Returns the diagnostics in this unpacking assignment.
    pub(crate) fn diagnostics(&self) -> &TypeCheckDiagnostics {
        &self.diagnostics
    }
}
