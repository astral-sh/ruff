//! Deferred mappings on aliases. Each step acts on a closed, specialized unfolding;
//! recursive references retain the step without rebuilding the recursive constructor.
//!
//! Mapping first checks whether reachable alias bodies change in an independent traversal.
//! Unchanged aliases keep their identity. Each retained step has a separate
//! transformation cache, while nested materialization comparisons share their recursion guard.

use std::cell::{Cell, RefCell};

use rustc_hash::{FxHashMap, FxHashSet};

use super::cyclic::TypeIdentity;
use super::generics::{ApplySpecialization, Specialization};
use super::{
    ApplyTypeMappingVisitor, BindingContext, BoundTypeVarIdentity, BoundTypeVarInstance,
    GenericContext, MaterializationKind, PromotionKind, PromotionMode, SelfBinding, Type,
    TypeContext, TypeMapping,
};
use crate::{Db, FxIndexMap, FxIndexSet};

/// Checks alias bodies and the arguments reaching their exposed parameters without rebuilding
/// recursive types. Growing aliases share a formal body; parameter uses propagate to arguments
/// until no new mapping state is discovered.
#[derive(Default)]
pub(super) struct MappingProbe<'db> {
    seen: RefCell<FxHashSet<(Type<'db>, MappingStep<'db>)>>,
    constructors: RefCell<Vec<ProbeConstructor<'db>>>,
    parameters: RefCell<
        FxHashMap<BoundTypeVarIdentity<'db>, (BoundTypeVarInstance<'db>, ProbeParameter<'db>)>,
    >,
    calls: RefCell<Vec<(usize, Specialization<'db>)>>,
    uses: RefCell<FxIndexSet<(ProbeParameter<'db>, MappingStep<'db>)>>,
    changed: Cell<bool>,
}

struct ProbeConstructor<'db> {
    source: Type<'db>,
    body_source: Type<'db>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
struct ProbeParameter<'db> {
    constructor: usize,
    variable: BoundTypeVarInstance<'db>,
    mappings: Option<DeferredTypeMapping<'db>>,
}

/// Deferred steps compose on opaque parameters; the requested step inspects their uses.
#[derive(Clone, Copy)]
pub(super) enum MappingProbeContext<'a, 'db> {
    Inspect(&'a MappingProbe<'db>),
    Compose(&'a MappingProbe<'db>),
}

impl<'db> MappingProbeContext<'_, 'db> {
    fn composing(self) -> Self {
        match self {
            Self::Inspect(probe) | Self::Compose(probe) => Self::Compose(probe),
        }
    }

    pub(super) fn map_type(
        self,
        db: &'db dyn Db,
        ty: Type<'db>,
        mapping: &TypeMapping<'_, 'db>,
        context: TypeContext<'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> Option<Type<'db>> {
        if !matches!(
            ty,
            Type::TypeVar(_) | Type::Recursive(_) | Type::TypeAlias(_)
        ) {
            return None;
        }
        let operation = MappingOperation::from_mapping(mapping)?;
        let probe = match self {
            Self::Inspect(probe) | Self::Compose(probe) => probe,
        };
        if let Type::TypeVar(variable) = ty {
            let reference = probe
                .parameters
                .borrow()
                .get(&variable.identity(db))
                .map(|(_, parameter)| *parameter);
            if let Some(mut reference) = reference {
                return Some(match self {
                    Self::Inspect(_) => {
                        probe.uses.borrow_mut().insert((
                            reference,
                            MappingStep {
                                operation,
                                context,
                                materialize_bounds: visitor.materialize_typevar_bounds_and_defaults,
                            },
                        ));
                        ty
                    }
                    Self::Compose(_) => {
                        reference.mappings = DeferredTypeMapping::append(
                            db,
                            reference.mappings,
                            mapping,
                            context,
                            visitor,
                        );
                        Type::TypeVar(probe.parameter(db, reference))
                    }
                });
            }
            return None;
        }
        match self {
            Self::Inspect(_) => probe.inspect_alias(
                db,
                ty,
                &MappingStep {
                    operation,
                    context,
                    materialize_bounds: visitor.materialize_typevar_bounds_and_defaults,
                },
                visitor,
            ),
            Self::Compose(_) => None,
        }
    }
}

