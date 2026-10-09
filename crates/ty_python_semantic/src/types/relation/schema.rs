//! Guarded proofs for uniformly parameterized structural declarations.
//!
//! A repeated class name does not prove compatibility. A schema checks the member
//! requirements of two type expressions with rigid parameters, then permits a recursive
//! member obligation to instantiate that universally quantified hypothesis.

use ruff_python_ast::name::Name;

use super::{TypeRelation, TypeRelationChecker, TypeVarEvaluation};
use crate::types::constraints::{
    ConstraintProvenance, ConstraintSet, ConstraintSetBuilder, IteratorConstraintsExtension,
};
use crate::types::generics::GenericContext;
use crate::types::projection::{ObservedType, ObservedTypePair};
use crate::types::typevar::{BoundTypeVarInstance, TypeVarSet};
use crate::types::{
    ClassType, GenericAlias, ProtocolInstanceType, StaticClassLiteral, SubclassOfInner,
    SubclassOfType, Type, TypeVarVariance,
};
use crate::{Db, ProgramEnvironment};

/// The runtime view of a class is part of a schema's expression, not just lookup metadata.
/// In particular, a descriptor can expose different members through an instance and its class.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SchemaView {
    Instance,
    Protocol { unfolded: bool },
    ClassObject,
    SubclassOf,
}

impl SchemaView {
    fn class<'db>(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        ty: Type<'db>,
    ) -> Option<ClassType<'db>> {
        match (self, ty) {
            (Self::Instance, Type::NominalInstance(instance)) => Some(instance.class(db, env)),
            (Self::Protocol { unfolded }, Type::ProtocolInstance(protocol))
                if unfolded == protocol.recursive_origin(db).is_some() =>
            {
                protocol.class_origin(db).map(|class| *class)
            }
            (Self::ClassObject, Type::ClassLiteral(class)) => Some(ClassType::NonGeneric(class)),
            (Self::ClassObject, Type::GenericAlias(alias)) => Some(ClassType::Generic(alias)),
            (Self::SubclassOf, Type::SubclassOf(subclass)) => match subclass.subclass_of() {
                SubclassOfInner::Class(class) => Some(class),
                _ => None,
            },
            _ => None,
        }
    }

    fn source(ty: Type<'_>) -> Option<Self> {
        match ty {
            Type::NominalInstance(_) => Some(Self::Instance),
            Type::ClassLiteral(_) | Type::GenericAlias(_) => Some(Self::ClassObject),
            Type::SubclassOf(subclass)
                if matches!(subclass.subclass_of(), SubclassOfInner::Class(_)) =>
            {
                Some(Self::SubclassOf)
            }
            _ => None,
        }
    }

    fn instantiate<'db>(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        class: ClassType<'db>,
    ) -> Option<Type<'db>> {
        Some(match self {
            Self::Instance => Type::instance(db, env, class),
            Self::Protocol { unfolded } => {
                let instance = Type::instance(db, env, class);
                if unfolded {
                    let Type::Recursive(recursive) = instance else {
                        return None;
                    };
                    recursive.unfold(db, env).into_type()
                } else {
                    Type::ProtocolInstance(instance.as_protocol_instance(db)?)
                }
            }
            Self::ClassObject => Type::from(class),
            Self::SubclassOf => SubclassOfType::from(db, env, class),
        })
    }
}

/// A declaration application whose argument positions refer to explicitly quantified binders.
/// Two positions can share a binder; matching must preserve that correlation across both sides.
#[derive(Debug)]
struct SchemaExpression<'db> {
    view: SchemaView,
    declaration: StaticClassLiteral<'db>,
    context: Option<GenericContext<'db>>,
    arguments: Box<[usize]>,
}

impl<'db> SchemaExpression<'db> {
    fn abstract_expression(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        ty: Type<'db>,
        view: SchemaView,
        bindings: &mut SchemaBindings<'db>,
    ) -> Option<Self> {
        let class = view.class(db, env, ty)?;
        let (declaration, specialization) = class.static_class_literal(db)?;
        let context = declaration.generic_context(db);
        if context.is_some_and(|context| !unconstrained_parameters(db, env, context)) {
            return None;
        }
        let arguments = match (context, specialization) {
            (Some(context), Some(specialization))
                if specialization.materialization_kind(db).is_none()
                    && specialization.types(db).len() == context.len(db) =>
            {
                let arguments = specialization.types(db);
                if arguments
                    .iter()
                    .any(|argument| !argument.is_fully_static(db, env))
                {
                    return None;
                }
                bindings.abstract_arguments(arguments)
            }
            (None, None) => Box::default(),
            _ => return None,
        };
        Some(Self {
            view,
            declaration,
            context,
            arguments,
        })
    }

