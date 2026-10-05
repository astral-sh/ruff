//! Locate type-variable occurrences before moving callable-only variables into returned callables.

use std::convert::Infallible;
use std::slice;

use rustc_hash::{FxHashMap, FxHashSet};

use crate::types::cyclic::TypeIdentity;
use crate::types::signatures::Parameters;
use crate::types::visitor::{
    NonAtomicType, OrdinaryTypeWalk, SyncTypeWalkEffects, TypeKind, TypeWalkCursor, TypeWalkEvent,
    TypeWalkPolicy, Unrestricted, WalkAction,
};
use crate::types::{
    BoundTypeVarInstance, CallableType, Parameter, RecursiveType, Type, TypeAliasType,
};
use crate::{Db, FxOrderSet, ProgramEnvironment};

/// Occurrences outside returned callables, and occurrences grouped by their outermost callable.
#[derive(Debug, Default)]
pub(in crate::types) struct TypeVarLocations<'db> {
    /// The set of typevars that appear somewhere other than in a `Callable` in the return
    /// type.
    pub(in crate::types) found_outside_callable_return: FxHashSet<BoundTypeVarInstance<'db>>,
    /// A map containing each outermost `Callable` in the return type that has typevar occurrences,
    /// along with those typevars. Nested callables contribute to their outermost callable's entry.
    /// (Note that at this point, we have not yet determined if those typevars _only_ appear there.)
    pub(in crate::types) found_inside_callable_return:
        FxHashMap<CallableType<'db>, FxOrderSet<BoundTypeVarInstance<'db>>>,
}

/// The interpretation of an occurrence; the same type can appear in several such contexts.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(in crate::types) enum OccurrenceLocation<'db> {
    Parameter,
    Return,
    Callable(CallableType<'db>),
}

/// Restores an enclosing occurrence context, and retains an alias identity while its child is active.
#[derive(Clone, Copy, Debug)]
pub(in crate::types) struct LocationBoundary<'db> {
    pub(in crate::types) previous: OccurrenceLocation<'db>,
    pub(in crate::types) active_alias: Option<TypeIdentity<'db>>,
}

/// Flat pending traversal and its unpublished occurrence sets.
/// Active aliases live in the boundary stack independently of completed visits.
pub(in crate::types) struct LocationState<'db> {
    pub(in crate::types) cursor: TypeWalkCursor<'db>,
    pub(in crate::types) location: OccurrenceLocation<'db>,
    pub(in crate::types) boundaries: Vec<LocationBoundary<'db>>,
    pub(in crate::types) seen: FxHashSet<(Type<'db>, OccurrenceLocation<'db>)>,
    pub(in crate::types) locations: TypeVarLocations<'db>,
}

impl std::fmt::Debug for LocationState<'_> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("LocationState")
            .field("pending", &self.cursor.pending.len())
            .field("boundaries", &self.boundaries.len())
            .field("seen", &self.seen.len())
            .finish_non_exhaustive()
    }
}

impl Default for LocationState<'_> {
    fn default() -> Self {
        Self {
            cursor: TypeWalkCursor {
                pending: smallvec::SmallVec::new(),
            },
            location: OccurrenceLocation::Parameter,
            boundaries: Vec::new(),
            seen: FxHashSet::default(),
            locations: TypeVarLocations::default(),
        }
    }
}

/// Finite classification used by the occurrence walk; child enumeration stays in the type walker.
#[derive(Clone, Copy, Debug)]
pub(in crate::types) struct LocationFacts;

