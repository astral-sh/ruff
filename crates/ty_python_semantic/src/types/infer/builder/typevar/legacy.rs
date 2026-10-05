use std::convert::Infallible;
use std::fmt;

use ruff_python_ast::name::Name;
use ruff_python_ast::{self as ast, PythonVersion};
use ruff_text_size::{Ranged, TextRange};
use ty_python_core::definition::{Definition, DefinitionKind};
use ty_python_core::place::ScopedPlaceId;
use ty_python_core::reachability_constraints::ScopedReachabilityConstraintId;
use ty_python_core::scope::FileScopeId;
use ty_python_core::{BindingWithConstraintsIterator, DeclarationsIterator, ScopedDefinitionId};

use super::super::TypeInferenceBuilder;
use crate::reachability::is_reachable;
use crate::types::diagnostic::{INVALID_LEGACY_TYPE_VARIABLE, report_mismatched_type_name};
use crate::types::typevar::{
    TypeVarBoundOrConstraintsEvaluation, TypeVarDefaultEvaluation, TypeVarIdentity, TypeVarInstance,
};
use crate::types::{
    KnownClass, KnownInstanceType, Truthiness, Type, TypeContext, TypeVarKind, TypeVarVariance,
};

pub(in crate::types::infer::builder) struct State<'db, 'expr> {
    target: &'expr ast::Expr,
    call: &'expr ast::ExprCall,
    definition: Definition<'db>,
    known_class: KnownClass,
    initialized: bool,
    assume_all_features: bool,
    argument_cursor: usize,
    keyword_cursor: usize,
    has_bound: bool,
    default: Option<TypeVarDefaultEvaluation<'db>>,
    covariant: bool,
    contravariant: bool,
    infer_variance: bool,
    name: Option<(Type<'db>, &'expr ast::Expr)>,
    complete: Option<Type<'db>>,
}

pub(in crate::types::infer::builder) struct Pending<'db, 'expr> {
    state: State<'db, 'expr>,
    kind: ArgumentKind,
    expression: &'expr ast::Expr,
}

pub(in crate::types::infer::builder) enum Action<'db, 'expr> {
    Infer {
        pending: Pending<'db, 'expr>,
        expression: &'expr ast::Expr,
        context: TypeContext<'db>,
    },
    Complete(Type<'db>),
}

#[derive(Clone, Copy)]
enum ArgumentKind {
    Name,
    Covariant,
    Contravariant,
    InferVariance,
    Ignore,
}

enum Keyword<'expr> {
    Name,
    Bound,
    Covariant,
    Contravariant,
    Default,
    InferVariance,
    Unknown(&'expr str),
    Starred,
}

pub(in crate::types::infer::builder) enum HeaderDiagnostic<'a> {
    Message(&'static str),
    UnknownKeyword(&'a str),
    AmbiguousVariance(&'static str),
}

impl fmt::Display for HeaderDiagnostic<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Message(message) => f.write_str(message),
            Self::UnknownKeyword(name) => {
                write!(f, "Unknown keyword argument `{name}` in `TypeVar` creation")
            }
            Self::AmbiguousVariance(name) => write!(
                f,
                "The `{name}` parameter of `TypeVar` cannot have an ambiguous truthiness"
            ),
        }
    }
}

pub(in crate::types::infer::builder) struct Candidates<'db> {
    bindings: BindingWithConstraintsIterator<'db, 'db>,
    declarations: Option<DeclarationsIterator<'db, 'db>>,
}

impl<'db> Candidates<'db> {
    pub(in crate::types::infer::builder) fn new(
        builder: &TypeInferenceBuilder<'db, '_>,
        scope: FileScopeId,
        place: ScopedPlaceId,
        declarations: bool,
    ) -> Self {
        let use_def = builder.index.use_def_map(scope);
        Self {
            bindings: use_def.reachable_bindings(place),
            declarations: declarations.then(|| use_def.reachable_declarations(place)),
        }
    }

    pub(in crate::types::infer::builder) fn step_work(&self) -> Option<usize> {
        if self.bindings.traversal_len() > 0 {
            return Some(3);
        }
        self.declarations
            .as_ref()
            .map_or(0, DeclarationsIterator::traversal_len)
            .checked_add(3)
    }

