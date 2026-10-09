//! Guarded proofs for uniformly parameterized structural declarations.
//!
//! A repeated class name does not prove compatibility. A schema instead checks all member
//! requirements with shared rigid parameters, and permits recursive member obligations to
//! instantiate that same universally quantified hypothesis.

use ruff_python_ast::name::Name;

use super::{TypeRelation, TypeRelationChecker, TypeVarEvaluation};
use crate::types::constraints::{
    ConstraintProvenance, ConstraintSet, IteratorConstraintsExtension,
};
use crate::types::generics::GenericContext;
use crate::types::projection::{ObservedType, ObservedTypePair};
use crate::types::typevar::{BoundTypeVarInstance, TypeVarSet};
use crate::types::{
    ClassType, GenericAlias, ProtocolInstanceType, StaticClassLiteral, Type, TypeVarVariance,
};
use crate::{Db, ProgramEnvironment};

/// A universally quantified relation whose finite member requirements are being checked.
#[derive(Debug)]
pub(super) struct ParametricSchema<'db> {
    source: StaticClassLiteral<'db>,
    target: StaticClassLiteral<'db>,
    arity: usize,
    relation: TypeRelation,
    evaluation: TypeVarEvaluation,
    provenance: ConstraintProvenance,
    expensive: bool,
}

impl<'db> ParametricSchema<'db> {
    fn matches(
        &self,
        db: &'db dyn Db,
        checker: &TypeRelationChecker<'_, '_, 'db>,
        source: ClassType<'db>,
        target: ClassType<'db>,
    ) -> bool {
        if checker.relation != self.relation
            || checker.typevar_evaluation != self.evaluation
            || checker.provenance != self.provenance
            || checker.perform_expensive_checks != self.expensive
            || checker.constraints.relation_session().is_negative()
        {
            return false;
        }
        let (Some((source, Some(source_args))), Some((target, Some(target_args)))) = (
            source.static_class_literal(db),
            target.static_class_literal(db),
        ) else {
            return false;
        };
        // The two vectors are the checked substitution for the schema's common binders.
        // Matching declarations with unrelated arguments is never a schema instance.
        source == self.source
            && target == self.target
            && source_args.types(db).len() == self.arity
            && source_args.materialization_kind(db).is_none()
            && target_args.materialization_kind(db).is_none()
            && source_args.types(db) == target_args.types(db)
    }
}

fn unconstrained_parameters<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    context: GenericContext<'db>,
) -> bool {
    context.variables(db).all(|parameter| {
        !parameter.is_paramspec(db)
            && !parameter.is_typevartuple(db)
            && parameter
                .typevar(db)
                .bound_or_constraints(db, env)
                .is_none()
            && parameter.typevar(db).default_type(db, env).is_none()
    })
}

impl<'c, 'db> TypeRelationChecker<'_, 'c, 'db> {
    pub(super) fn try_parametric_protocol_relation(
        &self,
        db: &'db dyn Db,
        source: Type<'db>,
        target: ProtocolInstanceType<'db>,
    ) -> Option<ConstraintSet<'db, 'c>> {
        let source_class = source.as_nominal_instance()?.class(db, self.env);
        let target_class = *target.class_origin(db)?;
        let session = self.constraints.relation_session();
        if target.materialization_kind(db).is_some()
            || target.requires_operation_replay(db)
            || session.is_negative()
        {
            return None;
        }
        {
            let active = session.parametric_schemas.borrow();
            if active
                .iter()
                .any(|schema| schema.matches(db, self, source_class, target_class))
            {
                session
                    .assumption_epoch
                    .set(session.assumption_epoch.get().wrapping_add(1));
                return Some(self.always());
            }
            // A failed instance match cannot invent a second hypothesis by generalizing the
            // already-specialized descendant. The enclosing schema must establish every edge.
            if !active.is_empty() {
                return None;
            }
        }
        if !(self.relation.is_assignability() || self.relation.is_subtyping()) {
            return None;
        }
        let (source_origin, Some(source_args)) = source_class.static_class_literal(db)? else {
            return None;
        };
        let (target_origin, Some(target_args)) = target_class.static_class_literal(db)? else {
            return None;
        };
        let source_context = source_origin.generic_context(db)?;
        let target_context = target_origin.generic_context(db)?;
        if source_context.len(db) == 0
            || source_context.len(db) != target_context.len(db)
            || source_args.types(db) != target_args.types(db)
            || source_args.materialization_kind(db).is_some()
            || target_args.materialization_kind(db).is_some()
            || !unconstrained_parameters(db, self.env, source_context)
            || !unconstrained_parameters(db, self.env, target_context)
            || source_args
                .types(db)
                .iter()
                .any(|argument| !argument.is_fully_static(db, self.env))
            || !source_class.has_uniform_protocol_members(db, self.env, target)
        {
            return None;
        }

        // These synthetic variables belong to neither class nor any method's generic context,
        // so callable inference cannot solve them. No nested generalization is admitted while
        // they are in scope, and no successful result containing them escapes this proof.
        let parameters: Vec<_> = (0..source_context.len(db))
            .map(|index| {
                Type::TypeVar(BoundTypeVarInstance::synthetic(
                    db,
                    self.env,
                    Name::new(format!("__structural_schema_{index}")),
                    TypeVarVariance::Invariant,
                ))
            })
            .collect();
        let source = Type::instance(
            db,
            self.env,
            ClassType::Generic(GenericAlias::new(
                db,
                source_origin,
                source_context.specialize(db, parameters.as_slice()),
            )),
        );
        let target = Type::instance(
            db,
            self.env,
            ClassType::Generic(GenericAlias::new(
                db,
                target_origin,
                target_context.specialize(db, parameters.as_slice()),
            )),
        );
        let protocol = target.as_protocol_instance(db)?;
        let mut checker = self
            .with_inferable_typevars(TypeVarSet::None)
            .with_operands(ObservedTypePair::new(
                ObservedType::root(source),
                ObservedType::root(target),
            ));
        checker.typevar_evaluation = TypeVarEvaluation::Eager;
        session
            .parametric_schemas
            .borrow_mut()
            .push(ParametricSchema {
                source: source_origin,
                target: target_origin,
                arity: parameters.len(),
                relation: checker.relation,
                evaluation: checker.typevar_evaluation,
                provenance: checker.provenance,
                expensive: checker.perform_expensive_checks,
            });
        // Opening member requirements is the structural guard. All of them are conjuncts:
        // a recursive return cannot hide an incompatible payload or parameter elsewhere.
        let result = protocol
            .interface(db)
            .members(db)
            .when_all(db, self.constraints, |member| {
                checker.type_satisfies_protocol_member(db, source, &member)
            });
        session.parametric_schemas.borrow_mut().pop();
        (result.is_complete() && result.is_always_satisfied(db, self.env, TypeVarSet::None))
            .then(|| self.always())
    }
}
