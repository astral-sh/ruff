use ruff_python_ast::{self as ast, AnyNodeRef};

use super::TypeInferenceBuilder;
use crate::Db;
use crate::types::constraints::ConstraintSetBuilder;
use crate::types::diagnostic::{DIVISION_BY_ZERO, report_unsupported_binary_operation};
use crate::types::{
    DynamicType, InternedConstraintSet, KnownClass, KnownInstanceType, LiteralValueTypeKind,
    MemberLookupPolicy, Type, TypeContext, TypeVarBoundOrConstraints, TypeVarConstraints,
    UnionBuilder, UnionTypeInstance,
};
use ruff_python_ast::PythonVersion;

use crate::Program;

impl<'db> TypeInferenceBuilder<'db, '_> {
    pub(super) fn infer_binary_expression(
        &mut self,
        binary: &ast::ExprBinOp,
        tcx: TypeContext<'db>,
    ) -> Type<'db> {
        if tcx.is_typealias() {
            return self.infer_pep_604_union_type_alias(binary, tcx);
        }

        let ast::ExprBinOp {
            left,
            op,
            right,
            range: _,
            node_index: _,
        } = binary;

        let left_ty = self.infer_expression(left, TypeContext::default());
        let right_ty = self.infer_expression(right, TypeContext::default());

        self.infer_binary_expression_type(binary.into(), false, left_ty, right_ty, *op)
            .unwrap_or_else(|| {
                report_unsupported_binary_operation(&self.context, binary, left_ty, right_ty, *op);
                Type::unknown()
            })
    }

    fn infer_pep_604_union_type_alias(
        &mut self,
        node: &ast::ExprBinOp,
        tcx: TypeContext<'db>,
    ) -> Type<'db> {
        let db = self.db();
        let ast::ExprBinOp {
            left,
            op,
            right,
            range: _,
            node_index: _,
        } = node;

        if *op != ast::Operator::BitOr {
            // TODO diagnostic?
            return Type::unknown();
        }

        let left_ty = self.infer_expression(left, tcx);
        let right_ty = self.infer_expression(right, tcx);

        // TODO this is overly aggressive; if the operands' `__or__` does not actually return a
        // `UnionType` at runtime, we should ideally not infer one here. But this is unlikely to be
        // a problem in practice: it would require someone having an explicitly annotated
        // `TypeAlias`, which uses `X | Y` syntax, where the returned type is not actually a union.
        // And attempting to enforce this more tightly showed a lot of potential false positives in
        // the ecosystem.
        if left_ty.is_equivalent_to(db, right_ty) {
            left_ty
        } else {
            UnionTypeInstance::from_value_expression_types(
                db,
                [left_ty, right_ty],
                self.scope(),
                self.typevar_binding_context,
            )
        }
    }

    /// Maps an operation over each constraint of a constrained `TypeVar`.
    ///
    /// Returns the original `TypeVar` if each result is equivalent to its input constraint;
    /// otherwise returns the union of all results.
    pub(super) fn map_constrained_typevar_constraints(
        db: &'db dyn Db,
        typevar: Type<'db>,
        constraints: TypeVarConstraints<'db>,
        mut op: impl FnMut(Type<'db>) -> Option<Type<'db>>,
    ) -> Option<Type<'db>> {
        let mut builder = UnionBuilder::new(db);
        let mut any_different = false;

        for constraint in constraints.elements(db) {
            let result = op(*constraint)?;
            if !result.is_equivalent_to(db, *constraint) {
                any_different = true;
            }
            builder = builder.add(result);
        }

        Some(if any_different {
            builder.build()
        } else {
            typevar
        })
    }

    pub(super) fn infer_binary_expression_type(
        &mut self,
        node: AnyNodeRef<'_>,
        mut emitted_division_by_zero_diagnostic: bool,
        left_ty: Type<'db>,
        right_ty: Type<'db>,
        op: ast::Operator,
    ) -> Option<Type<'db>> {
        let db = self.db();

        // Check for division by zero; this doesn't change the inferred type for the expression, but
        // may emit a diagnostic
        if !emitted_division_by_zero_diagnostic
            && matches!(
                op,
                ast::Operator::Div | ast::Operator::FloorDiv | ast::Operator::Mod
            )
            && right_ty.as_literal_value().is_some_and(|literal| {
                literal.as_bool() == Some(false) || literal.as_int() == Some(0)
            })
        {
            emitted_division_by_zero_diagnostic = self.check_division_by_zero(node, op, left_ty);
        }

        let pep_604_unions_allowed = || {
            Program::get(db).python_version(db) >= PythonVersion::PY310
                || self.file().is_stub(db)
                || self.scope().scope(db).in_type_checking_block()
        };

        match (left_ty, right_ty, op) {
            (Type::Union(lhs_union), rhs, _) => lhs_union.try_map(db, |lhs_element| {
                self.infer_binary_expression_type(
                    node,
                    emitted_division_by_zero_diagnostic,
                    *lhs_element,
                    rhs,
                    op,
                )
            }),
            (lhs, Type::Union(rhs_union), _) => rhs_union.try_map(db, |rhs_element| {
                self.infer_binary_expression_type(
                    node,
                    emitted_division_by_zero_diagnostic,
                    lhs,
                    *rhs_element,
                    op,
                )
            }),

            (Type::TypeAlias(alias), rhs, _) => self.infer_binary_expression_type(
                node,
                emitted_division_by_zero_diagnostic,
                alias.value_type(db),
                rhs,
                op,
            ),

            (lhs, Type::TypeAlias(alias), _) => self.infer_binary_expression_type(
                node,
                emitted_division_by_zero_diagnostic,
                lhs,
                alias.value_type(db),
                op,
            ),

            // Non-todo Anys take precedence over Todos (as if we fix this `Todo` in the future,
            // the result would then become Any or Unknown, respectively).
            (div @ Type::Dynamic(DynamicType::Divergent(_)), _, _)
            | (_, div @ Type::Dynamic(DynamicType::Divergent(_)), _) => Some(div),

            (any @ Type::Dynamic(DynamicType::Any), _, _)
            | (_, any @ Type::Dynamic(DynamicType::Any), _) => Some(any),

            (unknown @ Type::Dynamic(DynamicType::Unknown), _, _)
            | (_, unknown @ Type::Dynamic(DynamicType::Unknown), _) => Some(unknown),

            (unknown @ Type::Dynamic(DynamicType::UnknownGeneric(_)), _, _)
            | (_, unknown @ Type::Dynamic(DynamicType::UnknownGeneric(_)), _) => Some(unknown),

            (typevar @ Type::Dynamic(DynamicType::UnspecializedTypeVar), _, _)
            | (_, typevar @ Type::Dynamic(DynamicType::UnspecializedTypeVar), _) => Some(typevar),

            (todo @ Type::Dynamic(DynamicType::TodoFunctionalTypedDict), _, _)
            | (_, todo @ Type::Dynamic(DynamicType::TodoFunctionalTypedDict), _) => Some(todo),

            // When both operands are the same constrained TypeVar (e.g., `T: (int, str)`),
            // we check if the operation is valid for each constraint paired with itself.
            // This is different from treating it as a union, where we'd check all combinations.
            // For example, `T + T` where `T: (int, str)` should check `int + int` and `str + str`,
            // not `int + str` which would fail.
            //
            // If each constraint's operation returns the same type as the constraint (e.g.,
            // `int + int -> int`), we return the TypeVar to preserve the generic relationship.
            // Otherwise, we return the union of the return types.
            //
            // TODO: We expect to replace this with more general support for handling constrained TypeVars
            // in arbitrary method/function calls.
            (Type::TypeVar(left_tvar), Type::TypeVar(right_tvar), _)
                if left_tvar.identity(db) == right_tvar.identity(db) =>
            {
                match left_tvar.typevar(db).bound_or_constraints(db) {
                    Some(TypeVarBoundOrConstraints::Constraints(constraints)) => {
                        Self::map_constrained_typevar_constraints(
                            db,
                            left_ty,
                            constraints,
                            |constraint| {
                                self.infer_binary_expression_type(
                                    node,
                                    emitted_division_by_zero_diagnostic,
                                    constraint,
                                    constraint,
                                    op,
                                )
                            },
                        )
                    }
                    // For bounded TypeVars or unconstrained TypeVars, fall through to the default handling.
                    _ => Type::try_call_bin_op(db, left_ty, op, right_ty)
                        .map(|outcome| outcome.return_type(db))
                        .ok(),
                }
            }

            // When the left operand is a constrained TypeVar (e.g., `T: (int, float)`) and the
            // right operand is not a TypeVar, we check if each constraint supports the operation
            // with the right operand. For example, `T * 2` where `T: (int, float)` should check
            // `int * 2` and `float * 2`, both of which work.
            //
            // TODO: We expect to replace this with more general support once we migrate to the new
            // solver.
            (Type::TypeVar(left_tvar), rhs, _) if !rhs.is_type_var() => {
                match left_tvar.typevar(db).bound_or_constraints(db) {
                    Some(TypeVarBoundOrConstraints::Constraints(constraints)) => {
                        Self::map_constrained_typevar_constraints(
                            db,
                            left_ty,
                            constraints,
                            |constraint| {
                                self.infer_binary_expression_type(
                                    node,
                                    emitted_division_by_zero_diagnostic,
                                    constraint,
                                    rhs,
                                    op,
                                )
                            },
                        )
                    }
                    // For bounded TypeVars or unconstrained TypeVars, fall through to the default handling.
                    _ => Type::try_call_bin_op(db, left_ty, op, right_ty)
                        .map(|outcome| outcome.return_type(db))
                        .ok(),
                }
            }

            // When the right operand is a constrained TypeVar and the left operand is not a TypeVar,
            // we check if each constraint supports the operation with the left operand.
            (lhs, Type::TypeVar(right_tvar), _) if !lhs.is_type_var() => {
                match right_tvar.typevar(db).bound_or_constraints(db) {
                    Some(TypeVarBoundOrConstraints::Constraints(constraints)) => {
                        Self::map_constrained_typevar_constraints(
                            db,
                            right_ty,
                            constraints,
                            |constraint| {
                                self.infer_binary_expression_type(
                                    node,
                                    emitted_division_by_zero_diagnostic,
                                    lhs,
                                    constraint,
                                    op,
                                )
                            },
                        )
                    }
                    // For bounded TypeVars or unconstrained TypeVars, fall through to the default handling.
                    _ => Type::try_call_bin_op(db, left_ty, op, right_ty)
                        .map(|outcome| outcome.return_type(db))
                        .ok(),
                }
            }

            // `try_call_bin_op` works for almost all `NewType`s, but not for `NewType`s of `float`
            // and `complex`, where the concrete base type is a union. In that case it turns out
            // the `self` types of the dunder methods in typeshed don't match, because they don't
            // get the same `int | float` and `int | float | complex` special treatment that the
            // positional arguments get. In those cases we need to explicitly delegate to the base
            // type, so that it hits the `Type::Union` branches above.
            (Type::NewTypeInstance(newtype), rhs, _) => {
                Type::try_call_bin_op(db, left_ty, op, right_ty)
                    .map(|outcome| outcome.return_type(db))
                    .ok()
                    .or_else(|| {
                        self.infer_binary_expression_type(
                            node,
                            emitted_division_by_zero_diagnostic,
                            newtype.concrete_base_type(db),
                            rhs,
                            op,
                        )
                    })
            }
            (lhs, Type::NewTypeInstance(newtype), _) => {
                Type::try_call_bin_op(db, left_ty, op, right_ty)
                    .map(|outcome| outcome.return_type(db))
                    .ok()
                    .or_else(|| {
                        self.infer_binary_expression_type(
                            node,
                            emitted_division_by_zero_diagnostic,
                            lhs,
                            newtype.concrete_base_type(db),
                            op,
                        )
                    })
            }

            (
                todo @ Type::Dynamic(
                    DynamicType::Todo(_)
                    | DynamicType::TodoUnpack
                    | DynamicType::TodoStarredExpression
                    | DynamicType::TodoTypeVarTuple,
                ),
                _,
                _,
            )
            | (
                _,
                todo @ Type::Dynamic(
                    DynamicType::Todo(_)
                    | DynamicType::TodoUnpack
                    | DynamicType::TodoStarredExpression
                    | DynamicType::TodoTypeVarTuple,
                ),
                _,
            ) => Some(todo),

            (Type::Never, _, _) | (_, Type::Never, _) => Some(Type::Never),

            (Type::LiteralValue(left), Type::LiteralValue(right), _) => {
                match (left.kind(), right.kind(), op) {
                    (
                        LiteralValueTypeKind::Int(n),
                        LiteralValueTypeKind::Int(m),
                        ast::Operator::Add,
                    ) => Some(
                        n.as_i64()
                            .checked_add(m.as_i64())
                            .map(Type::int_literal)
                            .unwrap_or_else(|| KnownClass::Int.to_instance(db)),
                    ),

                    (
                        LiteralValueTypeKind::Int(n),
                        LiteralValueTypeKind::Int(m),
                        ast::Operator::Sub,
                    ) => Some(
                        n.as_i64()
                            .checked_sub(m.as_i64())
                            .map(Type::int_literal)
                            .unwrap_or_else(|| KnownClass::Int.to_instance(db)),
                    ),

                    (
                        LiteralValueTypeKind::Int(n),
                        LiteralValueTypeKind::Int(m),
                        ast::Operator::Mult,
                    ) => Some(
                        n.as_i64()
                            .checked_mul(m.as_i64())
                            .map(Type::int_literal)
                            .unwrap_or_else(|| KnownClass::Int.to_instance(db)),
                    ),

                    (
                        LiteralValueTypeKind::Int(_),
                        LiteralValueTypeKind::Int(_),
                        ast::Operator::Div,
                    ) => Some(KnownClass::Float.to_instance(db)),

                    (
                        LiteralValueTypeKind::Int(n),
                        LiteralValueTypeKind::Int(m),
                        ast::Operator::FloorDiv,
                    ) => Some({
                        let mut q = n.as_i64().checked_div(m.as_i64());
                        let r = n.as_i64().checked_rem(m.as_i64());
                        // Division works differently in Python than in Rust. If the result is negative and
                        // there is a remainder, the division rounds down (instead of towards zero):
                        if n.as_i64().is_negative() != m.as_i64().is_negative()
                            && r.unwrap_or(0) != 0
                        {
                            q = q.map(|q| q - 1);
                        }
                        q.map(Type::int_literal)
                            .unwrap_or_else(|| KnownClass::Int.to_instance(db))
                    }),

                    (
                        LiteralValueTypeKind::Int(n),
                        LiteralValueTypeKind::Int(m),
                        ast::Operator::Mod,
                    ) => Some({
                        let mut r = n.as_i64().checked_rem(m.as_i64());
                        // Division works differently in Python than in Rust. If the result is negative and
                        // there is a remainder, the division rounds down (instead of towards zero). Adjust
                        // the remainder to compensate so that q * m + r == n:
                        if n.as_i64().is_negative() != m.as_i64().is_negative()
                            && r.unwrap_or(0) != 0
                        {
                            r = r.map(|x| x + m.as_i64());
                        }
                        r.map(Type::int_literal)
                            .unwrap_or_else(|| KnownClass::Int.to_instance(db))
                    }),

                    (
                        LiteralValueTypeKind::Int(n),
                        LiteralValueTypeKind::Int(m),
                        ast::Operator::Pow,
                    ) => Some({
                        if m.as_i64() < 0 {
                            KnownClass::Float.to_instance(db)
                        } else {
                            u32::try_from(m.as_i64())
                                .ok()
                                .and_then(|m| n.as_i64().checked_pow(m))
                                .map(Type::int_literal)
                                .unwrap_or_else(|| KnownClass::Int.to_instance(db))
                        }
                    }),

                    (
                        LiteralValueTypeKind::Int(n),
                        LiteralValueTypeKind::Int(m),
                        ast::Operator::BitOr,
                    ) => Some(Type::int_literal(n.as_i64() | m.as_i64())),

                    (
                        LiteralValueTypeKind::Int(n),
                        LiteralValueTypeKind::Int(m),
                        ast::Operator::BitAnd,
                    ) => Some(Type::int_literal(n.as_i64() & m.as_i64())),

                    (
                        LiteralValueTypeKind::Int(n),
                        LiteralValueTypeKind::Int(m),
                        ast::Operator::BitXor,
                    ) => Some(Type::int_literal(n.as_i64() ^ m.as_i64())),

                    (
                        LiteralValueTypeKind::Bytes(lhs),
                        LiteralValueTypeKind::Bytes(rhs),
                        ast::Operator::Add,
                    ) => {
                        let bytes = [lhs.value(db), rhs.value(db)].concat();
                        Some(Type::bytes_literal(db, &bytes))
                    }

                    (
                        LiteralValueTypeKind::String(lhs),
                        LiteralValueTypeKind::String(rhs),
                        ast::Operator::Add,
                    ) => {
                        let lhs_value = lhs.value(db).to_string();
                        let rhs_value = rhs.value(db);
                        let ty =
                            if lhs_value.len() + rhs_value.len() <= Self::MAX_STRING_LITERAL_SIZE {
                                Type::string_literal(db, &(lhs_value + rhs_value))
                            } else {
                                Type::literal_string()
                            };
                        Some(ty)
                    }

                    (
                        LiteralValueTypeKind::String(_) | LiteralValueTypeKind::LiteralString,
                        LiteralValueTypeKind::String(_) | LiteralValueTypeKind::LiteralString,
                        ast::Operator::Add,
                    ) => Some(Type::literal_string()),

                    (
                        LiteralValueTypeKind::String(s),
                        LiteralValueTypeKind::Int(n),
                        ast::Operator::Mult,
                    )
                    | (
                        LiteralValueTypeKind::Int(n),
                        LiteralValueTypeKind::String(s),
                        ast::Operator::Mult,
                    ) => {
                        let ty = if n.as_i64() < 1 {
                            Type::string_literal(db, "")
                        } else if let Ok(n) = usize::try_from(n.as_i64())
                            && n.checked_mul(s.value(db).len()).is_some_and(|new_length| {
                                new_length <= Self::MAX_STRING_LITERAL_SIZE
                            })
                        {
                            let new_literal = s.value(db).repeat(n);
                            Type::string_literal(db, &new_literal)
                        } else {
                            Type::literal_string()
                        };
                        Some(ty)
                    }

                    (
                        LiteralValueTypeKind::LiteralString,
                        LiteralValueTypeKind::Int(n),
                        ast::Operator::Mult,
                    )
                    | (
                        LiteralValueTypeKind::Int(n),
                        LiteralValueTypeKind::LiteralString,
                        ast::Operator::Mult,
                    ) => {
                        let ty = if n.as_i64() < 1 {
                            Type::string_literal(db, "")
                        } else {
                            Type::literal_string()
                        };
                        Some(ty)
                    }

                    (
                        LiteralValueTypeKind::Bool(b1),
                        LiteralValueTypeKind::Bool(b2),
                        ast::Operator::BitOr,
                    ) => Some(Type::bool_literal(b1 | b2)),

                    (
                        LiteralValueTypeKind::Bool(b1),
                        LiteralValueTypeKind::Bool(b2),
                        ast::Operator::BitAnd,
                    ) => Some(Type::bool_literal(b1 & b2)),

                    (
                        LiteralValueTypeKind::Bool(b1),
                        LiteralValueTypeKind::Bool(b2),
                        ast::Operator::BitXor,
                    ) => Some(Type::bool_literal(b1 ^ b2)),

                    (
                        LiteralValueTypeKind::Bool(b1),
                        LiteralValueTypeKind::Bool(_) | LiteralValueTypeKind::Int(_),
                        op,
                    ) => self.infer_binary_expression_type(
                        node,
                        emitted_division_by_zero_diagnostic,
                        Type::int_literal(i64::from(b1)),
                        right_ty,
                        op,
                    ),

                    (LiteralValueTypeKind::Int(_), LiteralValueTypeKind::Bool(b2), op) => self
                        .infer_binary_expression_type(
                            node,
                            emitted_division_by_zero_diagnostic,
                            left_ty,
                            Type::int_literal(i64::from(b2)),
                            op,
                        ),

                    (
                        LiteralValueTypeKind::Int(n),
                        LiteralValueTypeKind::Int(m),
                        ast::Operator::LShift,
                    ) if n.as_i64() == 0 && m.as_i64() >= 0 => Some(Type::int_literal(0)),

                    (
                        LiteralValueTypeKind::Int(n),
                        LiteralValueTypeKind::Int(m),
                        ast::Operator::LShift,
                    ) => {
                        let n = n.as_i64();

                        // An additional overflow check beyond `checked_shl` is necessary
                        // here, because `checked_shl` only rejects shift amounts >= 64;
                        // it does not detect when significant bits are shifted into (or
                        // past) the sign bit. For example, `1i64.checked_shl(63)` returns
                        // `Some(i64::MIN)`, but Python's `1 << 63` is a large positive int.
                        //
                        // We compute the "headroom": the number of redundant sign-extension
                        // bits minus one (for the sign bit itself). A shift is safe iff
                        // `m <= headroom`.
                        let headroom = if n >= 0 {
                            n.leading_zeros().saturating_sub(1)
                        } else {
                            n.leading_ones().saturating_sub(1)
                        };
                        Some(
                            u32::try_from(m.as_i64())
                                .ok()
                                .filter(|&m| m <= headroom)
                                .and_then(|m| n.checked_shl(m))
                                .map(Type::int_literal)
                                .unwrap_or_else(|| KnownClass::Int.to_instance(db)),
                        )
                    }

                    (
                        LiteralValueTypeKind::Int(n),
                        LiteralValueTypeKind::Int(m),
                        ast::Operator::RShift,
                    ) => {
                        let n = n.as_i64();
                        let result = match u32::try_from(m.as_i64()) {
                            Ok(m) => Type::int_literal(n >> m.clamp(0, 63)),
                            Err(_) if m.as_i64() > 0 => {
                                Type::int_literal(if n >= 0 { 0 } else { -1 })
                            }
                            Err(_) => KnownClass::Int.to_instance(db),
                        };
                        Some(result)
                    }

                    _ => Type::try_call_bin_op(db, left_ty, op, right_ty)
                        .map(|outcome| outcome.return_type(db))
                        .ok(),
                }
            }

            (
                Type::KnownInstance(KnownInstanceType::ConstraintSet(left)),
                Type::KnownInstance(KnownInstanceType::ConstraintSet(right)),
                ast::Operator::BitAnd,
            ) => {
                let constraints = ConstraintSetBuilder::new();
                let result = constraints.into_owned(|constraints| {
                    let left = constraints.load(left.constraints(db));
                    let right = constraints.load(right.constraints(db));
                    left.and(db, constraints, || right)
                });
                Some(Type::KnownInstance(KnownInstanceType::ConstraintSet(
                    InternedConstraintSet::new(db, result),
                )))
            }

            (
                Type::KnownInstance(KnownInstanceType::ConstraintSet(left)),
                Type::KnownInstance(KnownInstanceType::ConstraintSet(right)),
                ast::Operator::BitOr,
            ) => {
                let constraints = ConstraintSetBuilder::new();
                let result = constraints.into_owned(|constraints| {
                    let left = constraints.load(left.constraints(db));
                    let right = constraints.load(right.constraints(db));
                    left.or(db, constraints, || right)
                });
                Some(Type::KnownInstance(KnownInstanceType::ConstraintSet(
                    InternedConstraintSet::new(db, result),
                )))
            }

            // PEP 604-style union types using the `|` operator.
            (
                Type::ClassLiteral(..)
                | Type::SubclassOf(..)
                | Type::GenericAlias(..)
                | Type::SpecialForm(_)
                | Type::KnownInstance(
                    KnownInstanceType::UnionType(_)
                    | KnownInstanceType::Literal(_)
                    | KnownInstanceType::Annotated(_)
                    | KnownInstanceType::TypeGenericAlias(_)
                    | KnownInstanceType::Callable(_)
                    | KnownInstanceType::TypeVar(_),
                ),
                Type::ClassLiteral(..)
                | Type::SubclassOf(..)
                | Type::GenericAlias(..)
                | Type::SpecialForm(_)
                | Type::KnownInstance(
                    KnownInstanceType::UnionType(_)
                    | KnownInstanceType::Literal(_)
                    | KnownInstanceType::Annotated(_)
                    | KnownInstanceType::TypeGenericAlias(_)
                    | KnownInstanceType::Callable(_)
                    | KnownInstanceType::TypeVar(_),
                ),
                ast::Operator::BitOr,
            ) if pep_604_unions_allowed() => {
                if left_ty.is_equivalent_to(db, right_ty) {
                    Some(left_ty)
                } else {
                    Some(UnionTypeInstance::from_value_expression_types(
                        db,
                        [left_ty, right_ty],
                        self.scope(),
                        self.typevar_binding_context,
                    ))
                }
            }
            (
                Type::ClassLiteral(..)
                | Type::SubclassOf(..)
                | Type::GenericAlias(..)
                | Type::KnownInstance(..)
                | Type::SpecialForm(..),
                Type::NominalInstance(instance),
                ast::Operator::BitOr,
            )
            | (
                Type::NominalInstance(instance),
                Type::ClassLiteral(..)
                | Type::SubclassOf(..)
                | Type::GenericAlias(..)
                | Type::KnownInstance(..)
                | Type::SpecialForm(..),
                ast::Operator::BitOr,
            ) if pep_604_unions_allowed() && instance.has_known_class(db, KnownClass::NoneType) => {
                Some(UnionTypeInstance::from_value_expression_types(
                    db,
                    [left_ty, right_ty],
                    self.scope(),
                    self.typevar_binding_context,
                ))
            }

            // We avoid calling `type.__(r)or__`, as typeshed annotates these methods as
            // accepting `Any` (since typeforms are inexpressable in the type system currently).
            // This means that many common errors would not be caught if we fell back to typeshed's stubs here.
            //
            // Note that if a class had a custom metaclass that overrode `__(r)or__`, we would also ignore
            // that custom method as we'd take one of the earlier branches.
            // This seems like it's probably rare enough that it's acceptable, however.
            (
                Type::ClassLiteral(..) | Type::GenericAlias(..) | Type::SubclassOf(..),
                _,
                ast::Operator::BitOr,
            )
            | (
                _,
                Type::ClassLiteral(..) | Type::GenericAlias(..) | Type::SubclassOf(..),
                ast::Operator::BitOr,
            ) if pep_604_unions_allowed() => Type::try_call_bin_op_with_policy(
                db,
                left_ty,
                ast::Operator::BitOr,
                right_ty,
                MemberLookupPolicy::META_CLASS_NO_TYPE_FALLBACK,
            )
            .ok()
            .map(|binding| binding.return_type(db)),

            // We've handled all of the special cases that we support for literals, so we need to
            // fall back on looking for dunder methods on one of the operand types.
            (
                Type::FunctionLiteral(_)
                | Type::Callable(..)
                | Type::BoundMethod(_)
                | Type::WrapperDescriptor(_)
                | Type::KnownBoundMethod(_)
                | Type::DataclassDecorator(_)
                | Type::DataclassTransformer(_)
                | Type::ModuleLiteral(_)
                | Type::ClassLiteral(_)
                | Type::GenericAlias(_)
                | Type::SubclassOf(_)
                | Type::NominalInstance(_)
                | Type::ProtocolInstance(_)
                | Type::SpecialForm(_)
                | Type::KnownInstance(_)
                | Type::PropertyInstance(_)
                | Type::Intersection(_)
                | Type::AlwaysTruthy
                | Type::AlwaysFalsy
                | Type::LiteralValue(_)
                | Type::BoundSuper(_)
                | Type::TypeVar(_)
                | Type::TypeIs(_)
                | Type::TypeGuard(_)
                | Type::TypedDict(_),
                Type::FunctionLiteral(_)
                | Type::Callable(..)
                | Type::BoundMethod(_)
                | Type::WrapperDescriptor(_)
                | Type::KnownBoundMethod(_)
                | Type::DataclassDecorator(_)
                | Type::DataclassTransformer(_)
                | Type::ModuleLiteral(_)
                | Type::ClassLiteral(_)
                | Type::GenericAlias(_)
                | Type::SubclassOf(_)
                | Type::NominalInstance(_)
                | Type::ProtocolInstance(_)
                | Type::SpecialForm(_)
                | Type::KnownInstance(_)
                | Type::PropertyInstance(_)
                | Type::Intersection(_)
                | Type::AlwaysTruthy
                | Type::AlwaysFalsy
                | Type::LiteralValue(_)
                | Type::BoundSuper(_)
                | Type::TypeVar(_)
                | Type::TypeIs(_)
                | Type::TypeGuard(_)
                | Type::TypedDict(_),
                op,
            ) => Type::try_call_bin_op(db, left_ty, op, right_ty)
                .map(|outcome| outcome.return_type(db))
                .ok(),
        }
    }

    /// Raise a diagnostic if the given type cannot be divided by zero.
    ///
    /// Expects the resolved type of the left side of the binary expression.
    fn check_division_by_zero(
        &mut self,
        node: AnyNodeRef<'_>,
        op: ast::Operator,
        left: Type<'db>,
    ) -> bool {
        let db = self.db();
        match left {
            Type::LiteralValue(literal)
                if matches!(
                    literal.kind(),
                    LiteralValueTypeKind::Bool(_) | LiteralValueTypeKind::Int(_)
                ) => {}
            Type::NominalInstance(instance)
                if matches!(
                    instance.known_class(db),
                    Some(KnownClass::Float | KnownClass::Int | KnownClass::Bool)
                ) => {}
            _ => return false,
        }

        let (op, by_zero) = match op {
            ast::Operator::Div => ("divide", "by zero"),
            ast::Operator::FloorDiv => ("floor divide", "by zero"),
            ast::Operator::Mod => ("reduce", "modulo zero"),
            _ => return false,
        };

        if let Some(builder) = self.context.report_lint(&DIVISION_BY_ZERO, node) {
            builder.into_diagnostic(format_args!(
                "Cannot {op} object of type `{}` {by_zero}",
                left.display(db)
            ));
        }

        true
    }
}