    pub(in crate::types::infer::builder) fn next(&mut self) -> Option<Candidate<'db>> {
        if let Some(binding) = self.bindings.next() {
            return Some(Candidate {
                order: binding.binding_order,
                definition: binding.binding.definition(),
                reachability: binding.reachability_constraint,
            });
        }
        self.declarations
            .as_mut()?
            .next()
            .map(|declaration| Candidate {
                order: declaration.declaration_order,
                definition: declaration.declaration.definition(),
                reachability: declaration.reachability_constraint,
            })
    }
}

pub(in crate::types::infer::builder) struct Candidate<'db> {
    order: ScopedDefinitionId,
    definition: Option<Definition<'db>>,
    reachability: ScopedReachabilityConstraintId,
}

pub(in crate::types::infer::builder) struct HeaderFacts;
pub(in crate::types::infer::builder) struct OrdinaryLegacyTypeVarEffects;

pub(in crate::types::infer::builder) fn new<'db, 'expr>(
    target: &'expr ast::Expr,
    call: &'expr ast::ExprCall,
    definition: Definition<'db>,
    known_class: KnownClass,
) -> State<'db, 'expr> {
    State {
        target,
        call,
        definition,
        known_class,
        initialized: false,
        assume_all_features: false,
        argument_cursor: 0,
        keyword_cursor: 0,
        has_bound: false,
        default: None,
        covariant: false,
        contravariant: false,
        infer_variance: false,
        name: None,
        complete: None,
    }
}

pub(in crate::types::infer::builder) fn next_argument<'expr>(
    call: &'expr ast::ExprCall,
    cursor: &mut usize,
) -> Option<&'expr ast::Expr> {
    let argument = call.arguments.args.get(*cursor)?;
    *cursor += 1;
    Some(argument)
}

pub(in crate::types::infer::builder) fn next_keyword<'expr>(
    call: &'expr ast::ExprCall,
    cursor: &mut usize,
) -> Option<&'expr ast::Keyword> {
    let keyword = call.arguments.keywords.get(*cursor)?;
    *cursor += 1;
    Some(keyword)
}

