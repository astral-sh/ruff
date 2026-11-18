use std::borrow::Cow;

use itertools::Itertools;

use super::cyclic::PairVisitor;
use super::tuple::TupleSpec;
use super::visitor::any_over_type_expanding_aliases;
use super::{ClassType, ProgramEnvironment, StaticClassLiteral, Type};
use crate::Db;

impl<'db> Type<'db> {
    /// Retain the inferred outer type of a binding, but use its explicit annotation
    /// to choose between mutually assignable arguments of the same generic type.
    pub(super) fn prefer_declared_type_arguments(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        declared: Self,
    ) -> Self {
        self.merge_binding_arguments(db, env, declared, &PairVisitor::new(None))
            .unwrap_or(self)
    }

    fn merge_binding_arguments(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        declared: Self,
        visitor: &PairVisitor<'db, (), Option<Self>>,
    ) -> Option<Self> {
        if self == declared {
            return None;
        }
        visitor.visit(db, (self, declared), || {
            let inferred = self.resolve_type_alias(db);
            let declared = declared.expand_top_level_aliases(db, env);
            if let Self::Union(union) = inferred {
                let merged = union.map_leave_aliases(db, env, |&arm| {
                    arm.merge_binding_arguments(db, env, declared, visitor)
                        .unwrap_or(arm)
                });
                return (merged != inferred).then_some(merged);
            }

            let (origin, arguments) = binding_arguments(db, env, inferred)?;
            let (declared_origin, declared_arguments) = if let Self::Union(union) = declared {
                // Match by constructor, independently of union ordering. A different union
                // arm can make the assignment valid even if this particular arm does not fit.
                union
                    .elements(db)
                    .iter()
                    .filter_map(|&arm| {
                        let matched = binding_arguments(db, env, arm.resolve_type_alias(db))?;
                        (origin == matched.0
                            && arguments.len() == matched.1.len()
                            && inferred.is_assignable_to(db, env, arm))
                        .then_some(matched)
                    })
                    .exactly_one()
                    .ok()?
            } else {
                binding_arguments(db, env, declared)?
            };
            if origin != declared_origin || arguments.len() != declared_arguments.len() {
                return None;
            }

            let mut merged = Cow::Borrowed(arguments);
            for (index, (&argument, &target)) in
                arguments.iter().zip(declared_arguments).enumerate()
            {
                if let Some(preferred) =
                    argument.preferred_binding_argument(db, env, target, visitor)
                {
                    merged.to_mut()[index] = preferred;
                }
            }
            let Cow::Owned(merged) = merged else {
                return None;
            };
            Some(match origin {
                BindingOrigin::FixedTuple => Type::heterogeneous_tuple(db, env, merged),
                BindingOrigin::Class(class) => Type::instance(
                    db,
                    env,
                    class.apply_specialization(db, |context| context.specialize(db, merged)),
                ),
            })
        })
    }

    fn preferred_binding_argument(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        declared: Self,
        visitor: &PairVisitor<'db, (), Option<Self>>,
    ) -> Option<Self> {
        if self == declared {
            return None;
        }
        // Replacing a type containing an inference-cycle marker with Any would discard
        // the dependency needed to infer it on the next cycle iteration.
        let has_cycle_marker =
            |ty| any_over_type_expanding_aliases(db, env, ty, |ty| ty.is_divergent());
        if declared.is_assignable_to(db, env, self)
            && self.is_assignable_to(db, env, declared)
            && !has_cycle_marker(self)
            && !has_cycle_marker(declared)
        {
            return (!self.is_equivalent_to(db, env, declared)).then_some(declared);
        }
        self.merge_binding_arguments(db, env, declared, visitor)
    }
}

#[derive(Copy, Clone, PartialEq, Eq)]
enum BindingOrigin<'db> {
    FixedTuple,
    Class(StaticClassLiteral<'db>),
}

fn binding_arguments<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    ty: Type<'db>,
) -> Option<(BindingOrigin<'db>, &'db [Type<'db>])> {
    let class = match ty {
        Type::NominalInstance(instance) => {
            if let Some(spec) = instance.own_tuple_spec(db) {
                match spec {
                    Cow::Borrowed(TupleSpec::Fixed(tuple)) => {
                        return Some((BindingOrigin::FixedTuple, tuple.elements_slice()));
                    }
                    // A homogeneous tuple's single argument is its element type.
                    Cow::Borrowed(TupleSpec::Variable(tuple))
                        if tuple.prefix_elements().is_empty()
                            && tuple.suffix_elements().is_empty()
                            && tuple.variable().homogeneous_type().is_some() => {}
                    _ => return None,
                }
            }
            instance.class(db, env)
        }
        Type::ProtocolInstance(instance) => *instance.class_origin(db)?,
        _ => return None,
    };
    let ClassType::Generic(alias) = class else {
        return None;
    };
    let specialization = alias.specialization(db);
    specialization.materialization_kind(db).is_none().then(|| {
        (
            BindingOrigin::Class(alias.origin(db)),
            specialization.types(db),
        )
    })
}
