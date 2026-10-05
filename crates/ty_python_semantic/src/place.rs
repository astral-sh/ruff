pub(crate) mod builtin_lookup;
pub(crate) mod definitions;
pub(crate) mod implicit_effects;
pub(crate) mod implicit_symbol;
pub(crate) mod imported;
pub(crate) mod normalization;
pub(crate) mod source_effects;

use self::builtin_lookup::{BuiltinVisibility, InlineBuiltinLookupEffects, builtins_symbol_sync};
use self::implicit_symbol::{OrdinaryClassBodySymbolEffects, class_body_implicit_symbol_sync};
use self::normalization::{
    OrdinaryPlaceNormalizationEffects, PlaceNormalizationFacts, place_cycle_normalized_sync,
};
use self::source_effects::{
    LegacyInlineEffects, PublicLookupEffects, SourcePlaceEffects, SourcePlaceWork,
    reachability_with,
};
use crate::ProgramEnvironment;
use crate::types::legacy_inline;
use ruff_index::IndexSlice;
use rustc_hash::FxHashMap;
use ty_module_resolver::{KnownModule, resolve_module_confident};

use crate::reachability::{ReachabilityEvaluationCache, evaluate_reachability};
use crate::types::{
    DynamicType, KnownClass, Type, TypeAndQualifiers, TypeQualifiers, UnionBuilder, UnionType,
    is_discarded_dict_key_assignment,
};
use crate::{Db, FxIndexSet, FxOrderSet};
use ty_python_core::definition::{Definition, DefinitionKind, DefinitionState};
use ty_python_core::narrowing_constraints::ScopedNarrowingConstraint;
use ty_python_core::place::ScopedPlaceId;
use ty_python_core::predicate::{Predicate, ScopedPredicateId};
use ty_python_core::reachability_constraints::{
    ReachabilityConstraints, ScopedReachabilityConstraintId,
};
use ty_python_core::scope::{Scope, ScopeId};
use ty_python_core::{
    BindingWithConstraints, BindingWithConstraintsIterator, BoundnessAnalysis,
    DeclarationWithConstraint, DeclarationsIterator, ImportedFinalCandidatesIterator, ProgramFile,
    Truthiness, UseDefMap, global_scope, place_table, use_def_map,
};

#[cfg(any(test, feature = "experimental-analysis"))]
pub(crate) use implicit_globals::{
    module_type_body_scope_with, module_type_implicit_global_declaration_with,
};
pub(crate) use implicit_globals::{
    module_type_implicit_global_declaration, module_type_implicit_global_symbol,
};

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq, get_size2::GetSize)]
pub(crate) enum Definedness {
    AlwaysDefined,
    PossiblyUndefined,
}

impl Definedness {
    pub(crate) const fn max(self, other: Self) -> Self {
        match (self, other) {
            (Definedness::AlwaysDefined, _) | (_, Definedness::AlwaysDefined) => {
                Definedness::AlwaysDefined
            }
            (Definedness::PossiblyUndefined, Definedness::PossiblyUndefined) => {
                Definedness::PossiblyUndefined
            }
        }
    }
}

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq, get_size2::GetSize)]
pub(crate) enum TypeOrigin {
    Declared,
    Inferred,
}

impl TypeOrigin {
    pub(crate) const fn is_declared(self) -> bool {
        matches!(self, TypeOrigin::Declared)
    }

    pub(crate) const fn merge(self, other: Self) -> Self {
        match (self, other) {
            (TypeOrigin::Declared, TypeOrigin::Declared) => TypeOrigin::Declared,
            _ => TypeOrigin::Inferred,
        }
    }
}

/// How a place's raw type should be adjusted when accessed publicly.
///
/// For undeclared public symbols (e.g., class attributes without type annotations),
/// we store the raw inferred type and lazily apply the public-type policy when
/// converting the place into a public lookup result.
#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq, Default, get_size2::GetSize)]
pub(crate) enum PublicTypePolicy {
    /// Public lookup should expose the raw stored type.
    #[default]
    Raw,
    /// Public lookup should expose the promoted stored type.
    Promote,
}

impl PublicTypePolicy {
    /// Apply the public-type policy to the raw type.
    async fn apply_if_needed_with<'db, E: PublicLookupEffects<'db>>(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        effects: &E,
        ty: Type<'db>,
    ) -> Result<Type<'db>, E::Error> {
        match self {
            Self::Raw => Ok(ty),
            Self::Promote => effects.promote_public_type(db, env, ty).await,
        }
    }
}

/// The source definition provenance for a place.
#[derive(
    Debug, Clone, Copy, Default, Hash, PartialEq, Eq, get_size2::GetSize, salsa::SalsaValue,
)]
pub(crate) enum Provenance<'db> {
    /// No source definition is known.
    #[default]
    Unknown,
    /// Exactly one source definition is known.
    SingleDefinition(Definition<'db>),
    /// Multiple distinct source definitions contribute to the place. Instead of storing all of
    /// them, or selecting one arbitrarily, we currently discard provenance information in this
    /// case.
    MultipleDefinitions,
}

impl<'db> Provenance<'db> {
    pub(crate) fn from_definition(definition: Option<Definition<'db>>) -> Self {
        definition.map_or(Self::Unknown, Self::SingleDefinition)
    }

    /// Merge the provenance from two places.
    pub(crate) fn or(self, other: Self) -> Self {
        match (self, other) {
            (Self::Unknown, provenance) | (provenance, Self::Unknown) => provenance,
            (Self::MultipleDefinitions, _) | (_, Self::MultipleDefinitions) => {
                Self::MultipleDefinitions
            }
            (Self::SingleDefinition(left), Self::SingleDefinition(right)) if left == right => self,
            (Self::SingleDefinition(_), Self::SingleDefinition(_)) => Self::MultipleDefinitions,
        }
    }

    pub(crate) fn definition(self) -> Option<Definition<'db>> {
        match self {
            Self::SingleDefinition(definition) => Some(definition),
            Self::Unknown | Self::MultipleDefinitions => None,
        }
    }
}

/// A defined place with its raw type, origin, definedness, public-type policy, and provenance.
#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq, get_size2::GetSize, salsa::SalsaValue)]
pub(crate) struct DefinedPlace<'db> {
    pub(crate) ty: Type<'db>,
    pub(crate) origin: TypeOrigin,
    pub(crate) definedness: Definedness,
    pub(crate) public_type_policy: PublicTypePolicy,
    pub(crate) provenance: Provenance<'db>,
}

impl<'db> DefinedPlace<'db> {
    fn new(ty: Type<'db>) -> Self {
        Self {
            ty,
            origin: TypeOrigin::Inferred,
            definedness: Definedness::AlwaysDefined,
            public_type_policy: PublicTypePolicy::Raw,
            provenance: Provenance::Unknown,
        }
    }

    fn with_origin(mut self, origin: TypeOrigin) -> Self {
        self.origin = origin;
        self
    }

    pub(crate) fn with_definedness(mut self, definedness: Definedness) -> Self {
        self.definedness = definedness;
        self
    }

    fn with_public_type_policy(mut self, public_type_policy: PublicTypePolicy) -> Self {
        self.public_type_policy = public_type_policy;
        self
    }

    fn with_definition(mut self, definition: Definition<'db>) -> Self {
        self.provenance = Provenance::SingleDefinition(definition);
        self
    }

    fn with_provenance(mut self, provenance: Provenance<'db>) -> Self {
        self.provenance = provenance;
        self
    }

    pub(crate) const fn is_definitely_defined(&self) -> bool {
        matches!(self.definedness, Definedness::AlwaysDefined)
    }
}

/// The result of a place lookup, which can either be a (possibly undefined) type
/// or a completely undefined place.
///
/// If a place has both a binding and a declaration, the result of the binding is used.
///
/// Consider this example:
/// ```py
/// bound = 1
/// declared: int
///
/// if flag:
///     possibly_unbound = 2
///     possibly_undeclared: int
///
/// if flag:
///     bound_or_declared = 1
/// else:
///     bound_or_declared: int
/// ```
///
/// If we look up places in this scope, we would get the following results:
/// ```rs
/// bound:               Place::Defined(DefinedPlace { ty: Literal[1], origin: TypeOrigin::Inferred, definedness: Definedness::AlwaysDefined, .. }),
/// declared:            Place::Defined(DefinedPlace { ty: int, origin: TypeOrigin::Declared, definedness: Definedness::AlwaysDefined, .. }),
/// possibly_unbound:    Place::Defined(DefinedPlace { ty: Literal[2], origin: TypeOrigin::Inferred, definedness: Definedness::PossiblyUndefined, .. }),
/// possibly_undeclared: Place::Defined(DefinedPlace { ty: int, origin: TypeOrigin::Declared, definedness: Definedness::PossiblyUndefined, .. }),
/// bound_or_declared:   Place::Defined(DefinedPlace { ty: Literal[1], origin: TypeOrigin::Inferred, definedness: Definedness::PossiblyUndefined, .. }),
/// non_existent:        Place::Undefined,
/// ```
#[derive(
    Debug, Clone, Copy, Default, Hash, PartialEq, Eq, get_size2::GetSize, salsa::SalsaValue,
)]
pub(crate) enum Place<'db> {
    Defined(DefinedPlace<'db>),
    #[default]
    Undefined,
}

impl<'db> Place<'db> {
    /// Constructor that creates a [`Place`] with type origin [`TypeOrigin::Inferred`] and definedness [`Definedness::AlwaysDefined`].
    pub(crate) fn bound(ty: impl Into<Type<'db>>) -> Self {
        Place::Defined(DefinedPlace::new(ty.into()))
    }

    /// Constructor that creates a [`Place`] with type origin [`TypeOrigin::Declared`] and definedness [`Definedness::AlwaysDefined`].
    pub(crate) fn declared(ty: impl Into<Type<'db>>) -> Self {
        Place::Defined(DefinedPlace::new(ty.into()).with_origin(TypeOrigin::Declared))
    }

    /// Returns this place with the given definition attached. A no-op for [`Place::Undefined`].
    #[must_use]
    pub(crate) fn with_definition(self, definition: Definition<'db>) -> Self {
        match self {
            Place::Defined(defined) => Place::Defined(defined.with_definition(definition)),
            Place::Undefined => Place::Undefined,
        }
    }

    /// Returns this place with the given provenance attached. A no-op for [`Place::Undefined`].
    #[must_use]
    pub(crate) fn with_provenance(self, provenance: Provenance<'db>) -> Self {
        match self {
            Place::Defined(defined) => Place::Defined(defined.with_provenance(provenance)),
            Place::Undefined => Place::Undefined,
        }
    }

    pub(crate) fn is_equal_ignoring_provenance(self, other: Self) -> bool {
        match (self, other) {
            (Place::Defined(left), Place::Defined(right)) => {
                left.ty == right.ty
                    && left.origin == right.origin
                    && left.definedness == right.definedness
                    && left.public_type_policy == right.public_type_policy
            }
            (Place::Undefined, Place::Undefined) => true,
            _ => false,
        }
    }

    pub(crate) fn is_undefined(&self) -> bool {
        matches!(self, Place::Undefined)
    }

    /// Returns the type of the place, ignoring possible undefinedness.
    ///
    /// If the place is *definitely* undefined, this function will return `None`. Otherwise,
    /// if there is at least one control-flow path where the place is defined, return the type.
    pub(crate) fn ignore_possibly_undefined(&self) -> Option<Type<'db>> {
        match self {
            Place::Defined(defined) => Some(defined.ty),
            Place::Undefined => None,
        }
    }

    /// Returns the raw stored type of the place.
    ///
    /// Any public-type adjustment is applied lazily when converting to `LookupResult`.
    pub(crate) fn raw_type(&self) -> Option<Type<'db>> {
        match self {
            Place::Defined(defined) => Some(defined.ty),
            Place::Undefined => None,
        }
    }

    #[cfg(test)]
    #[track_caller]
    pub(crate) fn expect_type(self) -> Type<'db> {
        self.ignore_possibly_undefined()
            .expect("Expected a (possibly undefined) type, not an undefined place")
    }

    #[must_use]
    fn map_type(self, f: impl FnOnce(Type<'db>) -> Type<'db>) -> Place<'db> {
        match self {
            Place::Defined(defined) => Place::Defined(DefinedPlace {
                ty: f(defined.ty),
                ..defined
            }),
            Place::Undefined => Place::Undefined,
        }
    }

    /// Set the public-type policy for this place.
    #[must_use]
    fn with_public_type_policy(self, new_public_type_policy: PublicTypePolicy) -> Place<'db> {
        match self {
            Place::Defined(defined) => {
                Place::Defined(defined.with_public_type_policy(new_public_type_policy))
            }
            Place::Undefined => Place::Undefined,
        }
    }

    #[must_use]
    pub(crate) fn with_qualifiers(self, qualifiers: TypeQualifiers) -> PlaceAndQualifiers<'db> {
        PlaceAndQualifiers {
            place: self,
            qualifiers,
        }
    }

    pub(crate) const fn is_definitely_bound(&self) -> bool {
        matches!(
            self,
            Place::Defined(DefinedPlace {
                definedness: Definedness::AlwaysDefined,
                ..
            })
        )
    }
}