ty_mapping_probe_macros::shared_semantic_family! {
    #[synchronous(SynchronousLegacyTypeVarEffects)]
    pub(in crate::types::infer::builder) trait LegacyTypeVarEffects<'db, 'ast> {
        type Error;
        #[operation(source)]
        async fn in_stub(&self, builder: &TypeInferenceBuilder<'db, 'ast>) -> Result<bool, Self::Error>;
        #[operation(source)]
        async fn python_version(&self, builder: &TypeInferenceBuilder<'db, 'ast>) -> Result<PythonVersion, Self::Error>;
        #[operation(local)]
        #[progress]
        async fn next_argument<'expr>(&self, call: &'expr ast::ExprCall, cursor: &mut usize) -> Result<Option<&'expr ast::Expr>, Self::Error>;
        #[operation(local)]
        #[progress]
        async fn next_keyword<'expr>(&self, call: &'expr ast::ExprCall, cursor: &mut usize) -> Result<Option<&'expr ast::Keyword>, Self::Error>;
        #[operation(child)]
        async fn truthiness(&self, builder: &TypeInferenceBuilder<'db, 'ast>, ty: Type<'db>) -> Result<Truthiness, Self::Error>;
        #[operation(source)]
        async fn string_value(&self, builder: &TypeInferenceBuilder<'db, 'ast>, ty: Type<'db>) -> Result<Option<&'db str>, Self::Error>;
        #[operation(local)]
        async fn same_name(&self, actual: &str, expected: &Name) -> Result<bool, Self::Error>;
        #[operation(child)]
        async fn error(&self, builder: &TypeInferenceBuilder<'db, 'ast>, diagnostic: HeaderDiagnostic<'_>, range: TextRange) -> Result<Type<'db>, Self::Error>;
        #[operation(source)]
        async fn mismatched_name(&self, builder: &TypeInferenceBuilder<'db, 'ast>, range: TextRange, expected: &Name, actual: &str, ty: Type<'db>) -> Result<(), Self::Error>;
        #[operation(source)]
        async fn redefinition(&self, builder: &TypeInferenceBuilder<'db, 'ast>, target: &ast::Expr, name: &Name, previous: Definition<'db>) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn mark_deferred(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, definition: Definition<'db>) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn intern_identity(&self, builder: &TypeInferenceBuilder<'db, 'ast>, name: &Name, definition: Definition<'db>) -> Result<TypeVarIdentity<'db>, Self::Error>;
        #[operation(local)]
        async fn intern_typevar(&self, builder: &TypeInferenceBuilder<'db, 'ast>, identity: TypeVarIdentity<'db>, bounds: Option<TypeVarBoundOrConstraintsEvaluation<'db>>, variance: Option<TypeVarVariance>, default: Option<TypeVarDefaultEvaluation<'db>>) -> Result<TypeVarInstance<'db>, Self::Error>;
        #[operation(source)]
        async fn scope_place(&self, builder: &TypeInferenceBuilder<'db, 'ast>, definition: Definition<'db>) -> Result<(FileScopeId, ScopedPlaceId), Self::Error>;
        #[operation(local)]
        async fn candidates(&self, builder: &TypeInferenceBuilder<'db, 'ast>, scope: FileScopeId, place: ScopedPlaceId, declarations: bool) -> Result<Candidates<'db>, Self::Error>;
        #[operation(local)]
        #[progress]
        async fn next_candidate(&self, candidates: &mut Candidates<'db>) -> Result<Option<Candidate<'db>>, Self::Error>;
        #[operation(child)]
        async fn reachable(&self, builder: &TypeInferenceBuilder<'db, 'ast>, scope: FileScopeId, reachability: ScopedReachabilityConstraintId) -> Result<bool, Self::Error>;
        #[operation(source)]
        async fn user_visible(&self, builder: &TypeInferenceBuilder<'db, 'ast>, definition: Definition<'db>) -> Result<bool, Self::Error>;
        #[operation(source)]
        async fn forwarded_owner(&self, builder: &TypeInferenceBuilder<'db, 'ast>, scope: FileScopeId, place: ScopedPlaceId) -> Result<Option<(FileScopeId, ScopedPlaceId)>, Self::Error>;
        #[operation(source)]
        async fn forwarded_before(&self, builder: &TypeInferenceBuilder<'db, 'ast>, scope: FileScopeId, owner_scope: FileScopeId, owner_place: ScopedPlaceId) -> Result<Option<ScopedDefinitionId>, Self::Error>;
        #[operation(child)]
        async fn previous_in(&self, builder: &TypeInferenceBuilder<'db, 'ast>, scope: FileScopeId, place: ScopedPlaceId, before: ScopedDefinitionId) -> Result<Option<Definition<'db>>, Self::Error>;
        #[operation(child)]
        async fn previous(&self, builder: &TypeInferenceBuilder<'db, 'ast>, definition: Definition<'db>) -> Result<Option<Definition<'db>>, Self::Error>;
    }

    #[finite_capability]
    impl HeaderFacts {
        fn initialize(&self, state: &mut State<'_, '_>, in_stub: bool) {
            state.assume_all_features = in_stub || state.known_class == KnownClass::ExtensionsTypeVar;
            state.initialized = true;
        }
        fn set_bound(&self, state: &mut State<'_, '_>) { state.has_bound = true; }
        fn set_default(&self, state: &mut State<'_, '_>) { state.default = Some(TypeVarDefaultEvaluation::Lazy); }
        fn set_name<'db, 'expr>(&self, state: &mut State<'db, 'expr>, ty: Type<'db>, expression: &'expr ast::Expr) { state.name = Some((ty, expression)); }
        fn complete<'db>(&self, state: &mut State<'db, '_>, ty: Type<'db>) { state.complete = Some(ty); }
        fn older(&self, order: ScopedDefinitionId, before: ScopedDefinitionId) -> bool { order < before }
        fn same_definition<'db>(&self, candidate: Option<Definition<'db>>, definition: Definition<'db>) -> bool { candidate == Some(definition) }
        fn before_version(&self, version: PythonVersion, minimum: PythonVersion) -> bool { version < minimum }
        fn deferred<'db>(&self, bounds: Option<TypeVarBoundOrConstraintsEvaluation<'db>>, default: Option<TypeVarDefaultEvaluation<'db>>) -> bool { bounds.is_some() || default.is_some() }
        fn keyword<'expr>(&self, keyword: &'expr ast::Keyword) -> Keyword<'expr> {
            match keyword.arg.as_ref().map(|identifier| identifier.id().as_str()) {
                Some("name") => Keyword::Name,
                Some("bound") => Keyword::Bound,
                Some("covariant") => Keyword::Covariant,
                Some("contravariant") => Keyword::Contravariant,
                Some("default") => Keyword::Default,
                Some("infer_variance") => Keyword::InferVariance,
                Some(name) => Keyword::Unknown(name),
                None => Keyword::Starred,
            }
        }
        fn starred(&self, expression: &ast::Expr) -> bool { expression.is_starred_expr() }
        fn positional_empty(&self, call: &ast::ExprCall) -> bool { call.arguments.args.is_empty() }
        fn first<'expr>(&self, call: &'expr ast::ExprCall) -> Option<&'expr ast::Expr> { call.arguments.args.first() }
        fn constraints(&self, call: &ast::ExprCall) -> usize { call.arguments.args.len().saturating_sub(1) }
        fn constraint_range(&self, call: &ast::ExprCall) -> TextRange { call.arguments.args[1].range() }
        fn call_range(&self, call: &ast::ExprCall) -> TextRange { call.range() }
        fn expression_range(&self, expression: &ast::Expr) -> TextRange { expression.range() }
        fn keyword_range(&self, keyword: &ast::Keyword) -> TextRange { keyword.range() }
        fn target_name<'expr>(&self, target: &'expr ast::Expr) -> Option<&'expr Name> {
            match target { ast::Expr::Name(name) => Some(&name.id), _ => None }
        }
        fn infer<'db, 'expr>(&self, state: State<'db, 'expr>, kind: ArgumentKind, expression: &'expr ast::Expr) -> Action<'db, 'expr> {
            Action::Infer { pending: Pending { state, kind, expression }, expression, context: TypeContext::default() }
        }
        fn typevar<'db>(&self, typevar: TypeVarInstance<'db>) -> Type<'db> { Type::KnownInstance(KnownInstanceType::TypeVar(typevar)) }
        fn variance_name(&self, kind: ArgumentKind) -> &'static str {
            match kind { ArgumentKind::Covariant => "covariant", ArgumentKind::Contravariant => "contravariant", ArgumentKind::InferVariance => "infer_variance", _ => "" }
        }
        fn set_true(&self, state: &mut State<'_, '_>, kind: ArgumentKind) {
            match kind { ArgumentKind::Covariant => state.covariant = true, ArgumentKind::Contravariant => state.contravariant = true, ArgumentKind::InferVariance => state.infer_variance = true, _ => {} }
        }
    }

    #[synchronous(previous_in_sync)]
    #[capabilities(effects = LegacyTypeVarEffects, facts = HeaderFacts)]
    #[passive_values()]
    pub(in crate::types::infer::builder) async fn previous_in_with<'db, 'ast, E: LegacyTypeVarEffects<'db, 'ast>>(
        builder: &TypeInferenceBuilder<'db, 'ast>, scope: FileScopeId, place: ScopedPlaceId, before: ScopedDefinitionId, facts: HeaderFacts, effects: &E,
    ) -> Result<Option<Definition<'db>>, E::Error> {
        let mut candidates = effects.candidates(builder, scope, place, true).await?;
        #[passive_state]
        let mut previous = None;
        #[cursor_loop]
        while let Some(candidate) = effects.next_candidate(&mut candidates).await? {
            if !facts.older(candidate.order, before) { continue; }
            if !effects.reachable(builder, scope, candidate.reachability).await? { continue; }
            let Some(definition) = candidate.definition else { continue; };
            if !effects.user_visible(builder, definition).await? { continue; }
            if let Some((order, _)) = previous && facts.older(candidate.order, order) { continue; }
            previous = Some((candidate.order, definition));
        }
        match previous { Some((_, definition)) => Ok(Some(definition)), None => Ok(None) }
    }

    #[synchronous(previous_sync)]
    #[capabilities(effects = LegacyTypeVarEffects, facts = HeaderFacts)]
    #[passive_values()]
    pub(in crate::types::infer::builder) async fn previous_with<'db, 'ast, E: LegacyTypeVarEffects<'db, 'ast>>(
        builder: &TypeInferenceBuilder<'db, 'ast>, definition: Definition<'db>, facts: HeaderFacts, effects: &E,
    ) -> Result<Option<Definition<'db>>, E::Error> {
        let (scope, place) = effects.scope_place(builder, definition).await?;
        let mut candidates = effects.candidates(builder, scope, place, false).await?;
        #[passive_state]
        let mut before = None;
        #[cursor_loop]
        while let Some(candidate) = effects.next_candidate(&mut candidates).await? {
            if facts.same_definition(candidate.definition, definition) { before = Some(candidate.order); break; }
        }
        if let Some(before) = before && let Some(previous) = effects.previous_in(builder, scope, place, before).await? { return Ok(Some(previous)); }
        let Some((owner_scope, owner_place)) = effects.forwarded_owner(builder, scope, place).await? else { return Ok(None); };
        let Some(before) = effects.forwarded_before(builder, scope, owner_scope, owner_place).await? else { return Ok(None); };
        effects.previous_in(builder, owner_scope, owner_place, before).await
    }

    #[synchronous(advance_impl_sync)]
    #[capabilities(effects = LegacyTypeVarEffects, facts = HeaderFacts)]
    #[passive_values(Action::Complete, HeaderDiagnostic::Message, HeaderDiagnostic::UnknownKeyword, ArgumentKind::Name, ArgumentKind::Covariant, ArgumentKind::Contravariant, ArgumentKind::InferVariance, ArgumentKind::Ignore, KnownClass::ExtensionsTypeVar, TypeVarDefaultEvaluation::Lazy, TypeVarBoundOrConstraintsEvaluation::LazyUpperBound, TypeVarBoundOrConstraintsEvaluation::LazyConstraints, TypeVarVariance::Covariant, TypeVarVariance::Contravariant, TypeVarVariance::Invariant, PythonVersion::PY312, PythonVersion::PY313)]
    async fn advance_impl_with<'db, 'ast, 'expr, E: LegacyTypeVarEffects<'db, 'ast>>(
        mut state: State<'db, 'expr>, builder: &mut TypeInferenceBuilder<'db, 'ast>, facts: HeaderFacts, effects: &E,
    ) -> Result<Action<'db, 'expr>, E::Error> {
        if let Some(ty) = state.complete { return Ok(Action::Complete(ty)); }
        if !state.initialized {
            facts.initialize(&mut state, effects.in_stub(builder).await?);
        }
        #[cursor_loop]
        while let Some(argument) = effects.next_argument(state.call, &mut state.argument_cursor).await? {
            if facts.starred(argument) {
                return Ok(Action::Complete(effects.error(builder, HeaderDiagnostic::Message("Starred arguments are not supported in `TypeVar` creation"), facts.expression_range(argument)).await?));
            }
        }
        #[cursor_loop]
        while let Some(keyword) = effects.next_keyword(state.call, &mut state.keyword_cursor).await? {
            match facts.keyword(keyword) {
                Keyword::Starred => return Ok(Action::Complete(effects.error(builder, HeaderDiagnostic::Message("Starred arguments are not supported in `TypeVar` creation"), facts.keyword_range(keyword)).await?)),
                Keyword::Name => {
                    // Duplicate keyword argument is a syntax error, so we don't have to check if
                    // `state.name.is_some()` here.
                    if !facts.positional_empty(state.call) {
                        return Ok(Action::Complete(effects.error(builder, HeaderDiagnostic::Message("The `name` parameter of `TypeVar` can only be provided once."), facts.keyword_range(keyword)).await?));
                    }
                    return Ok(facts.infer(state, ArgumentKind::Name, &keyword.value));
                }
                Keyword::Bound => facts.set_bound(&mut state),
                Keyword::Covariant => return Ok(facts.infer(state, ArgumentKind::Covariant, &keyword.value)),
                Keyword::Contravariant => return Ok(facts.infer(state, ArgumentKind::Contravariant, &keyword.value)),
                Keyword::Default => {
                    if !state.assume_all_features && facts.before_version(effects.python_version(builder).await?, PythonVersion::PY313) {
                        // We don't return here; this error is informational since this will error
                        // at runtime, but the user's intent is plain, we may as well respect it.
                        effects.error(builder, HeaderDiagnostic::Message("The `default` parameter of `typing.TypeVar` was added in Python 3.13"), facts.keyword_range(keyword)).await?;
                    }
                    facts.set_default(&mut state);
                }
                Keyword::InferVariance => {
                    if !state.assume_all_features && facts.before_version(effects.python_version(builder).await?, PythonVersion::PY312) {
                        // We don't return here; this error is informational since this will error
                        // at runtime, but the user's intent is plain, we may as well respect it.
                        effects.error(builder, HeaderDiagnostic::Message("The `infer_variance` parameter of `typing.TypeVar` was added in Python 3.12"), facts.keyword_range(keyword)).await?;
                    }
                    return Ok(facts.infer(state, ArgumentKind::InferVariance, &keyword.value));
                }
                Keyword::Unknown(name) => {
                    // We don't return here; this error is informational since this will error
                    // at runtime, but it will likely cause fewer cascading errors if we just
                    // ignore the unknown keyword and still understand as much of the typevar as we
                    // can.
                    effects.error(builder, HeaderDiagnostic::UnknownKeyword(name), facts.keyword_range(keyword)).await?;
                    return Ok(facts.infer(state, ArgumentKind::Ignore, &keyword.value));
                }
            }
        }
        let variance = match (state.covariant, state.contravariant, state.infer_variance) {
            (true, true, _) => return Ok(Action::Complete(effects.error(builder, HeaderDiagnostic::Message("A `TypeVar` cannot be both covariant and contravariant"), facts.call_range(state.call)).await?)),
            (true, false, true) | (false, true, true) => return Ok(Action::Complete(effects.error(builder, HeaderDiagnostic::Message("A `TypeVar` cannot specify variance when `infer_variance=True`"), facts.call_range(state.call)).await?)),
            (true, false, false) => Some(TypeVarVariance::Covariant),
            (false, true, false) => Some(TypeVarVariance::Contravariant),
            (false, false, false) => Some(TypeVarVariance::Invariant),
            (false, false, true) => None,
        };
        let Some((name_type, name_node)) = state.name else {
            if let Some(expression) = facts.first(state.call) { return Ok(facts.infer(state, ArgumentKind::Name, expression)); }
            return Ok(Action::Complete(effects.error(builder, HeaderDiagnostic::Message("The `name` parameter of `TypeVar` is required."), facts.call_range(state.call)).await?));
        };
        let Some(name) = effects.string_value(builder, name_type).await? else {
            return Ok(Action::Complete(effects.error(builder, HeaderDiagnostic::Message("The first argument to `TypeVar` must be a string literal."), facts.call_range(state.call)).await?));
        };
        let Some(target_name) = facts.target_name(state.target) else {
            return Ok(Action::Complete(effects.error(builder, HeaderDiagnostic::Message("A `TypeVar` definition must be a simple variable assignment"), facts.expression_range(state.target)).await?));
        };
        if !effects.same_name(name, target_name).await? {
            effects.mismatched_name(builder, facts.expression_range(name_node), target_name, name, name_type).await?;
        }
        if let Some(previous) = effects.previous(builder, state.definition).await? {
            effects.redefinition(builder, state.target, target_name, previous).await?;
        }
        // Inference of bounds, constraints, and defaults must be deferred, to avoid cycles. So we
        // only check presence/absence/number here.
        let bounds = match (state.has_bound, facts.constraints(state.call)) {
            (false, 0) => None,
            (true, 0) => Some(TypeVarBoundOrConstraintsEvaluation::LazyUpperBound),
            (true, _) => return Ok(Action::Complete(effects.error(builder, HeaderDiagnostic::Message("A `TypeVar` cannot have both a bound and constraints"), facts.call_range(state.call)).await?)),
            (_, 1) => return Ok(Action::Complete(effects.error(builder, HeaderDiagnostic::Message("A `TypeVar` cannot have exactly one constraint"), facts.constraint_range(state.call)).await?)),
            (false, _) => Some(TypeVarBoundOrConstraintsEvaluation::LazyConstraints),
        };
        if facts.deferred(bounds, state.default) { effects.mark_deferred(builder, state.definition).await?; }
        let identity = effects.intern_identity(builder, target_name, state.definition).await?;
        let typevar = effects.intern_typevar(builder, identity, bounds, variance, state.default).await?;
        Ok(Action::Complete(facts.typevar(typevar)))
    }

    #[synchronous(resume_impl_sync)]
    #[capabilities(effects = LegacyTypeVarEffects, facts = HeaderFacts)]
    #[passive_values(HeaderDiagnostic::AmbiguousVariance)]
    async fn resume_impl_with<'db, 'ast, 'expr, E: LegacyTypeVarEffects<'db, 'ast>>(
        pending: Pending<'db, 'expr>, ty: Type<'db>, builder: &mut TypeInferenceBuilder<'db, 'ast>, facts: HeaderFacts, effects: &E,
    ) -> Result<State<'db, 'expr>, E::Error> {
        let Pending { mut state, kind, expression } = pending;
        match kind {
            ArgumentKind::Name => facts.set_name(&mut state, ty, expression),
            ArgumentKind::Ignore => {},
            _ => match effects.truthiness(builder, ty).await? {
                Truthiness::AlwaysTrue => facts.set_true(&mut state, kind),
                Truthiness::AlwaysFalse => {},
                Truthiness::Ambiguous => facts.complete(&mut state, effects.error(builder, HeaderDiagnostic::AmbiguousVariance(facts.variance_name(kind)), facts.expression_range(expression)).await?),
            },
        }
        Ok(state)
    }
}