ty_mapping_probe_macros::shared_semantic_family! {
    #[synchronous(SynchronousLocationEffects)]
    pub(in crate::types) trait LocationEffects<'db> {
        type Error;
        #[operation(local)]
        async fn new_state(&self) -> Result<LocationState<'db>, Self::Error>;
        #[operation(local)]
        async fn parameters<'a>(&self, parameters: &'a Parameters<'db>) -> Result<slice::Iter<'a, Parameter<'db>>, Self::Error>;
        #[operation(local)]
        #[progress]
        async fn next_parameter(&self, parameters: &mut slice::Iter<'_, Parameter<'db>>) -> Result<Option<Type<'db>>, Self::Error>;
        #[operation(child)]
        async fn walk(&self, state: &mut LocationState<'db>, ty: Type<'db>, location: OccurrenceLocation<'db>) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn start(&self, state: &mut LocationState<'db>, location: OccurrenceLocation<'db>) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn push(&self, state: &mut LocationState<'db>, action: WalkAction<'db>) -> Result<(), Self::Error>;
        #[operation(child)]
        #[progress]
        async fn next(&self, state: &mut LocationState<'db>) -> Result<Option<TypeWalkEvent<'db>>, Self::Error>;
        #[operation(local)]
        async fn remember(&self, state: &mut LocationState<'db>, ty: Type<'db>) -> Result<bool, Self::Error>;
        #[operation(child)]
        async fn normalize(&self, variable: BoundTypeVarInstance<'db>) -> Result<BoundTypeVarInstance<'db>, Self::Error>;
        #[operation(local)]
        async fn record(&self, state: &mut LocationState<'db>, variable: BoundTypeVarInstance<'db>) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn enter_callable(&self, state: &mut LocationState<'db>, callable: CallableType<'db>) -> Result<bool, Self::Error>;
        #[operation(child)]
        async fn identity(&self, ty: Type<'db>) -> Result<TypeIdentity<'db>, Self::Error>;
        #[operation(local)]
        async fn enter_alias(&self, state: &mut LocationState<'db>, identity: TypeIdentity<'db>) -> Result<bool, Self::Error>;
        #[operation(local)]
        async fn leave(&self, state: &mut LocationState<'db>) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn alias_value(&self, alias: TypeAliasType<'db>) -> Result<Type<'db>, Self::Error>;
        #[operation(child)]
        async fn recursive_unfold(&self, recursive: RecursiveType<'db>) -> Result<Type<'db>, Self::Error>;
        #[operation(child)]
        async fn expand(&self, state: &mut LocationState<'db>, kind: NonAtomicType<'db>) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn finish(&self, state: &mut LocationState<'db>) -> Result<TypeVarLocations<'db>, Self::Error>;
    }

    #[finite_capability]
    impl LocationFacts {
        fn kind<'db>(&self, ty: Type<'db>) -> TypeKind<'db> { <TypeKind<'db> as From<Type<'db>>>::from(ty) }
    }

    /// Collects parameter occurrences first, then return occurrences outside or within outermost callables.
    #[synchronous(collect_locations_sync)]
    #[capabilities(effects = LocationEffects)]
    #[passive_values(OccurrenceLocation::Parameter, OccurrenceLocation::Return)]
    pub(in crate::types) async fn collect_locations_with<'db, E: LocationEffects<'db>>(
        parameters: &Parameters<'db>, return_type: Type<'db>, effects: &E,
    ) -> Result<TypeVarLocations<'db>, E::Error> {
        let mut state = effects.new_state().await?;
        let mut parameters = effects.parameters(parameters).await?;
        #[cursor_loop]
        while let Some(ty) = effects.next_parameter(&mut parameters).await? {
            effects.walk(&mut state, ty, OccurrenceLocation::Parameter).await?;
        }
        effects.walk(&mut state, return_type, OccurrenceLocation::Return).await?;
        effects.finish(&mut state).await
    }

    /// Walks stored child edges in order, retaining occurrence and recursion boundaries across children.
    #[synchronous(walk_locations_sync)]
    #[capabilities(effects = LocationEffects, facts = LocationFacts)]
    #[passive_values(WalkAction::Visit, WalkAction::Expand, WalkAction::EndScope, Type::TypeAlias, Type::Recursive)]
    pub(in crate::types) async fn walk_locations_with<'db, E: LocationEffects<'db>>(
        state: &mut LocationState<'db>, ty: Type<'db>, location: OccurrenceLocation<'db>, facts: LocationFacts, effects: &E,
    ) -> Result<(), E::Error> {
        effects.start(state, location).await?;
        effects.push(state, WalkAction::Visit(ty)).await?;
        #[cursor_loop]
        while let Some(event) = effects.next(state).await? {
            match event {
                TypeWalkEvent::SkippedLazy | TypeWalkEvent::ExitDepth { .. } => {},
                TypeWalkEvent::EndScope => effects.leave(state).await?,
                TypeWalkEvent::Visit(ty) => {
                    if let TypeKind::NonAtomic(kind) = facts.kind(ty)
                        && effects.remember(state, ty).await?
                    {
                        effects.push(state, WalkAction::Expand(kind)).await?;
                    }
                }
                TypeWalkEvent::Expand(kind) => {
                    match kind {
                        NonAtomicType::TypeVar(variable) => {
                            let variable = effects.normalize(variable).await?;
                            effects.record(state, variable).await?;
                        }
                        NonAtomicType::Callable(callable) => {
                            if effects.enter_callable(state, callable).await? {
                                effects.push(state, WalkAction::EndScope).await?;
                            }
                            effects.expand(state, kind).await?;
                        }
                        NonAtomicType::TypeAlias(alias) => {
                            let identity = effects.identity(Type::TypeAlias(alias)).await?;
                            if effects.enter_alias(state, identity).await? {
                                effects.push(state, WalkAction::EndScope).await?;
                                let value = effects.alias_value(alias).await?;
                                effects.push(state, WalkAction::Visit(value)).await?;
                            }
                        }
                        NonAtomicType::Recursive(recursive) => {
                            let identity = effects.identity(Type::Recursive(recursive)).await?;
                            if effects.enter_alias(state, identity).await? {
                                effects.push(state, WalkAction::EndScope).await?;
                                let value = effects.recursive_unfold(recursive).await?;
                                effects.push(state, WalkAction::Visit(value)).await?;
                            }
                        }
                        NonAtomicType::Union(_) | NonAtomicType::Intersection(_)
                        | NonAtomicType::EnumComplement(_) | NonAtomicType::FunctionLiteral(_)
                        | NonAtomicType::BoundMethod(_) | NonAtomicType::BoundSuper(_)
                        | NonAtomicType::MethodWrapper(_) | NonAtomicType::GenericAlias(_)
                        | NonAtomicType::KnownInstance(_) | NonAtomicType::SubclassOf(_)
                        | NonAtomicType::NominalInstance(_) | NonAtomicType::PropertyInstance(_)
                        | NonAtomicType::SlotDescriptor(_) | NonAtomicType::TypeIs(_)
                        | NonAtomicType::TypeGuard(_) | NonAtomicType::TypeForm(_)
                        | NonAtomicType::ProtocolInstance(_) | NonAtomicType::TypedDict(_)
                        | NonAtomicType::NewTypeInstance(_) => effects.expand(state, kind).await?,
                    }
                }
            }
        }
        Ok(())
    }
}

/// Ordinary field and semantic children for collecting return-callable locations.
pub(in crate::types) struct OrdinaryLocationEffects<'env, 'db> {
    pub(in crate::types) db: &'db dyn Db,
    pub(in crate::types) env: &'env ProgramEnvironment<'db>,
}

impl<'db> SynchronousLocationEffects<'db> for OrdinaryLocationEffects<'_, 'db> {
    type Error = Infallible;

    fn new_state(&self) -> Result<LocationState<'db>, Infallible> {
        Ok(LocationState::default())
    }
    fn parameters<'a>(
        &self,
        parameters: &'a Parameters<'db>,
    ) -> Result<slice::Iter<'a, Parameter<'db>>, Infallible> {
        Ok(parameters.iter())
    }
    fn next_parameter(
        &self,
        parameters: &mut slice::Iter<'_, Parameter<'db>>,
    ) -> Result<Option<Type<'db>>, Infallible> {
        Ok(parameters.next().map(Parameter::annotated_type))
    }
    fn walk(
        &self,
        state: &mut LocationState<'db>,
        ty: Type<'db>,
        location: OccurrenceLocation<'db>,
    ) -> Result<(), Infallible> {
        walk_locations_sync(state, ty, location, LocationFacts, self)
    }
    fn start(
        &self,
        state: &mut LocationState<'db>,
        location: OccurrenceLocation<'db>,
    ) -> Result<(), Infallible> {
        state.location = location;
        Ok(())
    }
    fn push(
        &self,
        state: &mut LocationState<'db>,
        action: WalkAction<'db>,
    ) -> Result<(), Infallible> {
        OrdinaryTypeWalk {
            db: self.db,
            env: self.env,
            control: &mut Unrestricted,
            query: (),
        }
        .push_action(&mut state.cursor, action)
    }
    fn next(
        &self,
        state: &mut LocationState<'db>,
    ) -> Result<Option<TypeWalkEvent<'db>>, Infallible> {
        OrdinaryTypeWalk {
            db: self.db,
            env: self.env,
            control: &mut Unrestricted,
            query: (),
        }
        .next_event(&mut state.cursor, TypeWalkPolicy::locations())
    }
    fn remember(&self, state: &mut LocationState<'db>, ty: Type<'db>) -> Result<bool, Infallible> {
        Ok(state.seen.insert((ty, state.location)))
    }
    fn normalize(
        &self,
        variable: BoundTypeVarInstance<'db>,
    ) -> Result<BoundTypeVarInstance<'db>, Infallible> {
        Ok(if variable.is_paramspec(self.db) {
            variable.without_paramspec_attr(self.db)
        } else {
            variable
        })
    }
    fn record(
        &self,
        state: &mut LocationState<'db>,
        variable: BoundTypeVarInstance<'db>,
    ) -> Result<(), Infallible> {
        match state.location {
            OccurrenceLocation::Parameter | OccurrenceLocation::Return => {
                state
                    .locations
                    .found_outside_callable_return
                    .insert(variable);
            }
            OccurrenceLocation::Callable(callable) => {
                state
                    .locations
                    .found_inside_callable_return
                    .entry(callable)
                    .or_default()
                    .insert(variable);
            }
        }
        Ok(())
    }
    fn enter_callable(
        &self,
        state: &mut LocationState<'db>,
        callable: CallableType<'db>,
    ) -> Result<bool, Infallible> {
        if state.location != OccurrenceLocation::Return {
            return Ok(false);
        }
        state.boundaries.push(LocationBoundary {
            previous: state.location,
            active_alias: None,
        });
        state.location = OccurrenceLocation::Callable(callable);
        Ok(true)
    }
    fn identity(&self, ty: Type<'db>) -> Result<TypeIdentity<'db>, Infallible> {
        Ok(ty.to_type_identity(self.db))
    }
    fn enter_alias(
        &self,
        state: &mut LocationState<'db>,
        identity: TypeIdentity<'db>,
    ) -> Result<bool, Infallible> {
        if state
            .boundaries
            .iter()
            .any(|boundary| boundary.active_alias == Some(identity))
        {
            return Ok(false);
        }
        state.boundaries.push(LocationBoundary {
            previous: state.location,
            active_alias: Some(identity),
        });
        Ok(true)
    }
    fn leave(&self, state: &mut LocationState<'db>) -> Result<(), Infallible> {
        if let Some(boundary) = state.boundaries.pop() {
            state.location = boundary.previous;
        }
        Ok(())
    }
    fn alias_value(&self, alias: TypeAliasType<'db>) -> Result<Type<'db>, Infallible> {
        Ok(alias.value_type(self.db))
    }
    fn recursive_unfold(&self, recursive: RecursiveType<'db>) -> Result<Type<'db>, Infallible> {
        Ok(recursive.unfold(self.db, self.env).into_type())
    }
    fn expand(
        &self,
        state: &mut LocationState<'db>,
        kind: NonAtomicType<'db>,
    ) -> Result<(), Infallible> {
        OrdinaryTypeWalk {
            db: self.db,
            env: self.env,
            control: &mut Unrestricted,
            query: (),
        }
        .expand_children(&mut state.cursor, kind, TypeWalkPolicy::locations())
    }
    fn finish(&self, state: &mut LocationState<'db>) -> Result<TypeVarLocations<'db>, Infallible> {
        Ok(std::mem::take(&mut state.locations))
    }
}
