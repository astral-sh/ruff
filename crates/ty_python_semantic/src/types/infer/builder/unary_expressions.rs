use ruff_python_ast::UnaryOp;

use super::TypeInferenceBuilder;
use crate::types::bool::BoolError;
use crate::types::call::{Bindings, CallArguments, CallDunderError};
use crate::types::constraints::ConstraintSetBuilder;
use crate::types::{
    InternedConstraintSet, KnownClass, KnownInstanceType, LiteralValueType, LiteralValueTypeKind,
    Type, TypeContext, TypeVarBoundOrConstraints,
};
use crate::{Db, ProgramEnvironment};

/// Diagnostics from unary evaluation, reported only at the original expression.
pub(in crate::types) enum UnaryOperationDiagnostic<'a, 'db> {
    DeprecatedBindings(&'a Bindings<'db>),
    InvertedBool(LiteralValueType<'db>),
    Unsupported {
        operand: Type<'db>,
        dunder: &'static str,
        error: Option<&'a CallDunderError<'db>>,
    },
    Bool(&'a BoolError<'db>),
}

impl<'a, 'db> UnaryOperationDiagnostic<'a, 'db> {
    fn deprecated_call(outcome: &'a Result<Bindings<'db>, CallDunderError<'db>>) -> Option<Self> {
        match outcome {
            Ok(bindings) => Some(Self::DeprecatedBindings(bindings)),
            // A method can be deprecated even if it is missing from some union members or
            // its signature rejects the implicit call.
            Err(
                CallDunderError::PossiblyUnbound { bindings, .. }
                | CallDunderError::CallError(_, bindings, _),
            ) => Some(Self::DeprecatedBindings(bindings)),
            Err(CallDunderError::MethodNotAvailable) => None,
        }
    }
}

impl<'db> Type<'db> {
    /// Evaluate a unary operation on an inferred operand. An error contains its fallback type.
    /// The diagnostic callback lets constraint solving reuse evaluation without emitting diagnostics.
    pub(in crate::types) fn try_unary_operation(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        op: UnaryOp,
        report: &mut dyn FnMut(UnaryOperationDiagnostic<'_, 'db>),
    ) -> Result<Type<'db>, Type<'db>> {
        match self {
            Type::Divergent(_) | Type::Never => return Ok(self),
            Type::TypeAlias(alias) => {
                return alias
                    .value_type(db)
                    .try_unary_operation(db, env, op, report);
            }
            _ => {}
        }

        let dunder = match op {
            UnaryOp::Invert => "__invert__",
            UnaryOp::UAdd => "__pos__",
            UnaryOp::USub => "__neg__",
            UnaryOp::Not => {
                return match self.try_bool(db, env) {
                    Ok(truthiness) => Ok(Type::from_truthiness(db, env, truthiness.negate())),
                    Err(error) => {
                        report(UnaryOperationDiagnostic::Bool(&error));
                        Err(Type::from_truthiness(
                            db,
                            env,
                            error.fallback_truthiness().negate(),
                        ))
                    }
                };
            }
        };

        match (op, self) {
            (_, Type::Dynamic(_)) => return Ok(self),
            (UnaryOp::UAdd, Type::LiteralValue(literal)) => match literal.kind() {
                LiteralValueTypeKind::Int(value) => return Ok(Type::int_literal(value.as_i64())),
                LiteralValueTypeKind::Bool(value) => {
                    return Ok(Type::int_literal(i64::from(value)));
                }
                _ => {}
            },
            (UnaryOp::USub, Type::LiteralValue(literal)) => match literal.kind() {
                LiteralValueTypeKind::Int(value) => {
                    return Ok(value
                        .as_i64()
                        .checked_neg()
                        .map(Type::int_literal)
                        .unwrap_or_else(|| KnownClass::Int.to_instance(db, env)));
                }
                LiteralValueTypeKind::Bool(value) => {
                    return Ok(Type::int_literal(-i64::from(value)));
                }
                _ => {}
            },
            (UnaryOp::Invert, Type::LiteralValue(literal)) => match literal.kind() {
                LiteralValueTypeKind::Int(value) => return Ok(Type::int_literal(!value.as_i64())),
                LiteralValueTypeKind::Bool(value) => {
                    report(UnaryOperationDiagnostic::InvertedBool(literal));
                    return Ok(Type::int_literal(!i64::from(value)));
                }
                _ => {}
            },
            (UnaryOp::Invert, Type::KnownInstance(KnownInstanceType::ConstraintSet(set))) => {
                let constraints = ConstraintSetBuilder::new();
                let result = constraints.into_owned(|constraints| {
                    let set = constraints.load(db, env, set.constraints(db));
                    set.negate(db, constraints)
                });
                return Ok(Type::KnownInstance(KnownInstanceType::ConstraintSet(
                    InternedConstraintSet::new(db, result),
                )));
            }
            // Handle constrained TypeVars specially: check each constraint individually.
            // TODO: Replace this with general support from the constraint solver.
            (_, Type::TypeVar(tvar)) => match tvar.typevar(db).bound_or_constraints(db, env) {
                Some(TypeVarBoundOrConstraints::Constraints(constraints)) => {
                    // Inspect every constraint so deprecation reporting does not depend on
                    // whether an earlier constraint fails.
                    let outcomes: Vec<_> = constraints
                        .elements(db)
                        .iter()
                        .map(|constraint| {
                            constraint.try_call_dunder(
                                db,
                                env,
                                dunder,
                                CallArguments::none(),
                                TypeContext::default(),
                            )
                        })
                        .collect();
                    for outcome in &outcomes {
                        if let Some(diagnostic) = UnaryOperationDiagnostic::deprecated_call(outcome)
                        {
                            report(diagnostic);
                        }
                    }
                    let mut outcomes = outcomes.into_iter();
                    let result = TypeInferenceBuilder::map_constrained_typevar_constraints(
                        db,
                        env,
                        self,
                        constraints,
                        |_constraint| Some(outcomes.next()?.ok()?.return_type(db, env)),
                    );
                    return match result {
                        Some(ty) => Ok(ty),
                        None => {
                            report(UnaryOperationDiagnostic::Unsupported {
                                operand: self,
                                dunder,
                                error: None,
                            });
                            Err(self
                                .try_call_dunder(
                                    db,
                                    env,
                                    dunder,
                                    CallArguments::none(),
                                    TypeContext::default(),
                                )
                                .map_or_else(
                                    |e| e.fallback_return_type(db, env),
                                    |b| b.return_type(db, env),
                                ))
                        }
                    };
                }
                // Delegate to the bound, including when it is a union such as `int | float`.
                Some(TypeVarBoundOrConstraints::UpperBound(bound)) => {
                    return bound.try_unary_operation(db, env, op, report);
                }
                None => {}
            },
            _ => {}
        }

        let outcome = self.try_call_dunder(
            db,
            env,
            dunder,
            CallArguments::none(),
            TypeContext::default(),
        );
        if let Some(diagnostic) = UnaryOperationDiagnostic::deprecated_call(&outcome) {
            report(diagnostic);
        }
        match outcome {
            Ok(bindings) => Ok(bindings.return_type(db, env)),
            Err(error) => {
                report(UnaryOperationDiagnostic::Unsupported {
                    operand: self,
                    dunder,
                    error: Some(&error),
                });
                Err(error.fallback_return_type(db, env))
            }
        }
    }
}