impl<'db> From<LookupResult<'db>> for PlaceAndQualifiers<'db> {
    fn from(value: LookupResult<'db>) -> Self {
        match value {
            Ok(type_and_qualifiers) => Place::Defined(
                DefinedPlace::new(type_and_qualifiers.inner_type())
                    .with_origin(type_and_qualifiers.origin())
                    .with_provenance(type_and_qualifiers.provenance()),
            )
            .with_qualifiers(type_and_qualifiers.qualifiers()),
            Err(LookupError::Undefined(qualifiers)) => Place::Undefined.with_qualifiers(qualifiers),
            Err(LookupError::PossiblyUndefined(type_and_qualifiers)) => Place::Defined(
                DefinedPlace::new(type_and_qualifiers.inner_type())
                    .with_origin(type_and_qualifiers.origin())
                    .with_definedness(Definedness::PossiblyUndefined)
                    .with_provenance(type_and_qualifiers.provenance()),
            )
            .with_qualifiers(type_and_qualifiers.qualifiers()),
        }
    }
}

/// Possible ways in which a place lookup can (possibly or definitely) fail.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub(crate) enum LookupError<'db> {
    Undefined(TypeQualifiers),
    PossiblyUndefined(TypeAndQualifiers<'db>),
}

impl<'db> LookupError<'db> {
    /// Fallback (wholly or partially) to `fallback` to create a new [`LookupResult`].
    pub(crate) fn or_fall_back_to(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        fallback: PlaceAndQualifiers<'db>,
    ) -> LookupResult<'db> {
        legacy_inline(self.or_fall_back_to_with(db, env, &LegacyInlineEffects::new(db), fallback))
    }

    pub(crate) async fn or_fall_back_to_with<E: PublicLookupEffects<'db>>(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        effects: &E,
        fallback: PlaceAndQualifiers<'db>,
    ) -> Result<LookupResult<'db>, E::Error> {
        let fallback = fallback.into_lookup_result_with(db, env, effects).await?;
        Ok(match (&self, &fallback) {
            (LookupError::Undefined(_), _) => fallback,
            (LookupError::PossiblyUndefined { .. }, Err(LookupError::Undefined(_))) => Err(self),
            (LookupError::PossiblyUndefined(ty), Ok(ty2)) => Ok(TypeAndQualifiers::new(
                effects
                    .union_two(db, env, ty.inner_type(), ty2.inner_type())
                    .await?,
                ty.origin().merge(ty2.origin()),
                ty.qualifiers().union(ty2.qualifiers()),
            )
            .with_provenance(ty.provenance().or(ty2.provenance()))),
            (LookupError::PossiblyUndefined(ty), Err(LookupError::PossiblyUndefined(ty2))) => {
                Err(LookupError::PossiblyUndefined(
                    TypeAndQualifiers::new(
                        effects
                            .union_two(db, env, ty.inner_type(), ty2.inner_type())
                            .await?,
                        ty.origin().merge(ty2.origin()),
                        ty.qualifiers().union(ty2.qualifiers()),
                    )
                    .with_provenance(ty.provenance().or(ty2.provenance())),
                ))
            }
        })
    }
}

/// A [`Result`] type in which the `Ok` variant represents a definitely bound place
/// and the `Err` variant represents a place that is either definitely or possibly unbound.
///
/// Note that this type is exactly isomorphic to [`Place`].
/// In the future, we could possibly consider removing `Place` and using this type everywhere instead.
pub(crate) type LookupResult<'db> = Result<TypeAndQualifiers<'db>, LookupError<'db>>;

/// Infer the public type of a symbol (its type as seen from outside its scope) in the given
/// `scope`.
#[allow(unused)]
pub(crate) fn symbol<'db>(
    db: &'db dyn Db,
    scope: ScopeId<'db>,
    name: &str,
    considered_definitions: ConsideredDefinitions,
) -> PlaceAndQualifiers<'db> {
    symbol_impl(
        db,
        scope,
        name,
        RequiresExplicitReExport::No,
        considered_definitions,
    )
}

/// Infers the public type of an explicit module-global symbol as seen from within the same file.
///
/// Note that all global scopes also include various "implicit globals" such as `__name__`,
/// `__doc__` and `__file__`. This function **does not** consider those symbols; it will return
/// `Place::Undefined` for them. Use the (currently test-only) `global_symbol` query to also include
/// those additional symbols.
///
/// Use [`imported_symbol`] to perform the lookup as seen from outside the file (e.g. via imports).
pub(crate) fn explicit_global_symbol<'db>(
    db: &'db dyn Db,
    file: ProgramFile<'db>,
    name: &str,
) -> PlaceAndQualifiers<'db> {
    symbol_impl(
        db,
        global_scope(db, file),
        name,
        RequiresExplicitReExport::No,
        ConsideredDefinitions::AllReachable,
    )
}

/// Infers the public type of an explicit module-global symbol as seen from within the same file.
///
/// Unlike [`explicit_global_symbol`], this function also considers various "implicit globals"
/// such as `__name__`, `__doc__` and `__file__`. These are looked up as attributes on `types.ModuleType`
/// rather than being looked up as symbols explicitly defined/declared in the global scope.
///
/// Use [`imported_symbol`] to perform the lookup as seen from outside the file (e.g. via imports).
#[allow(unused)]
pub(crate) fn global_symbol<'db>(
    db: &'db dyn Db,
    file: ProgramFile<'db>,
    name: &str,
) -> PlaceAndQualifiers<'db> {
    let env = ProgramEnvironment::from_file(file);
    explicit_global_symbol(db, file, name).or_fall_back_to(db, &env, || {
        module_type_implicit_global_symbol(db, file, name)
    })
}

/// Infers the public type of an imported symbol.
///
/// If `requires_explicit_reexport` is [`None`], it will be inferred from the file's source type.
/// For stub files, explicit re-export will be required, while for non-stub files, it will not.
///
/// `None` should be passed for the `file` parameter if looking up a symbol on a namespace package.
pub(crate) fn imported_symbol<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    file: Option<ProgramFile<'db>>,
    name: &str,
    requires_explicit_reexport: Option<RequiresExplicitReExport>,
) -> PlaceAndQualifiers<'db> {
    legacy_inline(imported_symbol_with(
        db,
        env,
        &LegacyInlineEffects::new(db),
        file,
        name,
        requires_explicit_reexport,
    ))
}

pub(crate) async fn imported_symbol_with<'db, E: SourcePlaceEffects<'db>>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    effects: &E,
    file: Option<ProgramFile<'db>>,
    name: &str,
    requires_explicit_reexport: Option<RequiresExplicitReExport>,
) -> Result<PlaceAndQualifiers<'db>, E::Error> {
    if let Some(file) = file {
        effects.check_imported_file(db, env, file).await?;
    }

    // If it's not found in the global scope, check if it's present as an instance on
    // `types.ModuleType` or `builtins.object`.
    //
    // We do a more limited version of this in `module_type_implicit_global_symbol`,
    // but there are two crucial differences here:
    // - If a member is looked up as an attribute, `__init__` is also available on the module, but
    //   it isn't available as a global from inside the module
    // - If a member is looked up as an attribute, members on `builtins.object` are also available
    //   (because `types.ModuleType` inherits from `object`); these attributes are also not
    //   available as globals from inside the module.
    //
    // The same way as in `module_type_implicit_global_symbol`, however, we need to be careful to
    // ignore `__getattr__`. Typeshed has a fake `__getattr__` on `types.ModuleType` to help out with
    // dynamic imports; we shouldn't use it for `ModuleLiteral` types where we know exactly which
    // module we're dealing with.
    let prior = if let Some(file) = file {
        let reexport = if let Some(reexport) = requires_explicit_reexport {
            reexport
        } else if effects.file_is_stub(db, file).await? {
            RequiresExplicitReExport::Yes
        } else {
            RequiresExplicitReExport::No
        };
        let scope = effects.global_scope(db, file).await?;
        symbol_with(
            db,
            effects,
            scope,
            name,
            reexport,
            ConsideredDefinitions::EndOfScope,
        )
        .await?
    } else {
        PlaceAndQualifiers::default()
    };
    if let Place::Defined(defined) = prior.place
        && defined.is_definitely_defined()
        && defined.public_type_policy == PublicTypePolicy::Raw
    {
        return Ok(prior);
    }
    effects.imported_fallback(db, env, prior, file, name).await
}

/// Lookup the type of `symbol` in the builtins namespace.
///
/// Returns `Place::Undefined` if the `builtins` module isn't available for some reason.
///
/// Note that this function is only intended for use in the context of the builtins *namespace*
/// and should not be used when a symbol is being explicitly imported from the `builtins` module
/// (e.g. `from builtins import int`).
pub(crate) fn builtins_symbol<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    symbol: &str,
) -> PlaceAndQualifiers<'db> {
    builtins_symbol_impl(db, env, symbol, BuiltinVisibility::All)
        .map(|(_, symbol)| symbol)
        .unwrap_or_default()
}

/// Looks up `symbol` for implicit builtin fallback.
///
/// Private type-checking-only definitions are implementation details, but private runtime
/// definitions from either the standard or project-level builtins remain available.
///
/// ```python
/// # builtins.pyi
/// _T = TypeVar("_T")  # Not available as an implicit builtin.
///
/// # __builtins__.pyi
/// _custom: int  # Available as an implicit builtin.
/// ```
pub(crate) fn implicit_builtins_symbol<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    symbol: &str,
) -> PlaceAndQualifiers<'db> {
    builtins_symbol_impl(db, env, symbol, BuiltinVisibility::RuntimeOnly)
        .map(|(_, symbol)| symbol)
        .unwrap_or_default()
}

/// Returns the module scope that supplies `symbol` through implicit builtin fallback.
///
/// Uses the same visibility rules as [`implicit_builtins_symbol`] so IDE definition lookup cannot
/// resolve a private typing-only helper that type inference considers undefined.
pub(crate) fn implicit_builtins_symbol_scope<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    symbol: &str,
) -> Option<ScopeId<'db>> {
    builtins_symbol_impl(db, env, symbol, BuiltinVisibility::RuntimeOnly).map(|(scope, _)| scope)
}

/// Resolves project-level builtins before standard builtins and optionally hides typing-only names.
///
/// Returns the supplying module's scope together with the symbol so inference and IDE lookups can
/// share the same resolution and visibility policy.
fn builtins_symbol_impl<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    symbol: &str,
    visibility: BuiltinVisibility,
) -> Option<(ScopeId<'db>, PlaceAndQualifiers<'db>)> {
    match builtins_symbol_sync(
        db,
        env,
        symbol,
        visibility,
        &InlineBuiltinLookupEffects::new(db),
    ) {
        Ok(found) => found,
        Err(error) => match error {},
    }
}

/// Lookup the type of `symbol` in a given known module.
///
/// Returns `Place::Undefined` if the given known module cannot be resolved for some reason.
pub(crate) fn known_module_symbol<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    known_module: KnownModule,
    symbol: &str,
) -> PlaceAndQualifiers<'db> {
    legacy_inline(known_module_symbol_with(
        db,
        env,
        &LegacyInlineEffects::new(db),
        known_module,
        symbol,
    ))
}

pub(crate) async fn known_module_symbol_with<'db, E: SourcePlaceEffects<'db>>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    effects: &E,
    known_module: KnownModule,
    symbol: &str,
) -> Result<PlaceAndQualifiers<'db>, E::Error> {
    let Some(file) = effects.resolve_known_module(db, env, known_module).await? else {
        return Ok(PlaceAndQualifiers::default());
    };
    imported_symbol_with(db, env, effects, Some(file), symbol, None).await
}

/// Lookup the type of `symbol` in the `typing` module namespace.
///
/// Returns `Place::Undefined` if the `typing` module isn't available for some reason.
#[inline]
#[cfg(test)]
pub(crate) fn typing_symbol<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    symbol: &str,
) -> PlaceAndQualifiers<'db> {
    known_module_symbol(db, env, KnownModule::Typing, symbol)
}

/// Lookup the type of `symbol` in the `typing_extensions` module namespace.
///
/// Returns `Place::Undefined` if the `typing_extensions` module isn't available for some reason.
#[inline]
pub(crate) fn typing_extensions_symbol<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    symbol: &str,
) -> PlaceAndQualifiers<'db> {
    known_module_symbol(db, env, KnownModule::TypingExtensions, symbol)
}

/// Get the `builtins` module scope.
///
/// Can return `None` if a custom typeshed is used that is missing `builtins.pyi`.
pub(crate) fn builtins_module_scope<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
) -> Option<ScopeId<'db>> {
    core_module_scope(db, env, KnownModule::Builtins)
}

/// Get the scope of a core stdlib module.
///
/// Can return `None` if a custom typeshed is used that is missing the core module in question.
fn core_module_scope<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    core_module: KnownModule,
) -> Option<ScopeId<'db>> {
    let program = env.program(db);
    let module = resolve_module_confident(db, env.resolver_environment(db), &core_module.name())?;
    Some(global_scope(
        db,
        ProgramFile::new(db, module.file(db)?, program),
    ))
}

/// Infer the combined type from an iterator of bindings, and return it
/// together with boundness information in a [`Place`].
///
/// The type will be a union if there are multiple bindings with different types.
pub(super) fn place_from_bindings<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    bindings_with_constraints: BindingWithConstraintsIterator<'_, 'db>,
) -> PlaceWithDefinition<'db> {
    place_from_bindings_impl(
        db,
        env,
        bindings_with_constraints,
        RequiresExplicitReExport::No,
        None,
    )
}

