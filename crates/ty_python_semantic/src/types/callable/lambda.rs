//! Transform lambda signatures without making their inferred return types part of their identity.

use std::cell::RefCell;

use super::{LambdaSignature, infer_lambda_signature};
use crate::types::generics::{ApplySpecialization, GenericContext, Specialization};
use crate::types::visitor::any_over_type_including_alias_arguments;
use crate::types::{
    ApplyTypeMappingVisitor, BindingContext, MaterializationKind, PromotionKind, PromotionMode,
    SelfBinding, Type, TypeContext, TypeMapping,
};
use crate::{Db, FxOrderSet, ProgramEnvironment};
use ty_python_core::semantic_index;

/// The source and context of a deferred lambda transformation. The inferred result is
/// deliberately absent: recursive returns refer to another application of this same mapping.
#[derive(Clone, Debug, PartialEq, Eq, Hash, get_size2::GetSize, salsa::SalsaValue)]
pub struct LambdaSignatureMapping<'db> {
    source: LambdaSignature<'db>,
    mapping: LambdaMapping<'db>,
    context: Option<Type<'db>>,
    materialize_typevar_bounds_and_defaults: bool,
}

impl<'db> LambdaSignatureMapping<'db> {
    /// Apply the saved mapping in the context in which it was requested.
    pub(super) fn return_type(&self, db: &'db dyn Db) -> Type<'db> {
        let env = ProgramEnvironment::from_scope(self.source.scope(db));
        let visitor = ApplyTypeMappingVisitor {
            materialize_typevar_bounds_and_defaults: self.materialize_typevar_bounds_and_defaults,
            ..ApplyTypeMappingVisitor::new(&env)
        };
        infer_lambda_signature(db, self.source)
            .overload_return_type_or_unknown(db, &env)
            .apply_type_mapping_impl(
                db,
                &self.mapping.as_type_mapping(),
                TypeContext::new(self.context),
                &visitor,
            )
    }
}

/// An owned semantic mapping that can be applied when a lambda's return type is requested.
/// Structural substitutions never enter an inferred body and are handled separately.
#[derive(Clone, Debug, PartialEq, Eq, Hash, get_size2::GetSize, salsa::SalsaValue)]
pub(super) enum LambdaMapping<'db> {
    Specialize {
        specialization: Specialization<'db>,
        specialize_self_domain: bool,
        materialization: Option<MaterializationKind>,
    },
    Promote(PromotionMode, PromotionKind),
    BindLegacyTypevars(BindingContext<'db>),
    FreshenBoundTypeVars(GenericContext<'db>, u32),
    BindSelf(SelfBinding<'db>),
    ReplaceSelf(Type<'db>),
    Materialize(MaterializationKind),
    ReplaceParameterDefaults,
    EagerExpansion,
}

impl<'db> LambdaMapping<'db> {
    /// Own the mapping's inputs, retaining only substitutions available to this lambda.
    pub(super) fn from_type_mapping(
        db: &'db dyn Db,
        lambda: LambdaSignature<'db>,
        mapping: &TypeMapping<'_, 'db>,
    ) -> Option<Self> {
        Some(match mapping {
            TypeMapping::ApplySpecialization(specialization)
            | TypeMapping::ApplySpecializationWithMaterialization { specialization, .. } => {
                if specialization.preserves_lazy_signatures() {
                    return None;
                }
                Self::Specialize {
                    specialization: {
                        let (variables, types): (Vec<_>, Vec<_>) = lambda
                            .captured_typevars(db)
                            .variables(db)
                            .filter_map(|variable| {
                                specialization.get(db, variable).map(|ty| (variable, ty))
                            })
                            .unzip();
                        // An absent substitution can still specialize a Self bound. Adding
                        // an identity substitution would suppress that existing behavior.
                        let context = GenericContext::from_typevar_instances(
                            db,
                            &ProgramEnvironment::from_scope(lambda.scope(db)),
                            variables,
                        );
                        Specialization::new(db, context, types.into_boxed_slice(), None, None)
                    },
                    specialize_self_domain: specialization.specialize_self_domain(),
                    materialization: match mapping {
                        TypeMapping::ApplySpecializationWithMaterialization {
                            materialization_kind,
                            ..
                        } => Some(*materialization_kind),
                        _ => None,
                    },
                }
            }
            TypeMapping::Promote(mode, kind) => Self::Promote(*mode, *kind),
            TypeMapping::BindLegacyTypevars(context) => Self::BindLegacyTypevars(*context),
            TypeMapping::FreshenBoundTypeVars {
                generic_context,
                delta,
            } => Self::FreshenBoundTypeVars(*generic_context, *delta),
            TypeMapping::BindSelf(binding) => Self::BindSelf(binding.clone()),
            TypeMapping::ReplaceSelf { new_upper_bound } => Self::ReplaceSelf(*new_upper_bound),
            TypeMapping::Materialize(kind) => Self::Materialize(*kind),
            TypeMapping::ReplaceParameterDefaults => Self::ReplaceParameterDefaults,
            TypeMapping::EagerExpansion => Self::EagerExpansion,
            TypeMapping::ApplyRecursiveSubstitution(_) | TypeMapping::RescopeReturnCallables(_) => {
                return None;
            }
        })
    }

    fn as_type_mapping(&self) -> TypeMapping<'_, 'db> {
        match self {
            Self::Specialize {
                specialization,
                specialize_self_domain,
                materialization,
            } => {
                let specialization = ApplySpecialization::Specialization {
                    specialization: *specialization,
                    specialize_self_domain: *specialize_self_domain,
                };
                match materialization {
                    Some(kind) => TypeMapping::ApplySpecializationWithMaterialization {
                        specialization,
                        materialization_kind: *kind,
                    },
                    None => TypeMapping::ApplySpecialization(specialization),
                }
            }
            Self::Promote(mode, kind) => TypeMapping::Promote(*mode, *kind),
            Self::BindLegacyTypevars(context) => TypeMapping::BindLegacyTypevars(*context),
            Self::FreshenBoundTypeVars(generic_context, delta) => {
                TypeMapping::FreshenBoundTypeVars {
                    generic_context: *generic_context,
                    delta: *delta,
                }
            }
            Self::BindSelf(binding) => TypeMapping::BindSelf(binding.clone()),
            Self::ReplaceSelf(new_upper_bound) => TypeMapping::ReplaceSelf {
                new_upper_bound: *new_upper_bound,
            },
            Self::Materialize(kind) => TypeMapping::Materialize(*kind),
            Self::ReplaceParameterDefaults => TypeMapping::ReplaceParameterDefaults,
            Self::EagerExpansion => TypeMapping::EagerExpansion,
        }
    }
}

#[salsa::tracked]
impl<'db> LambdaSignature<'db> {
    /// Variables in the lambda's lexical and contextual inputs after transformation.
    /// Inferring the body here would re-enter the transformation of its recursive references.
    #[salsa::tracked(
        returns(copy),
        cycle_initial=|db, _, lambda: LambdaSignature<'db>| GenericContext::from_typevar_instances(
            db, &ProgramEnvironment::from_scope(lambda.scope(db)), []
        ),
        heap_size=ruff_memory_usage::heap_size,
    )]
    fn captured_typevars(self, db: &'db dyn Db) -> GenericContext<'db> {
        let scope = self.scope(db);
        let env = ProgramEnvironment::from_scope(scope);
        let index = semantic_index(db, scope.program_file(db));
        let mut source = self;
        let mut mappings = Vec::new();
        while let Some(mapping) = source.mapping(db) {
            mappings.push(mapping);
            source = mapping.source;
        }
        let variables = RefCell::new(
            index
                .ancestor_scopes(scope.file_scope_id(db))
                .filter_map(|(_, scope)| GenericContext::lexical_of_node(db, scope.node(), index))
                .flat_map(|context| context.variables(db))
                .collect::<FxOrderSet<_>>(),
        );
        let collect = |ty| {
            any_over_type_including_alias_arguments(db, &env, ty, |ty| {
                if let Type::TypeVar(variable) = ty {
                    variables.borrow_mut().insert(variable);
                }
                false
            });
        };
        for parameter in source.parameters(db) {
            collect(parameter.annotated_type());
        }
        if let Some(annotation) = source.return_annotation(db) {
            collect(annotation);
        }
        for mapping in mappings.into_iter().rev() {
            let current = variables.take();
            let visitor = ApplyTypeMappingVisitor {
                materialize_typevar_bounds_and_defaults: mapping
                    .materialize_typevar_bounds_and_defaults,
                ..ApplyTypeMappingVisitor::new(&env)
            };
            let context = TypeContext::new(mapping.context);
            let mapping = mapping.mapping.as_type_mapping();
            for variable in current {
                // Captured variables can occur in either variance position in the body.
                for mapping in [&mapping, &mapping.flip()] {
                    collect(
                        Type::TypeVar(variable)
                            .apply_type_mapping_impl(db, mapping, context, &visitor),
                    );
                }
            }
        }
        GenericContext::from_typevar_instances(db, &env, variables.into_inner())
    }

    /// Rewrite stored inputs without evaluating the lambda's body. Recursive binding
    /// and unfolding must also rewrite captured arguments in deferred specializations.
    pub(super) fn map_inputs(
        self,
        db: &'db dyn Db,
        mapping: &TypeMapping<'_, 'db>,
        tcx: TypeContext<'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> Self {
        let map = |ty: Type<'db>| ty.apply_type_mapping_impl(db, mapping, tcx, visitor);
        let deferred = self.mapping(db).as_ref().map(|deferred| {
            let deferred_mapping = match &deferred.mapping {
                LambdaMapping::Specialize {
                    specialization,
                    specialize_self_domain,
                    materialization,
                } => LambdaMapping::Specialize {
                    specialization: specialization.apply_type_mapping_impl(
                        db,
                        mapping,
                        &[],
                        visitor,
                    ),
                    specialize_self_domain: *specialize_self_domain,
                    materialization: *materialization,
                },
                LambdaMapping::BindSelf(binding) => LambdaMapping::BindSelf(SelfBinding {
                    ty: map(binding.ty),
                    ..binding.clone()
                }),
                LambdaMapping::ReplaceSelf(ty) => LambdaMapping::ReplaceSelf(map(*ty)),
                other => other.clone(),
            };
            LambdaSignatureMapping {
                source: deferred.source.map_inputs(db, mapping, tcx, visitor),
                mapping: deferred_mapping,
                context: deferred.context.map(map),
                materialize_typevar_bounds_and_defaults: deferred
                    .materialize_typevar_bounds_and_defaults,
            }
        });
        Self::new(
            db,
            self.parameters(db)
                .apply_type_mapping_impl(db, mapping, tcx, visitor),
            self.scope(db),
            self.body(db),
            self.return_annotation(db).map(map),
            deferred,
        )
    }

    /// Compose transformations on the source rather than traversing the inferred return graph.
    pub(super) fn apply_mapping(
        self,
        db: &'db dyn Db,
        mut mapping: LambdaMapping<'db>,
        tcx: TypeContext<'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> Self {
        // Promotions commute with each other and with materialization. Materializing an
        // already materialized type has no effect. These operations must remain idempotent
        // when a recursive member lookup alternates between them.
        if matches!(
            mapping,
            LambdaMapping::Promote(..) | LambdaMapping::Materialize(_)
        ) {
            let mut source = self;
            while let Some(previous) = source.mapping(db)
                && matches!(
                    previous.mapping,
                    LambdaMapping::Promote(..) | LambdaMapping::Materialize(_)
                )
                && previous.context == tcx.annotation
                && previous.materialize_typevar_bounds_and_defaults
                    == visitor.materialize_typevar_bounds_and_defaults
            {
                if previous.mapping == mapping
                    || matches!(
                        (&previous.mapping, &mapping),
                        (LambdaMapping::Materialize(_), LambdaMapping::Materialize(_))
                    )
                {
                    return self;
                }
                source = previous.source;
            }
        }
        if let LambdaMapping::Specialize { specialization, .. } = &mapping
            && specialization
                .generic_context(db)
                .variables(db)
                .zip(specialization.types(db))
                .all(|(variable, ty)| Type::TypeVar(variable) == *ty)
        {
            return self;
        }

        if let Some(previous) = self.mapping(db)
            && previous.mapping == mapping
            && previous.context == tcx.annotation
            && previous.materialize_typevar_bounds_and_defaults
                == visitor.materialize_typevar_bounds_and_defaults
            && matches!(
                mapping,
                LambdaMapping::BindSelf(_)
                    | LambdaMapping::BindLegacyTypevars(_)
                    | LambdaMapping::ReplaceSelf(_)
                    | LambdaMapping::ReplaceParameterDefaults
                    | LambdaMapping::EagerExpansion
            )
        {
            return self;
        }

        let parameters = self.parameters(db).apply_type_mapping_impl(
            db,
            &mapping.as_type_mapping(),
            tcx,
            visitor,
        );
        let mut source = self;
        if let Some(previous_mapping) = self.mapping(db)
            && previous_mapping.context == tcx.annotation
            && previous_mapping.materialize_typevar_bounds_and_defaults
                == visitor.materialize_typevar_bounds_and_defaults
            && let LambdaMapping::Specialize {
                specialization: previous,
                specialize_self_domain: previous_self_domain,
                materialization: None,
            } = &previous_mapping.mapping
            && let LambdaMapping::Specialize {
                specialization,
                specialize_self_domain,
                materialization: None,
            } = &mut mapping
            && specialize_self_domain == previous_self_domain
        {
            let mapping = TypeMapping::ApplySpecialization(ApplySpecialization::Specialization {
                specialization: *specialization,
                specialize_self_domain: *specialize_self_domain,
            });
            let (variables, types): (Vec<_>, Vec<_>) = previous_mapping
                .source
                .captured_typevars(db)
                .variables(db)
                .filter_map(|variable| {
                    previous
                        .get(db, variable)
                        .map(|ty| ty.apply_type_mapping_impl(db, &mapping, tcx, visitor))
                        .or_else(|| specialization.get(db, variable))
                        .map(|ty| (variable, ty))
                })
                .unzip();
            let context = GenericContext::from_typevar_instances(db, visitor.env, variables);
            *specialization =
                Specialization::new(db, context, types.into_boxed_slice(), None, None);
            source = previous_mapping.source;
        }

        Self::new(
            db,
            parameters,
            source.scope(db),
            source.body(db),
            source.return_annotation(db),
            Some(LambdaSignatureMapping {
                source,
                mapping,
                context: tcx.annotation,
                materialize_typevar_bounds_and_defaults: visitor
                    .materialize_typevar_bounds_and_defaults,
            }),
        )
    }
}