    fn instantiate(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        parameters: &[Type<'db>],
    ) -> Option<ObservedType<'db>> {
        let class = match self.context {
            Some(context) => {
                let arguments = self
                    .arguments
                    .iter()
                    .map(|&index| parameters.get(index).copied())
                    .collect::<Option<Vec<_>>>()?;
                ClassType::Generic(GenericAlias::new(
                    db,
                    self.declaration,
                    context.specialize(db, arguments.as_slice()),
                ))
            }
            None => ClassType::NonGeneric(self.declaration.into()),
        };
        Some(ObservedType::root(self.view.instantiate(db, env, class)?))
    }

    fn match_expression(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        ty: Type<'db>,
        substitution: &mut [Option<Type<'db>>],
    ) -> bool {
        let Some(class) = self.view.class(db, env, ty) else {
            return false;
        };
        let Some((declaration, specialization)) = class.static_class_literal(db) else {
            return false;
        };
        if declaration != self.declaration {
            return false;
        }
        match (self.context, specialization) {
            (None, None) => self.arguments.is_empty(),
            (Some(_), Some(specialization))
                if specialization.materialization_kind(db).is_none() =>
            {
                match_arguments(&self.arguments, specialization.types(db), substitution)
            }
            _ => false,
        }
    }
}

/// Initial argument equality selects a stronger universally quantified hypothesis. It does
/// not establish an expression origin or prove the relation: the rigid proof below must do that.
#[derive(Default)]
struct SchemaBindings<'db> {
    initial_arguments: Vec<Type<'db>>,
}