pub(super) fn place_from_bindings_with_reachability_cache<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    bindings_with_constraints: BindingWithConstraintsIterator<'_, 'db>,
    reachability_cache: &ReachabilityEvaluationCache<'db>,
) -> PlaceWithDefinition<'db> {
    place_from_bindings_impl(
        db,
        env,
        bindings_with_constraints,
        RequiresExplicitReExport::No,
        Some(reachability_cache),
    )
}

/// Build a declared type from a [`DeclarationsIterator`].
///
/// If there is only one declaration, or all declarations declare the same type, returns
/// `Ok(..)`. If there are conflicting declarations, returns an `Err(..)` variant with
/// a union of the declared types as well as a list of all conflicting types.
///
/// This function also returns declaredness information (see [`Place`]) and a set of
/// [`TypeQualifiers`] that have been specified on the declaration(s).
pub(crate) fn place_from_declarations<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    declarations: DeclarationsIterator<'_, 'db>,
) -> PlaceFromDeclarationsResult<'db> {
    place_from_declarations_impl(db, env, declarations, RequiresExplicitReExport::No, None)
}

pub(crate) fn place_from_declarations_with_reachability_cache<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    declarations: DeclarationsIterator<'_, 'db>,
    reachability_cache: &ReachabilityEvaluationCache<'db>,
) -> PlaceFromDeclarationsResult<'db> {
    place_from_declarations_impl(
        db,
        env,
        declarations,
        RequiresExplicitReExport::No,
        Some(reachability_cache),
    )
}

type DeclaredTypeAndConflictingTypes<'db> = (
    TypeAndQualifiers<'db>,
    Option<Box<indexmap::set::Slice<Type<'db>>>>,
);

/// The result of looking up a declared type from declarations; see [`place_from_declarations`].
#[derive(Debug, Default)]
pub(crate) struct PlaceFromDeclarationsResult<'db> {
    place_and_quals: PlaceAndQualifiers<'db>,
    conflicting_types: Option<Box<indexmap::set::Slice<Type<'db>>>>,
    /// Contains the first reachable declaration for this place, if any.
    /// This field is used for backreferences in diagnostics.
    pub(crate) first_declaration: Option<Definition<'db>>,
}

impl<'db> PlaceFromDeclarationsResult<'db> {
    pub(crate) fn qualifiers(&self) -> TypeQualifiers {
        self.place_and_quals.qualifiers
    }

    fn conflict(
        place_and_quals: PlaceAndQualifiers<'db>,
        conflicting_types: Box<indexmap::set::Slice<Type<'db>>>,
        first_declaration: Option<Definition<'db>>,
    ) -> Self {
        PlaceFromDeclarationsResult {
            place_and_quals,
            conflicting_types: Some(conflicting_types),
            first_declaration,
        }
    }

    /// Add any reachable imported `Final` qualifier without establishing a declared type.
    #[must_use]
    pub(crate) fn with_imported_final(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        candidates: ImportedFinalCandidatesIterator<'_, 'db>,
    ) -> Self {
        self.with_imported_final_impl(
            db,
            env,
            candidates,
            RequiresExplicitReExport::No,
            None,
            false,
        )
    }

    /// Also use an imported `Final`'s source type to constrain an assignment when the target has
    /// no declared type. An existing annotation always takes precedence over that source type.
    #[must_use]
    pub(crate) fn with_imported_final_for_assignment(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        candidates: ImportedFinalCandidatesIterator<'_, 'db>,
        reachability_cache: &ReachabilityEvaluationCache<'db>,
    ) -> Self {
        self.with_imported_final_impl(
            db,
            env,
            candidates,
            RequiresExplicitReExport::No,
            Some(reachability_cache),
            true,
        )
    }

    /// This can be called cross-module, so resolve imports through semantic queries without
    /// reading AST nodes from the file containing the candidate definitions.
    fn with_imported_final_impl(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        candidates: ImportedFinalCandidatesIterator<'_, 'db>,
        requires_explicit_reexport: RequiresExplicitReExport,
        reachability_cache: Option<&ReachabilityEvaluationCache<'db>>,
        fallback_to_imported_type: bool,
    ) -> Self {
        legacy_inline(self.with_imported_final_with(
            env,
            &LegacyInlineEffects::new(db),
            candidates,
            requires_explicit_reexport,
            reachability_cache,
            fallback_to_imported_type,
        ))
    }

    pub(crate) async fn with_imported_final_with<E: SourcePlaceEffects<'db>>(
        mut self,
        env: &ProgramEnvironment<'db>,
        effects: &E,
        candidates: ImportedFinalCandidatesIterator<'_, 'db>,
        requires_explicit_reexport: RequiresExplicitReExport,
        reachability_cache: Option<&ReachabilityEvaluationCache<'db>>,
        fallback_to_imported_type: bool,
    ) -> Result<Self, E::Error> {
        self.apply_imported_final_with(
            env,
            effects,
            candidates,
            requires_explicit_reexport,
            reachability_cache,
            fallback_to_imported_type,
        )
        .await?;
        Ok(self)
    }

    pub(crate) async fn apply_imported_final_with<E: SourcePlaceEffects<'db>>(
        &mut self,
        env: &ProgramEnvironment<'db>,
        effects: &E,
        candidates: ImportedFinalCandidatesIterator<'_, 'db>,
        requires_explicit_reexport: RequiresExplicitReExport,
        reachability_cache: Option<&ReachabilityEvaluationCache<'db>>,
        fallback_to_imported_type: bool,
    ) -> Result<(), E::Error> {
        effects.reduction_checkpoint(SourcePlaceWork::Start).await?;
        let predicates = candidates.predicates();
        let reachability_constraints = candidates.reachability_constraints();
        let fallback_to_imported_type =
            fallback_to_imported_type && self.place_and_quals.place.is_undefined();

        let mut first_imported_type = None;
        let mut imported_type_builder: Option<PublicTypeBuilder<'db>> = None;
        let mut imported_provenance = Provenance::Unknown;
        let mut imported_reachability = Truthiness::AlwaysFalse;

        let mut candidates = candidates;
        loop {
            effects
                .reduction_checkpoint(SourcePlaceWork::ImportedFinalAdvance)
                .await?;
            let Some(candidate) = candidates.next() else {
                break;
            };
            let definition = candidate.definition;

            // A genuine stub annotation can export its symbol even when an import omits the
            // redundant alias normally required for re-export. The import itself remains private.
            // TODO: Preserve `Final` when a later public assignment re-exports a private import,
            // without leaking qualifiers from a private branch.
            if self.first_declaration.is_none()
                && is_non_exported_with(effects, definition, requires_explicit_reexport).await?
            {
                continue;
            }

            let static_reachability = reachability_with(
                effects,
                reachability_cache,
                reachability_constraints,
                predicates,
                candidate.reachability_constraint,
            )
            .await?;
            if static_reachability.is_always_false() {
                continue;
            }

            let Some(declared_type) = effects.inferred_declaration(definition).await? else {
                continue;
            };
            if !declared_type.qualifiers().contains(TypeQualifiers::FINAL) {
                continue;
            }

            self.place_and_quals
                .qualifiers
                .insert(TypeQualifiers::FINAL);

            // Public lookup must still regard this symbol as undeclared. Only assignment
            // inference without an annotation needs the imported source type as a constraint.
            if !fallback_to_imported_type {
                effects
                    .reduction_checkpoint(SourcePlaceWork::Complete)
                    .await?;
                return Ok(());
            }

            imported_reachability = imported_reachability.or(static_reachability);
            let source_provenance = match declared_type.provenance() {
                Provenance::Unknown => Provenance::SingleDefinition(definition),
                provenance => provenance,
            };
            imported_provenance = imported_provenance.or(source_provenance);

            let imported_type = declared_type.inner_type();
            if let Some(builder) = &mut imported_type_builder {
                builder
                    .add(effects, imported_type, static_reachability)
                    .await?;
            } else if let Some((first, first_reachability)) = first_imported_type {
                let mut builder = PublicTypeBuilder::new(effects.union_builder(env).await?);
                builder.add(effects, first, first_reachability).await?;
                builder
                    .add(effects, imported_type, static_reachability)
                    .await?;
                imported_type_builder = Some(builder);
            } else {
                first_imported_type = Some((imported_type, static_reachability));
            }
        }

        if let Some((first, _)) = first_imported_type {
            let imported_type = if let Some(builder) = imported_type_builder {
                builder.build(effects).await?
            } else {
                first
            };
            let definedness = if imported_reachability.is_always_true() {
                Definedness::AlwaysDefined
            } else {
                Definedness::PossiblyUndefined
            };
            self.place_and_quals.place = Place::Defined(
                DefinedPlace::new(imported_type)
                    .with_definedness(definedness)
                    .with_provenance(imported_provenance),
            );
        }

        effects
            .reduction_checkpoint(SourcePlaceWork::Complete)
            .await?;
        Ok(())
    }

    pub(crate) fn ignore_conflicting_declarations(self) -> PlaceAndQualifiers<'db> {
        self.place_and_quals
    }

    pub(crate) fn into_place_and_conflicting_declarations(
        self,
    ) -> (
        PlaceAndQualifiers<'db>,
        Option<Box<indexmap::set::Slice<Type<'db>>>>,
    ) {
        (self.place_and_quals, self.conflicting_types)
    }
}

/// A type with declaredness information, and a set of type qualifiers.
///
/// This is used to represent the result of looking up the declared type. Consider this
/// example:
/// ```py
/// class C:
///     if flag:
///         variable: ClassVar[int]
/// ```
/// If we look up the declared type of `variable` in the scope of class `C`, we will get
/// the type `int`, a "declaredness" of [`Definedness::PossiblyUndefined`], and the information
/// that this comes with a [`CLASS_VAR`] type qualifier.
///
/// [`CLASS_VAR`]: crate::types::TypeQualifiers::CLASS_VAR
#[derive(
    Debug, Clone, Default, Copy, Hash, PartialEq, Eq, get_size2::GetSize, salsa::SalsaValue,
)]
pub(crate) struct PlaceAndQualifiers<'db> {
    pub(crate) place: Place<'db>,
    pub(crate) qualifiers: TypeQualifiers,
}

impl<'db> PlaceAndQualifiers<'db> {
    pub(crate) fn unbound() -> Self {
        Self::default()
    }

    pub(crate) fn is_undefined(&self) -> bool {
        self.place.is_undefined()
    }