impl<'db> MappingProbe<'db> {
    pub(super) fn changes(
        db: &'db dyn Db,
        ty: Type<'db>,
        mapping: &TypeMapping<'_, 'db>,
        context: TypeContext<'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> bool {
        // During replay, formal parameters stand for arguments that have not been inspected
        // yet. Preserve the step so its effect is evaluated when those arguments are exposed.
        if matches!(visitor.mapping_probe, Some(MappingProbeContext::Compose(_))) {
            return true;
        }
        let probe = Self::default();
        let mut visitor = visitor.for_new_materialization_root();
        visitor.mapping_probe = Some(MappingProbeContext::Inspect(&probe));
        ty.apply_type_mapping_impl(db, mapping, context, &visitor);
        let mut checked = FxHashSet::default();
        while !probe.changed.get() {
            let before = checked.len();
            let calls = probe.calls.borrow().clone();
            let uses = probe.uses.borrow().clone();
            for (constructor, arguments) in calls {
                for (parameter, step) in &uses {
                    if parameter.constructor != constructor
                        || !checked.insert((arguments, *parameter, step.clone()))
                    {
                        continue;
                    }
                    let Some(argument) = arguments.get(db, parameter.variable) else {
                        continue;
                    };
                    let argument = parameter
                        .mappings
                        .map_or(argument, |mappings| mappings.apply(db, argument, &visitor));
                    let mut use_visitor = visitor.for_new_materialization_root();
                    use_visitor.materialize_typevar_bounds_and_defaults = step.materialize_bounds;
                    step.operation.with_mapping(&mut |mapping| {
                        if argument.apply_type_mapping_impl(
                            db,
                            &mapping,
                            step.context,
                            &use_visitor,
                        ) != argument
                        {
                            probe.changed.set(true);
                        }
                    });
                }
            }
            if before == checked.len() {
                break;
            }
        }
        probe.changed.get()
    }

    fn parameter(
        &self,
        db: &'db dyn Db,
        parameter: ProbeParameter<'db>,
    ) -> BoundTypeVarInstance<'db> {
        let mut parameters = self.parameters.borrow_mut();
        if let Some((variable, _)) = parameters
            .values()
            .find(|(_, existing)| *existing == parameter)
        {
            return *variable;
        }
        // These parameters only occur in probe bodies. The suffix cannot appear in a Python
        // identifier, and distinct mapping states must remain distinct during simplification.
        let variable = parameter
            .variable
            .with_name_suffix(db, &format!("$mapping{}", parameters.len()));
        parameters.insert(variable.identity(db), (variable, parameter));
        variable
    }

    fn inspect_alias(
        &self,
        db: &'db dyn Db,
        ty: Type<'db>,
        step: &MappingStep<'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> Option<Type<'db>> {
        let (source, arguments) = match ty.to_type_identity(db) {
            TypeIdentity::GrowingRecursive(_) => {
                let Type::Recursive(alias) = ty else {
                    return None;
                };
                let arguments = alias.arguments(db);
                (
                    Type::Recursive(alias.with_arguments(
                        db,
                        arguments.map(|arguments| {
                            arguments.generic_context(db).identity_specialization(db)
                        }),
                    )),
                    arguments,
                )
            }
            TypeIdentity::GrowingTypeAlias(_) => {
                let Type::TypeAlias(alias) = ty else {
                    return None;
                };
                let arguments = alias.specialization(db).or_else(|| {
                    alias
                        .generic_context(db)
                        .map(|parameters| parameters.default_specialization(db, None))
                });
                (
                    Type::TypeAlias(alias.apply_specialization(db, |parameters| {
                        parameters.identity_specialization(db)
                    })),
                    arguments,
                )
            }
            _ => (ty, None),
        };
        let source = if let Some(arguments) = arguments {
            let existing = self
                .constructors
                .borrow()
                .iter()
                .position(|node| node.source == source);
            let index = existing.unwrap_or_else(|| {
                let index = self.constructors.borrow().len();
                let parameters = arguments.generic_context(db);
                let variables = parameters
                    .variables(db)
                    .map(|variable| {
                        Type::TypeVar(self.parameter(
                            db,
                            ProbeParameter {
                                constructor: index,
                                variable,
                                mappings: None,
                            },
                        ))
                    })
                    .collect::<Vec<_>>();
                let formal = parameters.specialize(db, variables);
                let body_source = match source {
                    Type::Recursive(alias) => {
                        Type::Recursive(alias.with_arguments(db, Some(formal)))
                    }
                    Type::TypeAlias(alias) => {
                        Type::TypeAlias(alias.apply_specialization(db, |_| formal))
                    }
                    _ => source,
                };
                self.constructors.borrow_mut().push(ProbeConstructor {
                    source,
                    body_source,
                });
                index
            });
            let mut calls = self.calls.borrow_mut();
            if !calls.contains(&(index, arguments)) {
                calls.push((index, arguments));
            }
            self.constructors.borrow()[index].body_source
        } else {
            source
        };
        if !self.seen.borrow_mut().insert((source, step.clone())) {
            return Some(ty);
        }
        let body = match source {
            Type::Recursive(recursive) => recursive
                .unfold_with_mapping_visitor(db, visitor)
                .into_type(),
            Type::TypeAlias(alias) => alias.value_type_with_mapping_visitor(db, visitor),
            _ => return None,
        };
        // An enclosing callable may be identical to this body. Inspect each alias with its
        // own structural cache; only the alias/parameter worklist is shared across bodies.
        let mut visitor = visitor.for_new_materialization_root();
        visitor.mapping_probe = Some(MappingProbeContext::Inspect(self));
        step.operation.with_mapping(&mut |mapping| {
            if body.apply_type_mapping_impl(db, &mapping, step.context, &visitor) != body {
                self.changed.set(true);
            }
        });
        Some(ty)
    }
}