impl<'db> SchemaBindings<'db> {
    fn abstract_arguments(&mut self, arguments: &[Type<'db>]) -> Box<[usize]> {
        arguments
            .iter()
            .map(|argument| {
                if let Some(index) = self
                    .initial_arguments
                    .iter()
                    .position(|previous| previous == argument)
                {
                    index
                } else {
                    let index = self.initial_arguments.len();
                    self.initial_arguments.push(*argument);
                    index
                }
            })
            .collect()
    }
}

fn match_arguments<'db>(
    binders: &[usize],
    arguments: &[Type<'db>],
    substitution: &mut [Option<Type<'db>>],
) -> bool {
    binders.len() == arguments.len()
        && binders.iter().zip(arguments).all(|(&binder, &argument)| {
            let Some(bound) = substitution.get_mut(binder) else {
                return false;
            };
            match bound {
                Some(previous) => *previous == argument,
                None => {
                    *bound = Some(argument);
                    true
                }
            }
        })
}

/// A universally quantified relation whose finite member requirements are being checked.
#[derive(Debug)]
pub(super) struct ParametricSchema<'db> {
    source: SchemaExpression<'db>,
    target: SchemaExpression<'db>,
    parameters: Box<[Type<'db>]>,
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
        source: Type<'db>,
        target: Type<'db>,
    ) -> bool {
        if checker.relation != self.relation
            || checker.typevar_evaluation != self.evaluation
            || checker.provenance != self.provenance
            || checker.perform_expensive_checks != self.expensive
            || checker.constraints.relation_session().is_negative()
        {
            return false;
        }
        // Schema binders are slots, while the proof-local type variables are rigid values.
        // Substituting a slot with list[rigid] is legal and does not infer or solve that rigid.
        let mut substitution = vec![None; self.parameters.len()];
        self.source
            .match_expression(db, checker.env, source, &mut substitution)
            && self
                .target
                .match_expression(db, checker.env, target, &mut substitution)
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
        let source_view = SchemaView::source(source)?;
        let source_class = source_view.class(db, self.env, source)?;
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
                .any(|schema| schema.matches(db, self, source, Type::ProtocolInstance(target)))
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
        let mut bindings = SchemaBindings::default();
        let source_expression = SchemaExpression::abstract_expression(
            db,
            self.env,
            source,
            source_view,
            &mut bindings,
        )?;
        let target_expression = SchemaExpression::abstract_expression(
            db,
            self.env,
            Type::ProtocolInstance(target),
            SchemaView::Protocol {
                unfolded: target.recursive_origin(db).is_some(),
            },
            &mut bindings,
        )?;
        let uniform = source_class.has_uniform_protocol_members(db, self.env, source, target);
        if bindings.initial_arguments.is_empty() || !uniform {
            return None;
        }
        // Rebuilding with the original arguments must recover the full original types.
        // A class and its generic arguments alone can omit stored structure, such as the
        // element positions in a tuple. Generalizing that weaker expression would discard
        // constraints the concrete proof needs to infer.
        if source_expression
            .instantiate(db, self.env, &bindings.initial_arguments)?
            .ty
            != source
            || target_expression
                .instantiate(db, self.env, &bindings.initial_arguments)?
                .ty
                != Type::ProtocolInstance(target)
        {
            return None;
        }

        // These synthetic variables belong to neither class nor any method's generic context,
        // so callable inference cannot solve them. No successful result containing them escapes.
        let parameters: Box<[_]> = (0..bindings.initial_arguments.len())
            .map(|index| {
                Type::TypeVar(BoundTypeVarInstance::synthetic(
                    db,
                    self.env,
                    Name::new(format!("__structural_schema_{index}")),
                    TypeVarVariance::Invariant,
                ))
            })
            .collect();
        let operands = ObservedTypePair::new(
            source_expression.instantiate(db, self.env, &parameters)?,
            target_expression.instantiate(db, self.env, &parameters)?,
        );
        let source = operands.source.ty;
        let protocol = operands.target.ty.as_protocol_instance(db)?;
        // This is a closed, universally quantified lemma. Its rigid parameters cannot
        // use the concrete caller's assumptions or report diagnostics about those parameters.
        // Failure to prove the lemma must not mark the caller incomplete: it can still check
        // the concrete member types. All operations within the lemma share its own session.
        let constraints = ConstraintSetBuilder::new();
        let session = constraints.relation_session();
        let mut checker = TypeRelationChecker::new(
            self.env,
            self.relation,
            &constraints,
            TypeVarSet::None,
            self.materialization_visitor,
            operands,
        );
        checker.provenance = self.provenance;
        checker.perform_expensive_checks = self.perform_expensive_checks;
        session
            .parametric_schemas
            .borrow_mut()
            .push(ParametricSchema {
                source: source_expression,
                target: target_expression,
                parameters,
                relation: checker.relation,
                evaluation: checker.typevar_evaluation,
                provenance: checker.provenance,
                expensive: checker.perform_expensive_checks,
            });
        // Opening member requirements is the structural guard. Lookup selection has already
        // been certified independently of this hypothesis; every member remains a conjunct.
        let result = protocol
            .interface(db)
            .members(db)
            .when_all(db, &constraints, |member| {
                checker.type_satisfies_protocol_member(db, source, &member)
            });
        (result.is_complete() && result.is_always_satisfied(db, self.env, TypeVarSet::None))
            .then(|| self.always())
    }
}

#[cfg(test)]
mod tests {
    use super::{SchemaBindings, match_arguments};
    use crate::db::tests::setup_db;
    use crate::types::{KnownClass, Type};

    #[test]
    fn schema_instantiation_preserves_argument_correlations() {
        let db = setup_db();
        let env = db.program_environment();
        let int = KnownClass::Int.to_instance(&db, &env);
        let str = KnownClass::Str.to_instance(&db, &env);
        let mut bindings = SchemaBindings::default();
        let source = bindings.abstract_arguments(&[int, str, int]);
        let target = bindings.abstract_arguments(&[str, int]);
        let mut substitution = vec![None; bindings.initial_arguments.len()];
        assert!(match_arguments(
            &source,
            &[str, int, str],
            &mut substitution
        ));
        assert!(match_arguments(&target, &[int, str], &mut substitution));
        assert!(!match_arguments(
            &target,
            &[int, Type::object()],
            &mut substitution
        ));
    }

    #[test]
    fn nongeneric_target_does_not_bind_source_parameters() {
        let db = setup_db();
        let env = db.program_environment();
        let int = KnownClass::Int.to_instance(&db, &env);
        let mut bindings = SchemaBindings::default();
        let source = bindings.abstract_arguments(&[int]);
        let target = bindings.abstract_arguments(&[]);
        let mut substitution = vec![None; bindings.initial_arguments.len()];
        assert!(match_arguments(
            &source,
            &[Type::object()],
            &mut substitution
        ));
        assert!(match_arguments(&target, &[], &mut substitution));
        assert_eq!(substitution, [Some(Type::object())]);
    }
}