    pub(crate) fn ignore_possibly_undefined(&self) -> Option<Type<'db>> {
        self.place.ignore_possibly_undefined()
    }

    /// Returns `true` if the place has a `ClassVar` type qualifier.
    pub(crate) fn is_class_var(&self) -> bool {
        self.qualifiers.contains(TypeQualifiers::CLASS_VAR)
    }

    /// Returns `true` if the place has a `InitVar` type qualifier.
    pub(crate) fn is_init_var(&self) -> bool {
        self.qualifiers.contains(TypeQualifiers::INIT_VAR)
    }

    /// Returns `true` if the place has a `Required` type qualifier.
    pub(crate) fn is_required(&self) -> bool {
        self.qualifiers.contains(TypeQualifiers::REQUIRED)
    }

    /// Returns `true` if the place has a `NotRequired` type qualifier.
    pub(crate) fn is_not_required(&self) -> bool {
        self.qualifiers.contains(TypeQualifiers::NOT_REQUIRED)
    }

    /// Returns `true` if the place has a `ReadOnly` type qualifier.
    pub(crate) fn is_read_only(&self) -> bool {
        self.qualifiers.contains(TypeQualifiers::READ_ONLY)
    }

    /// Returns `Some(…)` if the place is qualified with `typing.Final` without a specified type.
    pub(crate) fn is_bare_final(&self) -> Option<TypeQualifiers> {
        match self {
            PlaceAndQualifiers { place, qualifiers }
                if (qualifiers.contains(TypeQualifiers::FINAL)
                    && place
                        .ignore_possibly_undefined()
                        .is_some_and(|ty| ty.is_unknown())) =>
            {
                Some(*qualifiers)
            }
            _ => None,
        }
    }

    #[must_use]
    pub(crate) fn map_type(
        self,
        f: impl FnOnce(Type<'db>) -> Type<'db>,
    ) -> PlaceAndQualifiers<'db> {
        PlaceAndQualifiers {
            place: self.place.map_type(f),
            qualifiers: self.qualifiers,
        }
    }

    /// Transform place and qualifiers into a [`LookupResult`],
    /// a [`Result`] type in which the `Ok` variant represents a definitely defined place
    /// and the `Err` variant represents a place that is either definitely or possibly undefined.
    ///
    /// For places whose public type differs from their raw stored type, this applies the
    /// public-type policy lazily during lookup.
    pub(crate) fn into_lookup_result(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
    ) -> LookupResult<'db> {
        legacy_inline(self.into_lookup_result_with(db, env, &LegacyInlineEffects::new(db)))
    }

    pub(crate) async fn into_lookup_result_with<E: PublicLookupEffects<'db>>(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        effects: &E,
    ) -> Result<LookupResult<'db>, E::Error> {
        Ok(match self {
            PlaceAndQualifiers {
                place: Place::Defined(place),
                qualifiers,
            } => {
                let ty = place
                    .public_type_policy
                    .apply_if_needed_with(db, env, effects, place.ty)
                    .await?;
                let type_and_qualifiers = TypeAndQualifiers::new(ty, place.origin, qualifiers)
                    .with_provenance(place.provenance);
                match place.definedness {
                    Definedness::AlwaysDefined => Ok(type_and_qualifiers),
                    Definedness::PossiblyUndefined => {
                        Err(LookupError::PossiblyUndefined(type_and_qualifiers))
                    }
                }
            }
            PlaceAndQualifiers {
                place: Place::Undefined,
                qualifiers,
            } => Err(LookupError::Undefined(qualifiers)),
        })
    }

    /// Safely unwrap the place and the qualifiers into a [`TypeAndQualifiers`].
    ///
    /// If the place is definitely unbound or possibly unbound, it will be transformed into a
    /// [`LookupError`] and `diagnostic_fn` will be applied to the error value before returning
    /// the result of `diagnostic_fn` (which will be a [`TypeAndQualifiers`]). This allows the caller
    /// to ensure that a diagnostic is emitted if the place is possibly or definitely unbound.
    pub(crate) fn unwrap_with_diagnostic(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        diagnostic_fn: impl FnOnce(LookupError<'db>) -> TypeAndQualifiers<'db>,
    ) -> TypeAndQualifiers<'db> {
        self.into_lookup_result(db, env)
            .unwrap_or_else(diagnostic_fn)
    }

    /// Fallback (partially or fully) to another place if `self` is partially or fully unbound.
    ///
    /// 1. If `self` is definitely bound, return `self` without evaluating `fallback_fn()`.
    /// 2. Else, evaluate `fallback_fn()`:
    ///    1. If `self` is definitely unbound, return the result of `fallback_fn()`.
    ///    2. Else, if `fallback` is definitely unbound, return `self`.
    ///    3. Else, if `self` is possibly unbound and `fallback` is definitely bound,
    ///       return `Place(<union of self-type and fallback-type>, Definedness::AlwaysDefined)`
    ///    4. Else, if `self` is possibly unbound and `fallback` is possibly unbound,
    ///       return `Place(<union of self-type and fallback-type>, Definedness::PossiblyUndefined)`
    #[must_use]
    pub(crate) fn or_fall_back_to(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        fallback_fn: impl FnOnce() -> PlaceAndQualifiers<'db>,
    ) -> Self {
        self.into_lookup_result(db, env)
            .or_else(|lookup_error| lookup_error.or_fall_back_to(db, env, fallback_fn()))
            .into()
    }

    pub(crate) fn cycle_normalized(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        previous_place: Self,
        cycle: &salsa::Cycle,
    ) -> Self {
        match place_cycle_normalized_sync(
            self,
            env,
            previous_place,
            cycle,
            PlaceNormalizationFacts,
            &OrdinaryPlaceNormalizationEffects { db },
        ) {
            Ok(place) => place,
            Err(error) => match error {},
        }
    }
}

impl<'db> From<Place<'db>> for PlaceAndQualifiers<'db> {
    fn from(place: Place<'db>) -> Self {
        place.with_qualifiers(TypeQualifiers::empty())
    }
}

pub(crate) fn place_by_id_ingredient(
    db: &dyn Db,
) -> &salsa::plumbing::function::IngredientImpl<PlaceByIdConfiguration> {
    place_by_id::fn_ingredient_(db, db.zalsa())
}

#[salsa::tracked(configuration = (pub(crate) PlaceByIdConfiguration), attempt = ReturnOnly,
    returns(copy),
    cycle_initial=|_, id, _, _, _, _| Place::bound(Type::divergent(id)).into(),
    cycle_fn=|db, cycle, previous: &PlaceAndQualifiers<'db>, place: PlaceAndQualifiers<'db>, scope: ScopeId<'db>, _, _, _| {
        let env = ProgramEnvironment::from_scope(scope);
        place.cycle_normalized(db, &env, *previous, cycle)
    },
    heap_size=ruff_memory_usage::heap_size
)]
pub(crate) fn place_by_id<'db>(
    db: &'db dyn Db,
    scope: ScopeId<'db>,
    place_id: ScopedPlaceId,
    requires_explicit_reexport: RequiresExplicitReExport,
    considered_definitions: ConsideredDefinitions,
) -> PlaceAndQualifiers<'db> {
    legacy_inline(place_by_id_with(
        db,
        &LegacyInlineEffects::new(db),
        scope,
        place_id,
        requires_explicit_reexport,
        considered_definitions,
        use_def_map(db, scope),
    ))
}

pub(crate) async fn place_by_id_with<'db, E: SourcePlaceEffects<'db>>(
    db: &'db dyn Db,
    effects: &E,
    scope: ScopeId<'db>,
    place_id: ScopedPlaceId,
    requires_explicit_reexport: RequiresExplicitReExport,
    considered_definitions: ConsideredDefinitions,
    use_def: &UseDefMap<'db>,
) -> Result<PlaceAndQualifiers<'db>, E::Error> {
    let env = ProgramEnvironment::from_scope(scope);

    // If the place is declared, the public type is based on declarations; otherwise, it's based
    // on inference from bindings.

    let (declarations, imported_final) = match considered_definitions {
        ConsideredDefinitions::EndOfScope => (
            use_def.end_of_scope_declarations(place_id),
            use_def.end_of_scope_imported_final_candidates(place_id),
        ),
        ConsideredDefinitions::AllReachable => (
            use_def.reachable_declarations(place_id),
            use_def.reachable_imported_final_candidates(place_id),
        ),
    };

    let declared = place_from_declarations_with(
        &env,
        effects,
        declarations,
        requires_explicit_reexport,
        None,
    )
    .await?
    .with_imported_final_with(
        &env,
        effects,
        imported_final,
        requires_explicit_reexport,
        None,
        false,
    )
    .await?
    .ignore_conflicting_declarations();

    let all_considered_bindings = || match considered_definitions {
        ConsideredDefinitions::EndOfScope => use_def.end_of_scope_bindings(place_id),
        ConsideredDefinitions::AllReachable => use_def.reachable_bindings(place_id),
    };

    // If a symbol is undeclared, but qualified with `typing.Final`, we use the right-hand side
    // inferred type, without unioning with `Unknown`, because it cannot be modified.
    if let Some(qualifiers) = declared.is_bare_final() {
        let bindings = all_considered_bindings();
        return Ok(place_from_bindings_with(
            &env,
            effects,
            bindings,
            requires_explicit_reexport,
            None,
        )
        .await?
        .place
        .with_qualifiers(qualifiers));
    }

    Ok(match declared {
        // Handle bare `ClassVar` annotations by falling back to the union of `Unknown` and the
        // inferred type.
        PlaceAndQualifiers {
            place:
                Place::Defined(DefinedPlace {
                    ty: Type::Dynamic(DynamicType::Unknown),
                    origin,
                    definedness,
                    provenance: declared_provenance,
                    ..
                }),
            qualifiers,
        } if qualifiers.contains(TypeQualifiers::CLASS_VAR) => {
            let bindings = all_considered_bindings();
            match place_from_bindings_with(
                &env,
                effects,
                bindings,
                requires_explicit_reexport,
                None,
            )
            .await?
            .place
            {
                Place::Defined(DefinedPlace {
                    ty: inferred,
                    origin,
                    definedness: boundness,
                    provenance: inferred_provenance,
                    ..
                }) => Place::Defined(DefinedPlace {
                    ty: effects
                        .union_two(db, &env, Type::unknown(), inferred)
                        .await?,
                    origin,
                    definedness: boundness,
                    public_type_policy: PublicTypePolicy::Raw,
                    provenance: inferred_provenance.or(declared_provenance),
                })
                .with_qualifiers(qualifiers),
                Place::Undefined => Place::Defined(DefinedPlace {
                    ty: Type::unknown(),
                    origin,
                    definedness,
                    public_type_policy: PublicTypePolicy::Raw,
                    provenance: declared_provenance,
                })
                .with_qualifiers(qualifiers),
            }
        }
        // Place is declared, trust the declared type
        place_and_quals @ PlaceAndQualifiers {
            place:
                Place::Defined(DefinedPlace {
                    definedness: Definedness::AlwaysDefined,
                    ..
                }),
            qualifiers: _,
        } => place_and_quals,
        // Place is possibly declared
        PlaceAndQualifiers {
            place:
                Place::Defined(DefinedPlace {
                    ty: declared_ty,
                    origin,
                    definedness: Definedness::PossiblyUndefined,
                    provenance: declared_provenance,
                    ..
                }),
            qualifiers,
        } => {
            let bindings = all_considered_bindings();
            let boundness_analysis = bindings.boundness_analysis();
            let inferred =
                place_from_bindings_with(&env, effects, bindings, requires_explicit_reexport, None)
                    .await?;

            let place = match inferred.place {
                // Place is possibly undeclared and definitely unbound
                Place::Undefined => {
                    // TODO: We probably don't want to report `AlwaysDefined` here. This requires a bit of
                    // design work though as we might want a different behavior for stubs and for
                    // normal modules.
                    Place::Defined(DefinedPlace {
                        ty: declared_ty,
                        origin,
                        definedness: Definedness::AlwaysDefined,
                        public_type_policy: PublicTypePolicy::Raw,
                        provenance: declared_provenance,
                    })
                }
                // Place is possibly undeclared and (possibly) bound
                Place::Defined(DefinedPlace {
                    ty: inferred_ty,
                    origin,
                    definedness: boundness,
                    provenance: inferred_provenance,
                    ..
                }) => Place::Defined(DefinedPlace {
                    ty: effects
                        .union_two(db, &env, inferred_ty, declared_ty)
                        .await?,
                    origin,
                    definedness: if boundness_analysis == BoundnessAnalysis::AssumeBound {
                        Definedness::AlwaysDefined
                    } else {
                        boundness
                    },
                    public_type_policy: PublicTypePolicy::Raw,
                    provenance: inferred_provenance.or(declared_provenance),
                }),
            };

            PlaceAndQualifiers { place, qualifiers }
        }
        // Place is undeclared, infer the type from bindings
        PlaceAndQualifiers {
            place: Place::Undefined,
            qualifiers,
        } => {
            let bindings = all_considered_bindings();
            let boundness_analysis = bindings.boundness_analysis();
            let mut inferred =
                place_from_bindings_with(&env, effects, bindings, requires_explicit_reexport, None)
                    .await?
                    .place;

            if boundness_analysis == BoundnessAnalysis::AssumeBound
                && let Place::Defined(defined) = inferred
                && defined.definedness == Definedness::PossiblyUndefined
            {
                inferred = Place::Defined(defined.with_definedness(Definedness::AlwaysDefined));
            }

            if !effects
                .preserve_raw_public_type(db, scope, place_id)
                .await?
            {
                // Public inferred types should expose a promoted view rather than their raw
                // inferred literal form. The adjustment is applied lazily when converting to
                // `LookupResult` via `into_lookup_result`.
                inferred = inferred.with_public_type_policy(PublicTypePolicy::Promote);
            }

            inferred.with_qualifiers(qualifiers)
        }
    })

    // TODO (ticket: https://github.com/astral-sh/ruff/issues/14297) Our handling of boundness
    // currently only depends on bindings, and ignores declarations. This is inconsistent, since
    // we only look at bindings if the place may be undeclared. Consider the following example:
    // ```py
    // x: int
    //
    // if flag:
    //     y: int
    // else
    //     y = 3
    // ```
    // If we import from this module, we will currently report `x` as a definitely-bound place
    // (even though it has no bindings at all!) but report `y` as possibly-unbound (even though
    // every path has either a binding or a declaration for it.)
}

enum DeclarationsBoundnessEvaluator<'map, 'db> {
    AssumeBound,
    BasedOnUnboundVisibility {
        reachability_cache: Option<&'map ReachabilityEvaluationCache<'db>>,
        unbound_visibility: Option<DeclarationWithConstraint<'db>>,
        reachability_constraints: &'map ReachabilityConstraints,
        predicates: &'map IndexSlice<ScopedPredicateId, Predicate<'db>>,
        requires_explicit_reexport: RequiresExplicitReExport,
    },
}

impl<'db> DeclarationsBoundnessEvaluator<'_, 'db> {
    async fn evaluate<E: SourcePlaceEffects<'db>>(
        self,
        effects: &E,
        all_declarations_definitely_reachable: bool,
    ) -> Result<Definedness, E::Error> {
        Ok(match self {
            DeclarationsBoundnessEvaluator::AssumeBound => {
                if all_declarations_definitely_reachable {
                    Definedness::AlwaysDefined
                } else {
                    // For declarations, it is important to consider the possibility that they might only
                    // be bound in one control flow path, while the other path contains a binding. In order
                    // to even consider the bindings as well in `place_by_id`, we return `PossiblyUndefined`
                    // here.
                    Definedness::PossiblyUndefined
                }
            }
            DeclarationsBoundnessEvaluator::BasedOnUnboundVisibility {
                reachability_cache,
                reachability_constraints,
                unbound_visibility,
                predicates,
                requires_explicit_reexport,
            } => {
                let undeclared_reachability = match unbound_visibility {
                    Some(DeclarationWithConstraint {
                        declaration,
                        reachability_constraint,
                        ..
                    }) if is_undefined_or_non_exported_with(
                        effects,
                        declaration,
                        requires_explicit_reexport,
                    )
                    .await? =>
                    {
                        reachability_with(
                            effects,
                            reachability_cache,
                            reachability_constraints,
                            predicates,
                            reachability_constraint,
                        )
                        .await?
                    }
                    _ => Truthiness::AlwaysFalse,
                };
                match undeclared_reachability {
                    Truthiness::AlwaysTrue => {
                        unreachable!(
                            "If we have at least one declaration, the implicit `unbound` binding should not be definitely visible"
                        )
                    }
                    Truthiness::AlwaysFalse => Definedness::AlwaysDefined,
                    Truthiness::Ambiguous => Definedness::PossiblyUndefined,
                }
            }
        })
    }
}