/// An ordered sequence of mappings applied after the alias's own specialization.
#[salsa::interned(debug, heap_size=ruff_memory_usage::heap_size)]
pub struct DeferredTypeMapping<'db> {
    #[returns(copy)]
    previous: Option<DeferredTypeMapping<'db>>,
    #[returns(ref)]
    step: MappingStep<'db>,
}

/// A retained operation and the inference context under which it was requested.
#[derive(Clone, Debug, PartialEq, Eq, Hash, get_size2::GetSize, salsa::SalsaValue)]
pub struct MappingStep<'db> {
    operation: MappingOperation<'db>,
    context: TypeContext<'db>,
    materialize_bounds: bool,
}

impl get_size2::GetSize for DeferredTypeMapping<'_> {}

impl<'db> DeferredTypeMapping<'db> {
    pub(super) fn operation(self, db: &'db dyn Db) -> &'db MappingOperation<'db> {
        &self.step(db).operation
    }

    pub(super) fn preceding(self, db: &'db dyn Db) -> Option<Self> {
        self.previous(db)
    }
    pub(super) fn append(
        db: &'db dyn Db,
        previous: Option<Self>,
        mapping: &TypeMapping<'_, 'db>,
        context: TypeContext<'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> Option<Self> {
        let operation = MappingOperation::from_mapping(mapping)?;
        let context = if context.annotation.is_none() {
            TypeContext::default()
        } else {
            context
        };
        if let Some(previous) = previous
            && operation.is_idempotent()
            && previous.step(db).operation == operation
            && previous.step(db).context == context
            && previous.step(db).materialize_bounds
                == visitor.materialize_typevar_bounds_and_defaults
        {
            return Some(previous);
        }
        Some(Self::new(
            db,
            previous,
            MappingStep {
                operation,
                context,
                materialize_bounds: visitor.materialize_typevar_bounds_and_defaults,
            },
        ))
    }

    pub(super) fn materialization_kind(self, db: &'db dyn Db) -> Option<MaterializationKind> {
        match &self.step(db).operation {
            MappingOperation::Materialize(kind) => Some(*kind),
            _ => None,
        }
    }