pub(in crate::types::infer::builder) async fn advance_with<
    'db,
    'ast,
    'expr,
    E: LegacyTypeVarEffects<'db, 'ast>,
>(
    state: State<'db, 'expr>,
    builder: &mut TypeInferenceBuilder<'db, 'ast>,
    effects: &E,
) -> Result<Action<'db, 'expr>, E::Error> {
    advance_impl_with(state, builder, HeaderFacts, effects).await
}

pub(in crate::types::infer::builder) fn advance_sync<
    'db,
    'ast,
    'expr,
    E: SynchronousLegacyTypeVarEffects<'db, 'ast>,
>(
    state: State<'db, 'expr>,
    builder: &mut TypeInferenceBuilder<'db, 'ast>,
    effects: &E,
) -> Result<Action<'db, 'expr>, E::Error> {
    advance_impl_sync(state, builder, HeaderFacts, effects)
}

pub(in crate::types::infer::builder) async fn resume_with<
    'db,
    'ast,
    'expr,
    E: LegacyTypeVarEffects<'db, 'ast>,
>(
    pending: Pending<'db, 'expr>,
    ty: Type<'db>,
    builder: &mut TypeInferenceBuilder<'db, 'ast>,
    effects: &E,
) -> Result<State<'db, 'expr>, E::Error> {
    resume_impl_with(pending, ty, builder, HeaderFacts, effects).await
}