/// Implementation of [`symbol`].
fn symbol_impl<'db>(
    db: &'db dyn Db,
    scope: ScopeId<'db>,
    name: &str,
    requires_explicit_reexport: RequiresExplicitReExport,
    considered_definitions: ConsideredDefinitions,
) -> PlaceAndQualifiers<'db> {
    let _span = tracing::trace_span!("symbol", ?name).entered();
    legacy_inline(symbol_with(
        db,
        &LegacyInlineEffects::new(db),
        scope,
        name,
        requires_explicit_reexport,
        considered_definitions,
    ))
}

pub(crate) async fn symbol_with<'db, E: SourcePlaceEffects<'db>>(
    db: &'db dyn Db,
    effects: &E,
    scope: ScopeId<'db>,
    name: &str,
    requires_explicit_reexport: RequiresExplicitReExport,
    considered_definitions: ConsideredDefinitions,
) -> Result<PlaceAndQualifiers<'db>, E::Error> {
    // Check the symbol name first to avoid a module-resolution query for every symbol lookup.
    if matches!(name, "version_info" | "platform")
        && effects.is_known_module(db, scope, KnownModule::Sys).await?
    {
        match name {
            "version_info" => {
                return Ok(Place::bound(Type::sys_version_info()).into());
            }
            "platform" => match scope.program(db).python_platform(db) {
                crate::PythonPlatform::Identifier(platform) => {
                    return Ok(Place::bound(Type::string_literal(db, platform.as_str())).into());
                }
                crate::PythonPlatform::All => {
                    // Fall through to the looked up type
                }
            },
            _ => {}
        }
    }

    if name == "name" && effects.is_known_module(db, scope, KnownModule::Os).await? {
        match scope.program(db).python_platform(db) {
            crate::PythonPlatform::Identifier(platform) => {
                // In CPython, `os.name` is `"nt"` on Windows and `"posix"` otherwise.
                let os_name = if platform == "win32" { "nt" } else { "posix" };
                return Ok(Place::bound(Type::string_literal(db, os_name)).into());
            }
            crate::PythonPlatform::All => {
                // Fall through to the looked up type
            }
        }
    }

    let Some(symbol) = effects.symbol_id(db, scope, name).await? else {
        return Ok(PlaceAndQualifiers::default());
    };
    effects
        .place_by_id(
            db,
            scope,
            symbol.into(),
            requires_explicit_reexport,
            considered_definitions,
        )
        .await
}

/// Pre-computed reachability analysis for loop-back bindings in a loop header.
#[salsa::tracked(
    attempt = ReturnOnly,
    returns(clone),
    cycle_initial=|db, _, definition: Definition<'db>| {
        loop_header_reachability_impl(db, definition, Some(&mut FxHashMap::default()))
    },
    cycle_fn=loop_header_reachability_cycle_recover,
    heap_size = ruff_memory_usage::heap_size,
)]
pub(crate) fn loop_header_reachability<'db>(
    db: &'db dyn Db,
    definition: Definition<'db>,
) -> LoopHeaderReachability<'db> {
    loop_header_reachability_impl(db, definition, None)
}

fn loop_header_reachability_cycle_recover<'db>(
    _db: &'db dyn Db,
    cycle: &salsa::Cycle,
    previous: &LoopHeaderReachability<'db>,
    result: LoopHeaderReachability<'db>,
    _definition: Definition<'db>,
) -> LoopHeaderReachability<'db> {
    result.cycle_normalized(previous, cycle)
}

fn loop_header_reachability_impl<'db>(
    db: &'db dyn Db,
    definition: Definition<'db>,
    mut cycle_initial_cache: Option<&mut FxHashMap<Definition<'db>, Truthiness>>,
) -> LoopHeaderReachability<'db> {
    // This cutoff was chosen by benchmarking real isort to keep loop analysis
    // overhead minimal while preserving diagnostics.
    const MAX_EXACT_LOOP_HEADER_REACHABILITY_NODES: usize = 2048;

    let DefinitionKind::LoopHeader(loop_header_definition) = definition.kind(db) else {
        unreachable!("`loop_header_reachability` called with non-loop-header definition");
    };

    let scope = definition.scope(db);
    let use_def = use_def_map(db, scope);
    let loop_header = use_def.loop_header(loop_header_definition.loop_header_id());
    let place = loop_header_definition.place();

    let mut deleted_reachability = Truthiness::AlwaysFalse;
    let mut deleted_narrowing_constraints = FxIndexSet::default();
    let mut reachable_bindings = FxIndexSet::default();
    let live_bindings: Vec<_> = loop_header.bindings_for_place(place).collect();
    let use_exact_reachability = use_def.reachability_constraints().used_interiors().len()
        <= MAX_EXACT_LOOP_HEADER_REACHABILITY_NODES;
    for live_binding in live_bindings {
        let reachability = if cycle_initial_cache.is_some() {
            Truthiness::Ambiguous
        } else if use_exact_reachability {
            evaluate_reachability(db, use_def, live_binding.reachability_constraint())
        } else if live_binding.reachability_constraint()
            == ScopedReachabilityConstraintId::ALWAYS_FALSE
        {
            Truthiness::AlwaysFalse
        } else {
            Truthiness::Ambiguous
        };
        // Skip unreachable bindings.
        if reachability.is_always_false() {
            continue;
        }

        match use_def.definition(live_binding.binding()) {
            // Assignment validity can depend on this header, so avoid inferring it while
            // initializing a cycle.
            DefinitionState::Defined(def)
                if cycle_initial_cache.is_some() || !is_discarded_dict_key_assignment(db, def) =>
            {
                debug_assert_ne!(
                    def, definition,
                    "loop headers only include bindings from within the loop"
                );
                if def.kind(db).is_loop_header() {
                    // An inner loop can reach a `break` with a header binding that carries a
                    // deletion from an earlier iteration. That deletion also affects boundness
                    // in the enclosing loop.
                    let nested_deleted_reachability =
                        if let Some(cache) = cycle_initial_cache.as_deref_mut() {
                            // Cycle initialization cannot evaluate predicates that could re-enter
                            // the cycle. Memoize this structural walk because a descendant header
                            // can be reached through several containing headers.
                            cache.get(&def).copied().unwrap_or_else(|| {
                                let deleted_reachability =
                                    loop_header_reachability_impl(db, def, Some(cache))
                                        .deleted_reachability;
                                cache.insert(def, deleted_reachability);
                                deleted_reachability
                            })
                        } else {
                            loop_header_reachability(db, def).deleted_reachability
                        };
                    // This binding is reachable, but a conditional loop-back path can make a
                    // definitely reachable nested deletion only possibly reachable here.
                    deleted_reachability =
                        deleted_reachability.or(match nested_deleted_reachability {
                            Truthiness::AlwaysTrue => reachability,
                            other => other,
                        });
                }
                reachable_bindings.insert(ReachableLoopBinding {
                    definition: def,
                    narrowing_constraint: live_binding.narrowing_constraint(),
                });
            }
            // `del` in the loop body is always visible to code after the loop via the
            // normal control flow merge. Updating `deleted_reachability` here is
            // necessary for prior uses in the loop to see it.
            // Discarded dictionary-key bindings also require a fallback to the receiver's
            // value type instead of contributing their assigned value.
            DefinitionState::Defined(_) | DefinitionState::Deleted => {
                deleted_reachability = deleted_reachability.or(reachability);
                deleted_narrowing_constraints.insert(live_binding.narrowing_constraint());
            }
            DefinitionState::Undefined => {
                unreachable!("loop headers only include bindings from within the loop")
            }
        }
    }

    LoopHeaderReachability {
        deleted_reachability,
        deleted_narrowing_constraints: deleted_narrowing_constraints.into_iter().collect(),
        reachable_bindings,
    }
}

/// Result of [`loop_header_reachability`]: pre-computed reachability info for loop-back bindings.
#[derive(Debug, Clone, PartialEq, Eq, get_size2::GetSize, salsa::SalsaValue)]
pub(crate) struct LoopHeaderReachability<'db> {
    /// Reachability of deletions, including those carried by nested loop headers.
    pub(crate) deleted_reachability: Truthiness,
    /// Constraints established after a deletion, member invalidation, or discarded key assignment.
    /// These still narrow the fallback type of the member on the next iteration.
    pub(crate) deleted_narrowing_constraints: Box<[ScopedNarrowingConstraint]>,
    /// Reachable loop-back bindings whose values contribute to inferred types.
    pub(crate) reachable_bindings: FxIndexSet<ReachableLoopBinding<'db>>,
}

impl<'db> LoopHeaderReachability<'db> {
    fn cycle_normalized(
        self,
        previous: &LoopHeaderReachability<'db>,
        cycle: &salsa::Cycle,
    ) -> LoopHeaderReachability<'db> {
        // Avoid losing precision for cycles that are soon to converge.
        // See [`Type::cycle_normalized`] for more details.
        if cycle.iteration() <= crate::TAINTED_CYCLES {
            return self;
        }

        let mut reachable_bindings: FxIndexSet<_> = previous
            .reachable_bindings
            .iter()
            .copied()
            .chain(self.reachable_bindings)
            .collect();
        reachable_bindings.shrink_to_fit();
        let deleted_narrowing_constraints: FxIndexSet<_> = previous
            .deleted_narrowing_constraints
            .iter()
            .copied()
            .chain(self.deleted_narrowing_constraints)
            .collect();

        LoopHeaderReachability {
            deleted_reachability: self.deleted_reachability,
            deleted_narrowing_constraints: deleted_narrowing_constraints.into_iter().collect(),
            reachable_bindings,
        }
    }
}

/// A single reachable loop-back binding with its narrowing constraint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, get_size2::GetSize, salsa::SalsaValue)]
pub(crate) struct ReachableLoopBinding<'db> {
    pub(crate) definition: Definition<'db>,
    pub(crate) narrowing_constraint: ScopedNarrowingConstraint,
}

/// Implementation of [`place_from_bindings`].
///
/// ## Implementation Note
/// This function gets called cross-module. It, therefore, shouldn't
/// access any AST nodes from the file containing the declarations.
fn place_from_bindings_impl<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    bindings_with_constraints: BindingWithConstraintsIterator<'_, 'db>,
    requires_explicit_reexport: RequiresExplicitReExport,
    reachability_cache: Option<&ReachabilityEvaluationCache<'db>>,
) -> PlaceWithDefinition<'db> {
    legacy_inline(place_from_bindings_with(
        env,
        &LegacyInlineEffects::new(db),
        bindings_with_constraints,
        requires_explicit_reexport,
        reachability_cache,
    ))
}