    pub(super) fn without_materialization(self, db: &'db dyn Db) -> Option<Self> {
        if self.materialization_kind(db).is_some() {
            self.previous(db)
        } else {
            Some(self)
        }
    }

    pub(super) fn apply(
        self,
        db: &'db dyn Db,
        mut ty: Type<'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> Type<'db> {
        if let Some(previous) = self.previous(db) {
            ty = previous.apply(db, ty, visitor);
        }
        // Each step has its own mapping identity, but materialization comparisons share a guard.
        let mut visitor = visitor.for_new_materialization_root();
        visitor.mapping_probe = visitor.mapping_probe.map(MappingProbeContext::composing);
        visitor.materialize_typevar_bounds_and_defaults = self.step(db).materialize_bounds;
        self.step(db).operation.with_mapping(&mut |mapping| {
            ty.apply_type_mapping_impl(db, &mapping, self.step(db).context, &visitor)
        })
    }
}

/// Owns the data borrowed by a mapping, so an alias can retain it after inference returns.
#[derive(Clone, Debug, PartialEq, Eq, Hash, get_size2::GetSize, salsa::SalsaValue)]
pub(super) enum MappingOperation<'db> {
    Specialize(OwnedSpecialization<'db>, Option<MaterializationKind>),
    Promote(PromotionMode, PromotionKind),
    BindLegacyTypevars(BindingContext<'db>),
    FreshenBoundTypeVars(GenericContext<'db>, u32),
    BindSelf(SelfBinding<'db>),
    ReplaceSelf(Type<'db>),
    Materialize(MaterializationKind),
    ReplaceParameterDefaults,
    RescopeReturnCallables(Box<[(BoundTypeVarInstance<'db>, BoundTypeVarInstance<'db>)]>),
}

impl<'db> MappingOperation<'db> {
    fn from_mapping(mapping: &TypeMapping<'_, 'db>) -> Option<Self> {
        Some(match mapping {
            TypeMapping::ApplySpecialization(specialization) => {
                Self::Specialize(OwnedSpecialization::new(*specialization), None)
            }
            TypeMapping::ApplySpecializationWithMaterialization {
                specialization,
                materialization_kind,
            } => Self::Specialize(
                OwnedSpecialization::new(*specialization),
                Some(*materialization_kind),
            ),
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
            TypeMapping::RescopeReturnCallables(replacements) => {
                Self::RescopeReturnCallables(replacements.iter().map(|(a, b)| (*a, *b)).collect())
            }
            TypeMapping::ApplyRecursiveSubstitution(_) | TypeMapping::EagerExpansion => {
                return None;
            }
        })
    }

    fn is_idempotent(&self) -> bool {
        matches!(self, Self::Promote(..) | Self::ReplaceParameterDefaults)
    }

    fn with_mapping<R>(&self, f: &mut dyn FnMut(TypeMapping<'_, 'db>) -> R) -> R {
        match self {
            Self::Specialize(specialization, kind) => {
                specialization.with_mapping(&mut |specialization| {
                    f(match kind {
                        Some(kind) => TypeMapping::ApplySpecializationWithMaterialization {
                            specialization,
                            materialization_kind: *kind,
                        },
                        None => TypeMapping::ApplySpecialization(specialization),
                    })
                })
            }
            Self::Promote(mode, kind) => f(TypeMapping::Promote(*mode, *kind)),
            Self::BindLegacyTypevars(context) => f(TypeMapping::BindLegacyTypevars(*context)),
            Self::FreshenBoundTypeVars(context, delta) => f(TypeMapping::FreshenBoundTypeVars {
                generic_context: *context,
                delta: *delta,
            }),
            Self::BindSelf(binding) => f(TypeMapping::BindSelf(binding.clone())),
            Self::ReplaceSelf(ty) => f(TypeMapping::ReplaceSelf {
                new_upper_bound: *ty,
            }),
            Self::Materialize(kind) => f(TypeMapping::Materialize(*kind)),
            Self::ReplaceParameterDefaults => f(TypeMapping::ReplaceParameterDefaults),
            Self::RescopeReturnCallables(replacements) => f(TypeMapping::RescopeReturnCallables(
                &replacements.iter().copied().collect(),
            )),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, get_size2::GetSize, salsa::SalsaValue)]
pub(super) enum OwnedSpecialization<'db> {
    Specialization(Specialization<'db>, bool),
    TypeAlias(Specialization<'db>),
    Partial(GenericContext<'db>, Box<[Type<'db>]>, Option<usize>),
    ReturnCallables(Box<[(BoundTypeVarInstance<'db>, BoundTypeVarInstance<'db>)]>),
    Single(BoundTypeVarInstance<'db>, Type<'db>),
    WithBindings(Box<Self>, Box<[(BoundTypeVarInstance<'db>, Type<'db>)]>),
}

impl<'db> OwnedSpecialization<'db> {
    /// The substitutions shown in the marker, including overrides not in its generic context.
    pub(super) fn bindings(
        &self,
        db: &'db dyn Db,
    ) -> FxIndexMap<BoundTypeVarInstance<'db>, Type<'db>> {
        match self {
            Self::Specialization(specialization, _) | Self::TypeAlias(specialization) => {
                specialization
                    .generic_context(db)
                    .variables(db)
                    .zip(specialization.types(db).iter().copied())
                    .collect()
            }
            Self::Partial(context, types, skip) => context
                .variables(db)
                .enumerate()
                .filter_map(|(index, variable)| {
                    if skip == &Some(index) {
                        Some((variable, Type::Never))
                    } else {
                        types.get(index).map(|ty| (variable, *ty))
                    }
                })
                .collect(),
            Self::ReturnCallables(replacements) => replacements
                .iter()
                .map(|(from, to)| (*from, Type::TypeVar(*to)))
                .collect(),
            Self::Single(variable, ty) => [(*variable, *ty)].into_iter().collect(),
            Self::WithBindings(specialization, overrides) => {
                let mut bindings = specialization.bindings(db);
                bindings.extend(overrides.iter().copied());
                bindings
            }
        }
    }

    fn new(specialization: ApplySpecialization<'_, 'db>) -> Self {
        match specialization {
            ApplySpecialization::Specialization {
                specialization,
                specialize_self_domain,
            } => Self::Specialization(specialization, specialize_self_domain),
            ApplySpecialization::TypeAlias(specialization) => Self::TypeAlias(specialization),
            ApplySpecialization::Partial {
                generic_context,
                types,
                skip,
            } => Self::Partial(generic_context, types.into(), skip),
            ApplySpecialization::ReturnCallables(replacements) => {
                Self::ReturnCallables(replacements.iter().map(|(a, b)| (*a, *b)).collect())
            }
            ApplySpecialization::Single(variable, ty) => Self::Single(variable, ty),
            ApplySpecialization::WithBindings {
                specialization,
                bindings,
            } => Self::WithBindings(Box::new(Self::new(*specialization)), bindings.into()),
        }
    }

    fn with_mapping<R>(&self, f: &mut dyn FnMut(ApplySpecialization<'_, 'db>) -> R) -> R {
        match self {
            Self::Specialization(specialization, specialize_self_domain) => {
                f(ApplySpecialization::Specialization {
                    specialization: *specialization,
                    specialize_self_domain: *specialize_self_domain,
                })
            }
            Self::TypeAlias(specialization) => f(ApplySpecialization::TypeAlias(*specialization)),
            Self::Partial(context, types, skip) => f(ApplySpecialization::Partial {
                generic_context: *context,
                types,
                skip: *skip,
            }),
            Self::ReturnCallables(replacements) => {
                let replacements: FxIndexMap<_, _> = replacements.iter().copied().collect();
                f(ApplySpecialization::ReturnCallables(&replacements))
            }
            Self::Single(variable, ty) => f(ApplySpecialization::Single(*variable, *ty)),
            Self::WithBindings(specialization, bindings) => {
                specialization.with_mapping(&mut |specialization| {
                    f(ApplySpecialization::WithBindings {
                        specialization: &specialization,
                        bindings,
                    })
                })
            }
        }
    }
}