pub(in crate::types::infer::builder) fn resume_sync<
    'db,
    'ast,
    'expr,
    E: SynchronousLegacyTypeVarEffects<'db, 'ast>,
>(
    pending: Pending<'db, 'expr>,
    ty: Type<'db>,
    builder: &mut TypeInferenceBuilder<'db, 'ast>,
    effects: &E,
) -> Result<State<'db, 'expr>, E::Error> {
    resume_impl_sync(pending, ty, builder, HeaderFacts, effects)
}

impl<'db, 'ast> SynchronousLegacyTypeVarEffects<'db, 'ast> for OrdinaryLegacyTypeVarEffects {
    type Error = Infallible;
    fn in_stub(&self, builder: &TypeInferenceBuilder<'db, 'ast>) -> Result<bool, Infallible> {
        Ok(builder.in_stub())
    }
    fn python_version(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
    ) -> Result<PythonVersion, Infallible> {
        Ok(builder.program_environment().python_version(builder.db()))
    }
    fn next_argument<'expr>(
        &self,
        call: &'expr ast::ExprCall,
        cursor: &mut usize,
    ) -> Result<Option<&'expr ast::Expr>, Infallible> {
        Ok(next_argument(call, cursor))
    }
    fn next_keyword<'expr>(
        &self,
        call: &'expr ast::ExprCall,
        cursor: &mut usize,
    ) -> Result<Option<&'expr ast::Keyword>, Infallible> {
        Ok(next_keyword(call, cursor))
    }
    fn truthiness(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        ty: Type<'db>,
    ) -> Result<Truthiness, Infallible> {
        Ok(ty.bool(builder.db(), builder.program_environment()))
    }
    fn string_value(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        ty: Type<'db>,
    ) -> Result<Option<&'db str>, Infallible> {
        Ok(ty
            .as_string_literal()
            .map(|literal| literal.value(builder.db())))
    }
    fn same_name(&self, actual: &str, expected: &Name) -> Result<bool, Infallible> {
        Ok(actual == expected)
    }
    fn error(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        diagnostic: HeaderDiagnostic<'_>,
        range: TextRange,
    ) -> Result<Type<'db>, Infallible> {
        if let Some(builder) = builder
            .context
            .report_lint(&INVALID_LEGACY_TYPE_VARIABLE, range)
        {
            builder.into_diagnostic(diagnostic);
        }
        // If the call doesn't create a valid typevar, we'll emit diagnostics and fall back to
        // just creating a regular instance of `typing.TypeVar`.
        Ok(KnownClass::TypeVar.to_instance(builder.db(), builder.program_environment()))
    }
    fn mismatched_name(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        range: TextRange,
        expected: &Name,
        actual: &str,
        ty: Type<'db>,
    ) -> Result<(), Infallible> {
        report_mismatched_type_name(
            &builder.context,
            range,
            "TypeVar",
            expected,
            Some(actual),
            ty,
        );
        Ok(())
    }
    fn redefinition(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        target: &ast::Expr,
        name: &Name,
        previous: Definition<'db>,
    ) -> Result<(), Infallible> {
        if let Some(diagnostic) = builder
            .context
            .report_lint(&INVALID_LEGACY_TYPE_VARIABLE, target)
        {
            let mut diagnostic = diagnostic
                .into_diagnostic(format_args!("Cannot redefine `{name}` as a type variable"));
            diagnostic.annotate(
                builder
                    .context
                    .secondary(previous.focus_range(builder.db(), builder.module()))
                    .message("Previously defined here"),
            );
        }
        Ok(())
    }
    fn mark_deferred(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        definition: Definition<'db>,
    ) -> Result<(), Infallible> {
        builder.deferred.insert(definition);
        Ok(())
    }
    fn intern_identity(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        name: &Name,
        definition: Definition<'db>,
    ) -> Result<TypeVarIdentity<'db>, Infallible> {
        Ok(TypeVarIdentity::new(
            builder.db(),
            name,
            Some(definition),
            TypeVarKind::LegacyTypeVar,
        ))
    }
    fn intern_typevar(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        identity: TypeVarIdentity<'db>,
        bounds: Option<TypeVarBoundOrConstraintsEvaluation<'db>>,
        variance: Option<TypeVarVariance>,
        default: Option<TypeVarDefaultEvaluation<'db>>,
    ) -> Result<TypeVarInstance<'db>, Infallible> {
        Ok(TypeVarInstance::new(
            builder.db(),
            identity,
            bounds,
            variance,
            default,
        ))
    }
    fn scope_place(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        definition: Definition<'db>,
    ) -> Result<(FileScopeId, ScopedPlaceId), Infallible> {
        Ok((
            definition.file_scope(builder.db()),
            definition.place(builder.db()),
        ))
    }
    fn candidates(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        scope: FileScopeId,
        place: ScopedPlaceId,
        declarations: bool,
    ) -> Result<Candidates<'db>, Infallible> {
        Ok(Candidates::new(builder, scope, place, declarations))
    }
    fn next_candidate(
        &self,
        candidates: &mut Candidates<'db>,
    ) -> Result<Option<Candidate<'db>>, Infallible> {
        Ok(candidates.next())
    }
    fn reachable(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        scope: FileScopeId,
        reachability: ScopedReachabilityConstraintId,
    ) -> Result<bool, Infallible> {
        Ok(is_reachable(
            builder.db(),
            builder.index.use_def_map(scope),
            reachability,
        ))
    }
    fn user_visible(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        definition: Definition<'db>,
    ) -> Result<bool, Infallible> {
        Ok(definition.kind(builder.db()).is_user_visible())
    }
    fn forwarded_owner(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        scope: FileScopeId,
        place: ScopedPlaceId,
    ) -> Result<Option<(FileScopeId, ScopedPlaceId)>, Infallible> {
        Ok(builder
            .forwarded_assignment_owner(scope, place.expect_symbol())
            .map(|(scope, place)| (scope, place.into())))
    }
    fn forwarded_before(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        scope: FileScopeId,
        owner_scope: FileScopeId,
        owner_place: ScopedPlaceId,
    ) -> Result<Option<ScopedDefinitionId>, Infallible> {
        Ok(builder
            .index
            .use_def_map(owner_scope)
            .reachable_bindings(owner_place)
            .find_map(|binding| {
                let definition = binding.binding.definition()?;
                let DefinitionKind::NestedBindings(nested) = definition.kind(builder.db()) else {
                    return None;
                };
                nested
                    .nested_declarations
                    .iter()
                    .any(|declaration| declaration.file_scope_id == scope)
                    .then_some(binding.binding_order)
            }))
    }
    fn previous_in(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        scope: FileScopeId,
        place: ScopedPlaceId,
        before: ScopedDefinitionId,
    ) -> Result<Option<Definition<'db>>, Infallible> {
        previous_in_sync(builder, scope, place, before, HeaderFacts, self)
    }
    fn previous(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        definition: Definition<'db>,
    ) -> Result<Option<Definition<'db>>, Infallible> {
        previous_sync(builder, definition, HeaderFacts, self)
    }
}