pub(crate) async fn place_from_bindings_with<'db, E: SourcePlaceEffects<'db>>(
    env: &ProgramEnvironment<'db>,
    effects: &E,
    bindings_with_constraints: BindingWithConstraintsIterator<'_, 'db>,
    requires_explicit_reexport: RequiresExplicitReExport,
    reachability_cache: Option<&ReachabilityEvaluationCache<'db>>,
) -> Result<PlaceWithDefinition<'db>, E::Error> {
    effects.reduction_checkpoint(SourcePlaceWork::Start).await?;
    let predicates = bindings_with_constraints.predicates();
    let reachability_constraints = bindings_with_constraints.reachability_constraints();
    let boundness_analysis = bindings_with_constraints.boundness_analysis();
    let mut bindings_with_constraints = bindings_with_constraints.peekable();

    let unbound_reachability_constraint = match bindings_with_constraints.peek() {
        Some(BindingWithConstraints {
            binding,
            reachability_constraint,
            ..
        }) if is_undefined_or_non_exported_with(effects, *binding, requires_explicit_reexport)
            .await? =>
        {
            Some(*reachability_constraint)
        }
        _ => None,
    };
    let mut deleted_reachability = Truthiness::AlwaysFalse;

    // Evaluate this lazily because we don't always need it (for example, if there are no visible
    // bindings at all, we don't need it), and it can cause us to evaluate reachability constraint
    // expressions, which is extra work and can lead to cycles.
    let unbound_visibility = async || {
        if let Some(constraint) = unbound_reachability_constraint {
            Ok::<_, E::Error>(Some(
                reachability_with(
                    effects,
                    reachability_cache,
                    reachability_constraints,
                    predicates,
                    constraint,
                )
                .await?,
            ))
        } else {
            Ok(None)
        }
    };

    let mut first_definition = None;
    let mut provenance = Provenance::Unknown;
    // special handling for synthetic loop header definitions and nested bindings definitions
    let mut only_non_shadowing_bindings = true;
    let mut narrowing_projector = None;

    let mut first_type = None;
    let mut type_builder: Option<PublicTypeBuilder<'db>> = None;
    loop {
        effects
            .reduction_checkpoint(SourcePlaceWork::BindingAdvance)
            .await?;
        let Some(BindingWithConstraints {
            binding,
            narrowing_constraint,
            reachability_constraint,
            ..
        }) = bindings_with_constraints.next()
        else {
            break;
        };
        let reduced = async {
            let binding = match binding {
                DefinitionState::Defined(binding)
                    if matches!(
                        effects.definition_kind(binding).await?,
                        DefinitionKind::DictKeyAssignment(_)
                    ) && effects.is_discarded_dict_key_assignment(binding).await? =>
                {
                    // This synthesized `d[key] = value` binding was derived from an assignment such
                    // as `d = {key: value}`. If the RHS is not known to be stored unchanged, discard
                    // the binding so that lookup of `d[key]` can fall back to `d`.
                    return Ok(None);
                }
                DefinitionState::Defined(binding) => binding,
                DefinitionState::Undefined => {
                    return Ok(None);
                }
                DefinitionState::Deleted => {
                    if !deleted_reachability.is_always_true() {
                        deleted_reachability = deleted_reachability.or(reachability_with(
                            effects,
                            reachability_cache,
                            reachability_constraints,
                            predicates,
                            reachability_constraint,
                        )
                        .await?);
                    }
                    return Ok(None);
                }
            };

            if is_non_exported_with(effects, binding, requires_explicit_reexport).await? {
                return Ok(None);
            }

            let static_reachability = reachability_with(
                effects,
                reachability_cache,
                reachability_constraints,
                predicates,
                reachability_constraint,
            )
            .await?;

            if static_reachability.is_always_false() {
                // If the static reachability evaluates to false, the binding is either not reachable
                // from the start of the scope, or there is no control flow path from that binding to
                // the use of the place that we are investigating. There are three interesting cases
                // to consider:
                //
                // ```py
                // def f1():
                //     if False:
                //         x = 1
                //     use(x)
                //
                // def f2():
                //     y = 1
                //     return
                //     use(y)
                //
                // def f3(flag: bool):
                //     if flag:
                //         z = 1
                //     else:
                //         z = 2
                //         return
                //     use(z)
                // ```
                //
                // In the first case, there is a single binding for `x`, but it is not reachable from
                // the start of the scope. However, the use of `x` is reachable (`unbound_reachability`
                // is not always-false). This means that `x` is unbound and we should return `None`.
                //
                // In the second case, the binding of `y` is reachable, but there is no control flow
                // path from the beginning of the scope, through that binding, to the use of `y` that
                // we are investigating. There is also no control flow path from the start of the
                // scope, through the implicit `y = <unbound>` binding, to the use of `y`. This means
                // that `unbound_reachability` is always false. Since there are no other bindings, no
                // control flow path can reach this use of `y`, implying that we are in unreachable
                // section of code. We return `Never` in order to silence the `unresolve-reference`
                // diagnostic that would otherwise be emitted at the use of `y`.
                //
                // In the third case, we have two bindings for `z`. The first one is visible (there
                // is a path of control flow from the start of the scope, through that binding, to
                // the use of `z`). So we consider the case that we now encounter the second binding
                // `z = 2`, which is not visible due to the early return. The `z = <unbound>` binding
                // is not live (shadowed by the other bindings), so `unbound_reachability` is `None`.
                // Here, we are *not* in an unreachable section of code. However, it is still okay to
                // return `Never` in this case, because we will union the types of all bindings, and
                // `Never` will be eliminated automatically.

                if unbound_visibility()
                    .await?
                    .is_none_or(Truthiness::is_always_false)
                {
                    return Ok(Some((Type::Never, static_reachability)));
                }
                return Ok(None);
            }

            // We need to "look through" loop header definitions to do boundness analysis. The
            // actual type is computed by `infer_loop_header_definition` via `binding_type` below,
            // like all other bindings, so that it can participate in fixpoint iteration.
            let binding_kind = effects.definition_kind(binding).await?;
            if binding_kind.is_loop_header() {
                let loop_header = effects.loop_header_reachability(binding).await?;
                deleted_reachability = deleted_reachability.or(loop_header.deleted_reachability);
                // If all the bindings in the loop are in statically false branches, it might be
                // that none of them loop-back. In that case short-circuit, so that we don't
                // produce an `Unknown` fallback type, and so that `Place::Undefined` is still a
                // possibility below.
                if loop_header.reachable_bindings.is_empty() {
                    return Ok(None);
                }
            } else if matches!(binding_kind, DefinitionKind::NestedBindings(_)) {
                // Nested bindings definitions similar to loop header definitions, synthetic
                // bindings with special shadowing behavior. They can also coexist with `UNBOUND`.
            } else {
                only_non_shadowing_bindings = false;
            }

            first_definition.get_or_insert(binding);
            provenance = provenance.or(Provenance::SingleDefinition(binding));
            let binding_ty = effects.binding_type(binding).await?;
            let narrowed = match narrowing_constraint.constraint() {
                ScopedNarrowingConstraint::ALWAYS_TRUE => binding_ty,
                ScopedNarrowingConstraint::ALWAYS_FALSE => Type::Never,
                constraint => {
                    let projector = match &mut narrowing_projector {
                        Some(projector) => projector,
                        slot @ None => slot.insert(
                            effects
                                .narrowing_projector(
                                    env,
                                    narrowing_constraint.narrowing_constraints(),
                                    predicates,
                                    narrowing_constraint.predicate_narrowing_targets(),
                                    binding,
                                    binding_ty,
                                )
                                .await?,
                        ),
                    };
                    effects.narrow(projector, constraint, binding_ty).await?
                }
            };
            Ok::<_, E::Error>(Some((narrowed, static_reachability)))
        }
        .await?;
        if let Some((ty, reachability)) = reduced {
            if let Some(builder) = &mut type_builder {
                builder.add(effects, ty, reachability).await?;
            } else if let Some((first, first_reachability)) = first_type {
                let mut builder = PublicTypeBuilder::new(effects.union_builder(env).await?);
                builder.add(effects, first, first_reachability).await?;
                builder.add(effects, ty, reachability).await?;
                type_builder = Some(builder);
            } else {
                first_type = Some((ty, reachability));
            }
        }
    }

    let place = if let Some((first, _)) = first_type {
        let ty = if let Some(builder) = type_builder {
            builder.build(effects).await?
        } else {
            first
        };

        let boundness = match boundness_analysis {
            BoundnessAnalysis::AssumeBound => Definedness::AlwaysDefined,
            BoundnessAnalysis::BasedOnUnboundVisibility => match unbound_visibility().await? {
                Some(Truthiness::AlwaysTrue) if only_non_shadowing_bindings => {
                    // Loop header and nested binding definitions don't shadow prior bindings, so
                    // UNBOUND can still be definitely-visible alongside them. See "Use with loop
                    // header and also `UNBOUND` definitely visible" in `while_loop.md`.
                    Definedness::PossiblyUndefined
                }
                Some(Truthiness::AlwaysTrue) => {
                    unreachable!(
                        "If we have at least one binding, the implicit `unbound` binding should not be definitely visible"
                    )
                }
                Some(Truthiness::AlwaysFalse) | None => Definedness::AlwaysDefined,
                Some(Truthiness::Ambiguous) => Definedness::PossiblyUndefined,
            },
        };

        match deleted_reachability {
            Truthiness::AlwaysFalse => Place::Defined(
                DefinedPlace::new(ty)
                    .with_definedness(boundness)
                    .with_provenance(provenance),
            ),
            Truthiness::AlwaysTrue => Place::Undefined,
            Truthiness::Ambiguous => Place::Defined(
                DefinedPlace::new(ty)
                    .with_definedness(Definedness::PossiblyUndefined)
                    .with_provenance(provenance),
            ),
        }
    } else {
        Place::Undefined
    };

    effects
        .reduction_checkpoint(SourcePlaceWork::Complete)
        .await?;
    Ok(PlaceWithDefinition {
        place,
        first_definition,
    })
}

pub(super) struct PlaceWithDefinition<'db> {
    pub(super) place: Place<'db>,
    pub(super) first_definition: Option<Definition<'db>>,
}

/// Accumulates types from multiple bindings or declarations, and eventually builds a
/// union type from them.
///
/// `@overload`ed function literal types are discarded if they are definitely followed
/// by their implementation. This is to ensure that we do not merge all of them into the
/// union type. The last one will include the other overloads already.
struct PublicTypeBuilder<'db> {
    queue: Option<Type<'db>>,
    builder: UnionBuilder<'db>,
}

impl<'db> PublicTypeBuilder<'db> {
    fn new(builder: UnionBuilder<'db>) -> Self {
        PublicTypeBuilder {
            queue: None,
            builder,
        }
    }

    async fn add_to_union<E: SourcePlaceEffects<'db>>(
        &mut self,
        effects: &E,
        element: Type<'db>,
    ) -> Result<(), E::Error> {
        effects.union_add(&mut self.builder, element).await
    }

    async fn drain_queue<E: SourcePlaceEffects<'db>>(
        &mut self,
        effects: &E,
    ) -> Result<(), E::Error> {
        if let Some(queued_element) = self.queue.take() {
            self.add_to_union(effects, queued_element).await?;
        }
        Ok(())
    }

    async fn add<E: SourcePlaceEffects<'db>>(
        &mut self,
        effects: &E,
        element: Type<'db>,
        reachability: Truthiness,
    ) -> Result<bool, E::Error> {
        Ok(match element {
            Type::FunctionLiteral(function) => {
                if effects.function_is_overload(function).await? {
                    // Distinct overloaded function values can be assigned to the same public
                    // symbol in separate branches. Preserve the queued value unless the next
                    // overload belongs to the same place.
                    let same_place =
                        if let Some(Type::FunctionLiteral(queued_function)) = self.queue {
                            effects
                                .function_same_place(function, queued_function)
                                .await?
                        } else {
                            false
                        };
                    if !same_place {
                        self.drain_queue(effects).await?;
                    }

                    self.queue = Some(element);
                    false
                } else {
                    // An unconditional implementation shadows preceding overload definitions. A
                    // conditional definition, however, can be only one public possibility among
                    // several, so keep any unrelated queued overloaded function in the union.
                    if reachability.is_always_true()
                        || (if let Some(Type::FunctionLiteral(queued_function)) = self.queue {
                            effects.function_contains(function, queued_function).await?
                        } else {
                            false
                        })
                    {
                        self.queue = None;
                    } else {
                        self.drain_queue(effects).await?;
                    }
                    self.add_to_union(effects, element).await?;
                    true
                }
            }
            _ => {
                self.drain_queue(effects).await?;
                self.add_to_union(effects, element).await?;
                true
            }
        })
    }

    async fn build<E: SourcePlaceEffects<'db>>(
        mut self,
        effects: &E,
    ) -> Result<Type<'db>, E::Error> {
        self.drain_queue(effects).await?;
        effects.union_build(self.builder).await
    }
}

/// Accumulates multiple (potentially conflicting) declared types and type qualifiers,
/// and eventually builds a union from them.
struct DeclaredTypeBuilder<'db> {
    inner: PublicTypeBuilder<'db>,
    qualifiers: TypeQualifiers,
    first_type: Option<Type<'db>>,
    conflicting_types: FxOrderSet<Type<'db>>,
}

impl<'db> DeclaredTypeBuilder<'db> {
    fn new(builder: UnionBuilder<'db>) -> Self {
        DeclaredTypeBuilder {
            inner: PublicTypeBuilder::new(builder),
            qualifiers: TypeQualifiers::empty(),
            first_type: None,
            conflicting_types: FxOrderSet::default(),
        }
    }

    async fn add<E: SourcePlaceEffects<'db>>(
        &mut self,
        env: &ProgramEnvironment<'db>,
        effects: &E,
        element: TypeAndQualifiers<'db>,
        reachability: Truthiness,
    ) -> Result<(), E::Error> {
        let element_ty = element.inner_type();

        if self.inner.add(effects, element_ty, reachability).await? {
            if let Some(first_ty) = self.first_type {
                if !effects.equivalent(env, first_ty, element_ty).await? {
                    self.conflicting_types.insert(element_ty);
                }
            } else {
                self.first_type = Some(element_ty);
            }
        }

        self.qualifiers = self.qualifiers.union(element.qualifiers());
        Ok(())
    }

    async fn build<E: SourcePlaceEffects<'db>>(
        mut self,
        effects: &E,
    ) -> Result<DeclaredTypeAndConflictingTypes<'db>, E::Error> {
        let type_and_quals = TypeAndQualifiers::new(
            self.inner.build(effects).await?,
            TypeOrigin::Declared,
            self.qualifiers,
        );
        Ok(if self.conflicting_types.is_empty() {
            (type_and_quals, None)
        } else {
            self.conflicting_types.insert_before(
                0,
                self.first_type
                    .expect("there must be a first type if there are conflicting types"),
            );
            (
                type_and_quals,
                Some(self.conflicting_types.into_boxed_slice()),
            )
        })
    }
}

/// Implementation of [`place_from_declarations`].
///
/// ## Implementation Note
/// This function gets called cross-module. It, therefore, shouldn't
/// access any AST nodes from the file containing the declarations.
fn place_from_declarations_impl<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    declarations_iterator: DeclarationsIterator<'_, 'db>,
    requires_explicit_reexport: RequiresExplicitReExport,
    reachability_cache: Option<&ReachabilityEvaluationCache<'db>>,
) -> PlaceFromDeclarationsResult<'db> {
    legacy_inline(place_from_declarations_with(
        env,
        &LegacyInlineEffects::new(db),
        declarations_iterator,
        requires_explicit_reexport,
        reachability_cache,
    ))
}

pub(crate) async fn place_from_declarations_with<'db, E: SourcePlaceEffects<'db>>(
    env: &ProgramEnvironment<'db>,
    effects: &E,
    declarations_iterator: DeclarationsIterator<'_, 'db>,
    requires_explicit_reexport: RequiresExplicitReExport,
    reachability_cache: Option<&ReachabilityEvaluationCache<'db>>,
) -> Result<PlaceFromDeclarationsResult<'db>, E::Error> {
    effects.reduction_checkpoint(SourcePlaceWork::Start).await?;
    let predicates = declarations_iterator.predicates();
    let reachability_constraints = declarations_iterator.reachability_constraints();
    let boundness_analysis = declarations_iterator.boundness_analysis();

    let mut declarations = declarations_iterator.peekable();
    let boundness_evaluator = match boundness_analysis {
        BoundnessAnalysis::AssumeBound => DeclarationsBoundnessEvaluator::AssumeBound,
        BoundnessAnalysis::BasedOnUnboundVisibility => {
            DeclarationsBoundnessEvaluator::BasedOnUnboundVisibility {
                reachability_cache,
                unbound_visibility: declarations.peek().cloned(),
                predicates,
                reachability_constraints,
                requires_explicit_reexport,
            }
        }
    };

    let mut first_declaration = None;
    let mut provenance = Provenance::Unknown;
    let mut all_declarations_definitely_reachable = true;

    let mut first_type = None;
    let mut type_builder: Option<DeclaredTypeBuilder<'db>> = None;
    loop {
        effects
            .reduction_checkpoint(SourcePlaceWork::DeclarationAdvance)
            .await?;
        let Some(declaration_with_constraint) = declarations.next() else {
            break;
        };
        let DeclarationWithConstraint {
            declaration,
            reachability_constraint,
            ..
        } = declaration_with_constraint;

        let DefinitionState::Defined(declaration) = declaration else {
            continue;
        };

        if is_non_exported_with(effects, declaration, requires_explicit_reexport).await? {
            continue;
        }

        let static_reachability = reachability_with(
            effects,
            reachability_cache,
            reachability_constraints,
            predicates,
            reachability_constraint,
        )
        .await?;

        if static_reachability.is_always_false() {
            continue;
        }
        let Some(declared_type) = effects.inferred_declaration(declaration).await? else {
            continue;
        };
        first_declaration.get_or_insert(declaration);
        provenance = provenance.or(Provenance::SingleDefinition(declaration));
        all_declarations_definitely_reachable =
            all_declarations_definitely_reachable && static_reachability.is_always_true();

        if let Some(builder) = &mut type_builder {
            builder
                .add(env, effects, declared_type, static_reachability)
                .await?;
        } else if let Some((first, first_reachability)) = first_type {
            let mut builder = DeclaredTypeBuilder::new(effects.union_builder(env).await?);
            builder.add(env, effects, first, first_reachability).await?;
            builder
                .add(env, effects, declared_type, static_reachability)
                .await?;
            type_builder = Some(builder);
        } else {
            first_type = Some((declared_type, static_reachability));
        }
    }

    let result = if let Some((first, _)) = first_type {
        let (declared, conflicting) = if let Some(builder) = type_builder {
            builder.build(effects).await?
        } else {
            (first, None)
        };

        let boundness = boundness_evaluator
            .evaluate(effects, all_declarations_definitely_reachable)
            .await?;

        let place_and_quals = Place::Defined(
            DefinedPlace::new(declared.inner_type())
                .with_origin(TypeOrigin::Declared)
                .with_definedness(boundness)
                .with_provenance(provenance),
        )
        .with_qualifiers(declared.qualifiers());

        if let Some(conflicting) = conflicting {
            PlaceFromDeclarationsResult::conflict(place_and_quals, conflicting, first_declaration)
        } else {
            PlaceFromDeclarationsResult {
                place_and_quals,
                conflicting_types: None,
                first_declaration,
            }
        }
    } else {
        PlaceFromDeclarationsResult::default()
    };
    effects
        .reduction_checkpoint(SourcePlaceWork::Complete)
        .await?;
    Ok(result)
}

pub(crate) mod implicit_globals {
    use ruff_python_ast as ast;
    use ruff_python_ast::name::Name;
    use ty_module_resolver::KnownModule;

    use crate::db::Db;
    use crate::place::PlaceAndQualifiers;
    use crate::place::implicit_effects::{ImplicitGlobalEffects, ImplicitGlobalWork};
    use crate::place::implicit_symbol::{
        OrdinaryModuleGlobalSymbolEffects, module_type_implicit_global_symbol_sync,
    };
    use crate::place::source_effects::{LegacyInlineEffects, reachability_with};
    use crate::types::Type;
    use crate::types::legacy_inline;
    use crate::{Program, ProgramEnvironment};
    use ty_python_core::definition::{DefinitionKind, DefinitionState};
    use ty_python_core::scope::{NodeWithScopeRef, ScopeId};
    use ty_python_core::symbol::Symbol;
    use ty_python_core::{ProgramFile, place_table};

    use super::{Place, RequiresExplicitReExport, place_from_declarations_with};

    /// Returns the body scope when all reachable, exported definitions of `name`
    /// in a vendored module are the same direct class definition.
    ///
    /// This can be used as a fast-path to avoid query cycles.
    async fn try_vendored_class_scope_with<'db, E: ImplicitGlobalEffects<'db>>(
        db: &'db dyn Db,
        effects: &E,
        module_scope: ScopeId<'db>,
        name: &str,
    ) -> Result<Option<ScopeId<'db>>, E::Error> {
        effects
            .checkpoint(ImplicitGlobalWork::InspectModule)
            .await?;
        let program_file = effects
            .field(module_scope.read_fields(db).program_file())
            .await?;
        let python_file = effects
            .field(program_file.read_fields(db).python_file())
            .await?;
        let file = effects.field(python_file.read_fields(db).file()).await?;
        if !effects
            .field(file.read_fields(db).path())
            .await?
            .is_vendored_path()
        {
            return Ok(None);
        }
        let table = effects.place_table(db, module_scope).await?;
        effects
            .checkpoint(ImplicitGlobalWork::SymbolLookup {
                symbols: table.symbols().len(),
                name_bytes: name.len(),
            })
            .await?;
        let Some(symbol_id) = table.symbol_id(name) else {
            return Ok(None);
        };
        let use_def = effects.use_def_map(db, module_scope).await?;
        let module = effects.parsed_module(db, program_file).await?;
        let index = effects.semantic_index(db, program_file).await?;
        let mut body_scope = None;
        let bindings = use_def.end_of_scope_symbol_bindings(symbol_id);
        effects
            .checkpoint(ImplicitGlobalWork::ClassBindings {
                entries: bindings.traversal_len(),
            })
            .await?;
        for binding in bindings {
            let DefinitionState::Defined(definition) = binding.binding else {
                continue;
            };
            if effects.is_stub(db, file).await? && !effects.is_reexported(definition).await? {
                continue;
            }
            if reachability_with(
                effects,
                None,
                use_def.reachability_constraints(),
                use_def.predicates(),
                binding.reachability_constraint,
            )
            .await?
            .is_always_false()
            {
                continue;
            }

            let DefinitionKind::Class(class) =
                effects.field(definition.read_fields(db).kind()).await?
            else {
                return Ok(None);
            };
            let file_scope = index.node_scope(NodeWithScopeRef::Class(class.node(&module)));
            let class_scope = index.scope_id(file_scope);
            if body_scope.is_some_and(|body_scope| body_scope != class_scope) {
                return Ok(None);
            }
            body_scope = Some(class_scope);
        }

        Ok(body_scope)
    }

    /// Return the body scope of the canonical `types.ModuleType` class.
    pub(super) fn module_type_body_scope<'db>(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
    ) -> Option<ScopeId<'db>> {
        module_type_body_scope_inner(db, env.program(db))
    }

    #[salsa::tracked(configuration = (pub(crate) ModuleTypeBodyScopeInnerConfiguration), attempt = ReturnOnly, returns(copy), heap_size=ruff_memory_usage::heap_size)]
    fn module_type_body_scope_inner<'db>(
        db: &'db dyn Db,
        program: Program<'db>,
    ) -> Option<ScopeId<'db>> {
        let env = ProgramEnvironment::from_program(program);
        legacy_inline(module_type_body_scope_with(
            db,
            &env,
            &LegacyInlineEffects::new(db),
        ))
    }

    #[cfg(any(test, feature = "experimental-analysis"))]
    pub(crate) fn module_type_body_scope_ingredient(
        db: &dyn Db,
    ) -> &salsa::plumbing::function::IngredientImpl<ModuleTypeBodyScopeInnerConfiguration> {
        module_type_body_scope_inner::fn_ingredient_(db, db.zalsa())
    }

    pub(crate) async fn module_type_body_scope_with<'db, E: ImplicitGlobalEffects<'db>>(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        effects: &E,
    ) -> Result<Option<ScopeId<'db>>, E::Error> {
        let Some(file) = effects
            .resolve_known_module(db, env, KnownModule::Types)
            .await?
        else {
            return Ok(None);
        };
        let module_scope = effects.global_scope(db, file).await?;
        if let Some(scope) =
            try_vendored_class_scope_with(db, effects, module_scope, "ModuleType").await?
        {
            return Ok(Some(scope));
        }
        effects.fallback_module_type_body_scope(db, env).await
    }

    pub(crate) fn module_type_implicit_global_declaration<'db>(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        name: &str,
    ) -> PlaceAndQualifiers<'db> {
        // Retain the cycle recovery of the legacy symbol-list query. Suspended inference
        // establishes absence through the shared lookup after its dependencies complete.
        if !module_type_symbols(db, env)
            .iter()
            .any(|module_type_member| module_type_member == name)
        {
            return Place::Undefined.into();
        }
        legacy_inline(module_type_implicit_global_declaration_with(
            db,
            env,
            &LegacyInlineEffects::new(db),
            name,
        ))
    }

    pub(crate) async fn module_type_implicit_global_declaration_with<
        'db,
        E: ImplicitGlobalEffects<'db>,
    >(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        effects: &E,
        name: &str,
    ) -> Result<PlaceAndQualifiers<'db>, E::Error> {
        let Some(module_type_scope) = effects.module_type_body_scope(db, env).await? else {
            return Ok(Place::Undefined.into());
        };
        let table = effects.place_table(db, module_type_scope).await?;
        effects
            .checkpoint(ImplicitGlobalWork::SymbolLookup {
                symbols: table.symbols().len(),
                name_bytes: name.len(),
            })
            .await?;
        let Some(symbol_id) = table.symbol_id(name) else {
            return Ok(Place::Undefined.into());
        };
        if !is_implicit_module_global(table.symbol(symbol_id)) {
            return Ok(Place::Undefined.into());
        }
        let declarations = effects
            .use_def_map(db, module_type_scope)
            .await?
            .end_of_scope_symbol_declarations(symbol_id);
        effects
            .checkpoint(ImplicitGlobalWork::Declarations {
                entries: declarations.traversal_len(),
            })
            .await?;
        Ok(place_from_declarations_with(
            env,
            effects,
            declarations,
            RequiresExplicitReExport::No,
            None,
        )
        .await?
        .ignore_conflicting_declarations())
    }

    /// Looks up the type of an "implicit global symbol". Returns [`Place::Undefined`] if
    /// `name` is not present as an implicit symbol in module-global namespaces.
    ///
    /// Implicit global symbols are symbols such as `__doc__`, `__name__`, and `__file__`
    /// that are implicitly defined in every module's global scope. Because their type is
    /// always the same, we simply look these up as instance attributes on `types.ModuleType`.
    ///
    /// Note that this function should only be used as a fallback if a symbol is being looked
    /// up in the global scope **from within the same file**. If the symbol is being looked up
    /// from outside the file (e.g. via imports), use [`super::imported_symbol`] (or fallback logic
    /// like the logic used in that function) instead. The reason is that this function returns
    /// [`Place::Undefined`] for `__init__` and `__dict__` (which cannot be found in globals if
    /// the lookup is being done from the same file) -- but these symbols *are* available in the
    /// global scope if they're being imported **from a different file**.
    pub(crate) fn module_type_implicit_global_symbol<'db>(
        db: &'db dyn Db,
        file: ProgramFile<'db>,
        name: &str,
    ) -> PlaceAndQualifiers<'db> {
        match module_type_implicit_global_symbol_sync(
            db,
            file,
            name,
            &OrdinaryModuleGlobalSymbolEffects,
        ) {
            Ok(place) => place,
            Err(never) => match never {},
        }
    }

    /// An internal micro-optimisation for `module_type_implicit_global_symbol`.
    ///
    /// This function returns a list of the symbols that typeshed declares in the
    /// body scope of the stub for the class `types.ModuleType`.
    ///
    /// The returned list excludes the attributes `__dict__` and `__init__`. These are very
    /// special members that can be accessed as attributes on the module when imported,
    /// but cannot be accessed as globals *inside* the module.
    ///
    /// The list also excludes `__getattr__`. `__getattr__` is even more special: it doesn't
    /// exist at runtime, but typeshed includes it to reduce false positives associated with
    /// functions that dynamically import modules and return `Instance(types.ModuleType)`.
    /// We should ignore it for any known module-literal type.
    ///
    /// Conceptually this function could be a `Set` rather than a list,
    /// but the number of symbols declared in this scope is likely to be very small,
    /// so the cost of hashing the names is likely to be more expensive than it's worth.
    fn module_type_symbols_from_scope(
        db: &dyn Db,
        module_type_scope: ScopeId<'_>,
    ) -> smallvec::SmallVec<[ast::name::Name; 8]> {
        let module_type_symbol_table = place_table(db, module_type_scope);

        module_type_symbol_table
            .symbols()
            .filter(|symbol| is_implicit_module_global(symbol))
            .map(Symbol::name)
            .cloned()
            .collect()
    }

    pub(crate) fn is_implicit_module_global(symbol: &Symbol) -> bool {
        symbol.is_declared()
            && !matches!(
                symbol.name().as_str(),
                "__dict__" | "__getattr__" | "__init__"
            )
    }

    pub(super) fn module_type_symbols<'db>(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
    ) -> &'db [ast::name::Name] {
        module_type_symbols_inner(db, env.program(db))
    }

    #[salsa::tracked(attempt = ReturnOnly,
        returns(deref),
        cycle_initial=|_, _, _| smallvec::SmallVec::default(),
        heap_size=ruff_memory_usage::heap_size
    )]
    fn module_type_symbols_inner<'db>(
        db: &'db dyn Db,
        program: Program<'db>,
    ) -> smallvec::SmallVec<[ast::name::Name; 8]> {
        let env = ProgramEnvironment::from_program(program);
        let Some(module_type_scope) = module_type_body_scope(db, &env) else {
            // The most likely way we get here is if a user specified a `--custom-typeshed-dir`
            // without a resolvable `ModuleType` class in the `stdlib/types.pyi` stub.
            return smallvec::SmallVec::default();
        };
        module_type_symbols_from_scope(db, module_type_scope)
    }

    /// Returns an iterator over all implicit module global symbols and their types.
    ///
    /// This is used for completions in the global scope of a module. It returns
    /// the correct types for special-cased symbols like `__file__` (which is `str`
    /// for the current module, not `str | None`).
    pub(crate) fn all_implicit_module_globals<'db>(
        db: &'db dyn Db,
        file: ProgramFile<'db>,
    ) -> impl Iterator<Item = (Name, Type<'db>)> + 'db {
        // Special-cased implicit globals that are not in `module_type_symbols`
        let special_cased = ["__builtins__", "__debug__", "__warningregistry__"]
            .into_iter()
            .map(Name::new_static);

        // All symbols from ModuleType (already includes `__file__`, `__name__`, etc.)
        let env = ProgramEnvironment::from_file(file);
        let module_type_syms = module_type_symbols(db, &env).iter().cloned();

        // Combine and map to (name, type) pairs
        special_cased
            .chain(module_type_syms)
            .filter_map(move |name| {
                let place = module_type_implicit_global_symbol(db, file, name.as_str());
                // Only include bound symbols
                place.place.ignore_possibly_undefined().map(|ty| (name, ty))
            })
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use crate::db::tests::setup_db;

        #[test]
        fn module_type_symbols_includes_declared_types_but_not_referenced_types() {
            let db = setup_db();
            let db = &db;
            let env = db.program_environment();
            let symbol_names = module_type_symbols(db, &env);

            let dunder_name_symbol_name = ast::name::Name::new_static("__name__");
            assert!(symbol_names.contains(&dunder_name_symbol_name));

            let property_symbol_name = ast::name::Name::new_static("property");
            assert!(!symbol_names.contains(&property_symbol_name));
        }
    }
}

/// Looks up the type of an "implicit class body symbol". Returns [`Place::Undefined`] if
/// `name` is not present as an implicit symbol in class bodies.
///
/// Implicit class body symbols are symbols such as `__qualname__`, `__module__`, `__doc__`,
/// and `__firstlineno__` that Python implicitly makes available inside a class body during
/// class creation.
///
/// See <https://docs.python.org/3/reference/datamodel.html#creating-the-class-object>
pub(crate) fn class_body_implicit_symbol<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    name: &str,
) -> PlaceAndQualifiers<'db> {
    match class_body_implicit_symbol_sync(env, name, &OrdinaryClassBodySymbolEffects { db }) {
        Ok(place) => place,
        Err(never) => match never {},
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) enum RequiresExplicitReExport {
    Yes,
    No,
}

impl RequiresExplicitReExport {
    const fn is_yes(self) -> bool {
        matches!(self, RequiresExplicitReExport::Yes)
    }
}

/// Specifies which definitions should be considered when looking up a place.
///
/// In the example below, the `EndOfScope` variant would consider the `x = 2` and `x = 3` definitions,
/// while the `AllReachable` variant would also consider the `x = 1` definition.
/// ```py
/// def _():
///     x = 1
///
///     x = 2
///
///     if flag():
///         x = 3
/// ```
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) enum ConsideredDefinitions {
    /// Consider only the definitions that are "live" at the end of the scope, i.e. those
    /// that have not been shadowed or deleted.
    EndOfScope,
    /// Consider all definitions that are reachable from the start of the scope.
    AllReachable,
}

pub(crate) fn preserve_raw_public_type(
    db: &dyn Db,
    scope: ScopeId<'_>,
    place_id: ScopedPlaceId,
) -> bool {
    let symbol_name = place_id
        .as_symbol()
        .map(|symbol_id| place_table(db, scope).symbol(symbol_id).name().as_str());
    preserve_raw_public_type_in_scope(scope.scope(db), symbol_name, scope.file(db).is_stub(db))
}

pub(crate) fn preserve_raw_public_type_in_scope(
    scope: &Scope,
    symbol_name: Option<&str>,
    in_stub_file: bool,
) -> bool {
    // `__slots__` is a symbol with special behavior in Python's runtime. It can be
    // modified externally, but those changes do not take effect. We therefore issue
    // a diagnostic if we see it being modified externally. In type inference, we
    // can assign a "narrow" type to it even if it is not *declared*. This means we do
    // not have to adjust its public type.
    //
    // `TYPE_CHECKING` is a special variable that should only be assigned `False`
    // at runtime, but is always considered `True` in type checking.
    // See mdtest/known_constants.md#user-defined-type_checking for details.
    let is_considered_non_modifiable = matches!(symbol_name, Some("__slots__" | "TYPE_CHECKING"));

    // Module-level globals can be mutated externally, and strict application of the
    // gradual guarantee would suggest that if not annotated, all such external mutations
    // should be valid. However, external modifications (or modifications through `global`
    // statements) that would require a different public type are relatively rare. From a
    // practical perspective, we get a better user experience by trusting the inferred type
    // by default, and only requiring annotation for the rare case.
    let is_module_global = scope.kind().is_module();

    // If the visibility of the scope is private (like for a function scope), we also keep
    // the raw type, because the symbol cannot be modified externally.
    let scope_has_private_visibility = scope.visibility().is_private();

    // We generally trust undeclared places in stubs and expose the raw type.
    is_considered_non_modifiable || is_module_global || scope_has_private_visibility || in_stub_file
}

async fn is_non_exported_with<'db, E: SourcePlaceEffects<'db>>(
    effects: &E,
    definition: Definition<'db>,
    reexport: RequiresExplicitReExport,
) -> Result<bool, E::Error> {
    if !reexport.is_yes() || effects.definition_is_reexported(definition).await? {
        return Ok(false);
    }
    Ok(!effects.is_reexported(definition).await?)
}

async fn is_undefined_or_non_exported_with<'db, E: SourcePlaceEffects<'db>>(
    effects: &E,
    state: DefinitionState<'db>,
    reexport: RequiresExplicitReExport,
) -> Result<bool, E::Error> {
    match state {
        DefinitionState::Undefined => Ok(true),
        DefinitionState::Deleted => Ok(false),
        DefinitionState::Defined(definition) => {
            is_non_exported_with(effects, definition, reexport).await
        }
    }
}

#[cfg(test)]
mod tests {
    use std::assert_matches;

    use super::*;
    use crate::db::tests::{TestDb, setup_db};

    #[test]
    fn test_symbol_or_fall_back_to() {
        use Definedness::{AlwaysDefined, PossiblyUndefined};
        use TypeOrigin::Inferred;

        let db = setup_db();
        let db = &db;
        let env = db.program_environment();
        let ty1 = Type::int_literal(1);
        let ty2 = Type::int_literal(2);

        let unbound = || PlaceAndQualifiers::default();

        let possibly_unbound_ty1 = || {
            Place::Defined(DefinedPlace {
                ty: ty1,
                origin: Inferred,
                definedness: PossiblyUndefined,
                public_type_policy: PublicTypePolicy::Raw,
                provenance: Provenance::Unknown,
            })
            .with_qualifiers(TypeQualifiers::empty())
        };
        let possibly_unbound_ty2 = || {
            Place::Defined(DefinedPlace {
                ty: ty2,
                origin: Inferred,
                definedness: PossiblyUndefined,
                public_type_policy: PublicTypePolicy::Raw,
                provenance: Provenance::Unknown,
            })
            .with_qualifiers(TypeQualifiers::empty())
        };

        let bound_ty1 = || {
            Place::Defined(DefinedPlace {
                ty: ty1,
                origin: Inferred,
                definedness: AlwaysDefined,
                public_type_policy: PublicTypePolicy::Raw,
                provenance: Provenance::Unknown,
            })
            .with_qualifiers(TypeQualifiers::empty())
        };
        let bound_ty2 = || {
            Place::Defined(DefinedPlace {
                ty: ty2,
                origin: Inferred,
                definedness: AlwaysDefined,
                public_type_policy: PublicTypePolicy::Raw,
                provenance: Provenance::Unknown,
            })
            .with_qualifiers(TypeQualifiers::empty())
        };

        // Start from an unbound symbol
        assert_eq!(unbound().or_fall_back_to(db, &env, unbound), unbound());
        assert_eq!(
            unbound().or_fall_back_to(db, &env, possibly_unbound_ty1),
            possibly_unbound_ty1()
        );
        assert_eq!(unbound().or_fall_back_to(db, &env, bound_ty1), bound_ty1());

        // Start from a possibly unbound symbol
        assert_eq!(
            possibly_unbound_ty1().or_fall_back_to(db, &env, unbound),
            possibly_unbound_ty1()
        );
        assert_eq!(
            possibly_unbound_ty1().or_fall_back_to(db, &env, possibly_unbound_ty2),
            Place::Defined(DefinedPlace {
                ty: UnionType::from_elements(db, &env, [ty1, ty2]),
                origin: Inferred,
                definedness: PossiblyUndefined,
                public_type_policy: PublicTypePolicy::Raw,
                provenance: Provenance::Unknown,
            })
            .into()
        );
        assert_eq!(
            possibly_unbound_ty1().or_fall_back_to(db, &env, bound_ty2),
            Place::Defined(DefinedPlace {
                ty: UnionType::from_elements(db, &env, [ty1, ty2]),
                origin: Inferred,
                definedness: AlwaysDefined,
                public_type_policy: PublicTypePolicy::Raw,
                provenance: Provenance::Unknown,
            })
            .into()
        );

        // Start from a definitely bound symbol
        assert_eq!(bound_ty1().or_fall_back_to(db, &env, unbound), bound_ty1());
        assert_eq!(
            bound_ty1().or_fall_back_to(db, &env, possibly_unbound_ty2),
            bound_ty1()
        );
        assert_eq!(
            bound_ty1().or_fall_back_to(db, &env, bound_ty2),
            bound_ty1()
        );
    }

    #[track_caller]
    fn assert_bound_string_symbol<'db>(db: &'db TestDb, symbol: Place<'db>) {
        assert_matches!(
            symbol,
            Place::Defined(DefinedPlace {
                ty: Type::NominalInstance(_),
                definedness: Definedness::AlwaysDefined,
                ..
            })
        );
        assert_eq!(
            symbol.expect_type(),
            KnownClass::Str.to_instance(db, &db.program_environment())
        );
    }

    #[test]
    fn implicit_builtin_globals() {
        let db = setup_db();
        assert_bound_string_symbol(
            &db,
            builtins_symbol(&db, &db.program_environment(), "__name__").place,
        );
    }

    #[test]
    fn implicit_typing_globals() {
        let db = setup_db();
        assert_bound_string_symbol(
            &db,
            typing_symbol(&db, &db.program_environment(), "__name__").place,
        );
    }

    #[test]
    fn implicit_typing_extensions_globals() {
        let db = setup_db();
        assert_bound_string_symbol(
            &db,
            typing_extensions_symbol(&db, &db.program_environment(), "__name__").place,
        );
    }

    #[test]
    fn implicit_sys_globals() {
        let db = setup_db();
        assert_bound_string_symbol(
            &db,
            known_module_symbol(&db, &db.program_environment(), KnownModule::Sys, "__name__").place,
        );
    }
}
