//! Tuple context selection and child inference shared by ordinary and suspended execution.

use std::borrow::Cow;
use std::convert::Infallible;
use std::slice;
use std::vec;

use ruff_python_ast as ast;
use ty_mapping_probe_macros::shared_semantic_family;

use super::TypeInferenceBuilder;
use super::local::tuple_expression::{Action, Phase, Prepared, State};
use super::local::{self, BuilderId, BuilderStore};
use crate::types::generics::Specialization;
use crate::types::tuple::{Tuple, TupleLength, TupleSpec, TupleType, VariableSegment};
use crate::types::unpacker::{sequence_from_literal_elements, tuple_literal_needs_promotion};
use crate::types::{KnownClass, Type, TypeContext};

pub(in crate::types::infer::builder) struct TupleExpressionFacts;
pub(super) struct OrdinaryTupleExpressionEffects;

shared_semantic_family! {
    #[synchronous(SynchronousTupleExpressionEffects)]
    pub(in crate::types::infer::builder) trait TupleExpressionEffects<'db, 'ast> {
        type Error;
        #[operation(child)]
        async fn narrow_targets(&self, builder: &TypeInferenceBuilder<'db, 'ast>, annotation: Type<'db>) -> Result<Option<Cow<'db, [Type<'db>]>>, Self::Error>;
        #[operation(source)]
        async fn setup_cache(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>) -> Result<bool, Self::Error>;
        #[operation(source)]
        async fn teardown_cache(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn specialization(&self, builder: &TypeInferenceBuilder<'db, 'ast>, annotation: Type<'db>) -> Result<Option<Specialization<'db>>, Self::Error>;
        #[operation(child)]
        async fn assignable(&self, builder: &TypeInferenceBuilder<'db, 'ast>, inferred: Type<'db>, target: Type<'db>) -> Result<bool, Self::Error>;
        #[operation(child)]
        async fn infer_impl(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, tuple: &ast::ExprTuple, context: TypeContext<'db>) -> Result<Type<'db>, Self::Error>;
        #[operation(child)]
        async fn filter_annotation(&self, builder: &TypeInferenceBuilder<'db, 'ast>, annotation: Type<'db>) -> Result<Type<'db>, Self::Error>;
        #[operation(child)]
        async fn annotation_tuple(&self, builder: &TypeInferenceBuilder<'db, 'ast>, specialization: Specialization<'db>) -> Result<&'db TupleSpec<'db>, Self::Error>;
        #[operation(child)]
        async fn resize_annotation(&self, builder: &TypeInferenceBuilder<'db, 'ast>, spec: &TupleSpec<'db>, length: usize) -> Result<Option<TupleSpec<'db>>, Self::Error>;
        #[operation(local)]
        async fn elements<'expr>(&self, tuple: &'expr ast::ExprTuple) -> Result<slice::Iter<'expr, ast::Expr>, Self::Error>;
        #[operation(local)]
        #[progress]
        async fn next_element<'expr>(&self, elements: &mut slice::Iter<'expr, ast::Expr>) -> Result<Option<&'expr ast::Expr>, Self::Error>;
        #[operation(child)]
        async fn annotation_elements(&self, builder: &TypeInferenceBuilder<'db, 'ast>, tuple: &TupleSpec<'db>) -> Result<vec::IntoIter<Type<'db>>, Self::Error>;
        #[operation(local)]
        async fn empty_annotation_elements(&self) -> Result<vec::IntoIter<Type<'db>>, Self::Error>;
        #[operation(child)]
        async fn iterable_context(&self, builder: &TypeInferenceBuilder<'db, 'ast>, element: Type<'db>) -> Result<Type<'db>, Self::Error>;
        #[operation(local)]
        async fn initialize_targets<'expr>(&self, state: &mut State<'db, 'expr>, targets: Option<Cow<'db, [Type<'db>]>>) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn select_phase<'expr>(&self, state: &mut State<'db, 'expr>, phase: Phase<'db>) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn cache_ready<'expr>(&self, state: &mut State<'db, 'expr>, teardown: bool) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn next_state_target<'expr>(&self, state: &mut State<'db, 'expr>) -> Result<Option<Type<'db>>, Self::Error>;
        #[operation(source)]
        async fn speculate_store(&self, builders: &mut BuilderStore<'_, 'db, 'ast>, parent: BuilderId) -> Result<BuilderId, Self::Error>;
        #[operation(source)]
        async fn finish_speculation(&self, builders: &mut BuilderStore<'_, 'db, 'ast>, parent: BuilderId, child: BuilderId, keep: bool) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn select_context<'expr>(&self, state: &mut State<'db, 'expr>, builder: BuilderId, context: TypeContext<'db>) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn prepare_context(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, tuple: &ast::ExprTuple, context: TypeContext<'db>) -> Result<Prepared<'db>, Self::Error>;
        #[operation(local)]
        async fn install_prepared<'expr>(&self, state: &mut State<'db, 'expr>, prepared: Prepared<'db>) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn next_state_element<'expr>(&self, state: &mut State<'db, 'expr>) -> Result<Option<&'expr ast::Expr>, Self::Error>;
        #[operation(local)]
        async fn state_annotation<'expr>(&self, state: &mut State<'db, 'expr>) -> Result<Option<Type<'db>>, Self::Error>;
        #[operation(source)]
        async fn needs_promotion(&self, tuple: &ast::ExprTuple) -> Result<bool, Self::Error>;
        #[operation(child)]
        async fn sequence(&self, builder: &TypeInferenceBuilder<'db, 'ast>, tuple: &ast::ExprTuple, promote: bool) -> Result<TupleSpec<'db>, Self::Error>;
        #[operation(child)]
        async fn construct(&self, builder: &TypeInferenceBuilder<'db, 'ast>, spec: &TupleSpec<'db>) -> Result<Type<'db>, Self::Error>;
    }

    #[finite_capability]
    impl TupleExpressionFacts {
        fn has_targets(&self, targets: &Option<Cow<'_, [Type<'_>]>>) -> bool { targets.is_some() }
        fn phase<'db>(&self, state: &State<'db, '_>) -> Phase<'db> { state.phase }
        fn root(&self, state: &State<'_, '_>) -> BuilderId { state.root }
        fn active(&self, state: &State<'_, '_>) -> BuilderId { state.active }
        fn is_speculative(&self, active: BuilderId, root: BuilderId) -> bool { active != root }
        fn tuple<'expr>(&self, state: &State<'_, 'expr>) -> &'expr ast::ExprTuple { state.tuple }
        fn current_context<'db>(&self, state: &State<'db, '_>) -> TypeContext<'db> { state.context }
        fn original_context<'db>(&self, state: &State<'db, '_>) -> TypeContext<'db> { state.original_context }
        fn teardown(&self, state: &State<'_, '_>) -> bool { state.teardown_cache }
        fn supplies_context(&self, state: &State<'_, '_>) -> bool { state.prepared.can_use_type_context }
        fn builder<'a, 'root, 'db, 'ast>(&self, builders: &'a BuilderStore<'root, 'db, 'ast>, id: BuilderId) -> &'a TypeInferenceBuilder<'db, 'ast> { builders.builder(id) }
        fn builder_mut<'a, 'root, 'db, 'ast>(&self, builders: &'a mut BuilderStore<'root, 'db, 'ast>, id: BuilderId) -> &'a mut TypeInferenceBuilder<'db, 'ast> { builders.get_mut(id) }
        fn annotation<'db>(&self, context: TypeContext<'db>) -> Option<Type<'db>> { context.annotation }
        fn context<'db>(&self, annotation: Option<Type<'db>>) -> TypeContext<'db> { TypeContext::new(annotation) }
        fn object<'db>(&self) -> Type<'db> { Type::object() }
        fn length(&self, tuple: &ast::ExprTuple) -> usize { tuple.elts.len() }
        fn is_starred(&self, element: &ast::Expr) -> bool { element.is_starred_expr() }
        fn homogeneous(&self, spec: &TupleSpec<'_>) -> bool {
            matches!(spec, Tuple::Variable(tuple) if tuple.prefix_elements().is_empty()
                && tuple.suffix_elements().is_empty()
                && matches!(tuple.variable(), VariableSegment::Homogeneous(_)))
        }
    }

    #[synchronous(infer_tuple_expression_sync)]
    #[capabilities(effects = TupleExpressionEffects)]
    #[passive_values()]
    pub(in crate::types::infer::builder) async fn infer_tuple_expression_with<'db, 'ast, E: TupleExpressionEffects<'db, 'ast>>(
        builder: &mut TypeInferenceBuilder<'db, 'ast>, tuple: &ast::ExprTuple, context: TypeContext<'db>, _facts: TupleExpressionFacts, effects: &E,
    ) -> Result<Type<'db>, E::Error> {
        effects.infer_impl(builder, tuple, context).await
    }

    /// Prepares and retains resized tuple element annotations before child inference.
    /// Starred elements suppress these contexts unless the annotation is homogeneous.
    #[synchronous(prepare_tuple_expression_sync)]
    #[capabilities(effects = TupleExpressionEffects, facts = TupleExpressionFacts)]
    #[passive_values(Prepared)]
    pub(in crate::types::infer::builder) async fn prepare_tuple_expression_with<'db, 'ast, E: TupleExpressionEffects<'db, 'ast>>(
        builder: &mut TypeInferenceBuilder<'db, 'ast>, tuple: &ast::ExprTuple, context: TypeContext<'db>, facts: TupleExpressionFacts, effects: &E,
    ) -> Result<Prepared<'db>, E::Error> {
        // Remove any union elements of the annotation that are unrelated to the tuple type.
        let annotation = if let Some(annotation) = facts.annotation(context) {
            Some(effects.filter_annotation(builder, annotation).await?)
        } else { None };
        let specialization = if let Some(annotation) = annotation {
            effects.specialization(builder, annotation).await?
        } else { None };
        let (is_homogeneous, annotated_tuple) = if let Some(specialization) = specialization {
            let spec = effects.annotation_tuple(builder, specialization).await?;
            let homogeneous = facts.homogeneous(spec);
            (homogeneous, effects.resize_annotation(builder, spec, facts.length(tuple)).await?)
        } else { (false, None) };

        // TODO: this is a simplification for now.
        //
        // It might be possible to use the type context where the annotation is not a pure-homogeneous
        // tuple and the actual tuple has starred elements in it. It seems complex to reason about,
        // though, and unlikely to come up much.
        #[passive_state]
        let mut can_use_type_context = true;
        if !is_homogeneous {
            let mut elements = effects.elements(tuple).await?;
            #[cursor_loop]
            while let Some(element) = effects.next_element(&mut elements).await? {
                if facts.is_starred(element) {
                    can_use_type_context = false;
                    break;
                }
            }
        }
        let annotations = if let Some(annotated_tuple) = &annotated_tuple {
            effects.annotation_elements(builder, annotated_tuple).await?
        } else {
            effects.empty_annotation_elements().await?
        };
        Ok(Prepared { _specification: annotated_tuple, annotations, can_use_type_context })

    }
    /// Advances one tuple phase; element requests return to the current inference driver.
    #[synchronous(advance_tuple_expression_sync)]
    #[capabilities(effects = TupleExpressionEffects, facts = TupleExpressionFacts)]
    #[passive_values(Action::Continue, Action::Infer, Action::Complete, Phase::Cache, Phase::Select, Phase::Prepare, Phase::Elements, Phase::Waiting, Phase::Build, Phase::Complete)]
    pub(in crate::types::infer::builder) async fn advance_tuple_expression_with<'db, 'ast, 'expr, E: TupleExpressionEffects<'db, 'ast>>(
        state: &mut State<'db, 'expr>, builders: &mut BuilderStore<'_, 'db, 'ast>, facts: TupleExpressionFacts, effects: &E,
    ) -> Result<Action<'db, 'expr>, E::Error> {
        let root = facts.root(state);
        let active = facts.active(state);
        let tuple = facts.tuple(state);
        match facts.phase(state) {
            Phase::Start => {
                // TypeContext::narrow_targets stops at an absent annotation before examining unions.
                let targets = if let Some(annotation) = facts.annotation(facts.original_context(state)) {
                    effects.narrow_targets(facts.builder(builders, root), annotation).await?
                } else { None };
                let phase = if facts.has_targets(&targets) { Phase::Cache } else { Phase::Prepare };
                effects.initialize_targets(state, targets).await?;
                effects.select_phase(state, phase).await?;
                Ok(Action::Continue)
            }
            Phase::Cache => {
                // Cache expressions inferred across speculative inference attempts, to avoid
                // exponential blowup.
                let teardown = effects.setup_cache(facts.builder_mut(builders, root)).await?;
                effects.cache_ready(state, teardown).await?;
                Ok(Action::Continue)
            }
            Phase::Select => {
                if let Some(target) = effects.next_state_target(state).await? {
                    if let Some(_) = effects.specialization(facts.builder(builders, root), target).await? {
                        let speculative = effects.speculate_store(builders, root).await?;
                        effects.select_context(state, speculative, facts.context(Some(target))).await?;
                    }
                } else {
                    if facts.teardown(state) {
                        effects.teardown_cache(facts.builder_mut(builders, root)).await?;
                    }
                    let context = facts.original_context(state);
                    effects.select_context(state, root, context).await?;
                }
                Ok(Action::Continue)
            }
            Phase::Prepare => {
                let context = facts.current_context(state);
                let prepared = effects.prepare_context(facts.builder_mut(builders, active), tuple, context).await?;
                effects.install_prepared(state, prepared).await?;
                Ok(Action::Continue)
            }
            Phase::Elements => {
                if let Some(expression) = effects.next_state_element(state).await? {
                    let annotation = effects.state_annotation(state).await?;
                    let context = if facts.supplies_context(state) {
                        let expected = if facts.is_starred(expression) {
                            let element = match annotation { Some(ty) => ty, None => facts.object() };
                            Some(effects.iterable_context(facts.builder(builders, active), element).await?)
                        } else { annotation };
                        facts.context(expected)
                    } else { facts.context(None) };
                    effects.select_phase(state, Phase::Waiting).await?;
                    Ok(Action::Infer { builder: active, expression, context })
                } else {
                    effects.select_phase(state, Phase::Build).await?;
                    Ok(Action::Continue)
                }
            }
            Phase::Waiting => {
                effects.select_phase(state, Phase::Elements).await?;
                Ok(Action::Continue)
            }
            Phase::Build => {
                // Infer expressions once, in evaluation order and with their type context, before
                // recovering literal positions. For `(*[(item := 1), item],)`, both list elements
                // must be inferred before the traversal reads their types.
                let promote = effects.needs_promotion(tuple).await?;
                let spec = effects.sequence(facts.builder(builders, active), tuple, promote).await?;
                let inferred = effects.construct(facts.builder(builders, active), &spec).await?;
                if facts.is_speculative(active, root) {
                    if let Some(target) = facts.annotation(facts.current_context(state)) {
                        let keep = effects.assignable(facts.builder(builders, root), inferred, target).await?;
                        effects.finish_speculation(builders, root, active, keep).await?;
                        if !keep {
                            let context = facts.original_context(state);
                            effects.select_context(state, root, context).await?;
                            effects.select_phase(state, Phase::Select).await?;
                            return Ok(Action::Continue);
                        }
                    }
                    if facts.teardown(state) {
                        effects.teardown_cache(facts.builder_mut(builders, root)).await?;
                    }
                }
                effects.select_phase(state, Phase::Complete(inferred)).await?;
                Ok(Action::Complete)
            }
            Phase::Complete(_) => Ok(Action::Complete),
        }
    }

}

impl<'db, 'ast> SynchronousTupleExpressionEffects<'db, 'ast> for OrdinaryTupleExpressionEffects {
    type Error = Infallible;

    fn narrow_targets(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        annotation: Type<'db>,
    ) -> Result<Option<Cow<'db, [Type<'db>]>>, Infallible> {
        Ok(TypeContext::new(Some(annotation))
            .narrow_targets(builder.db(), builder.program_environment()))
    }
    fn setup_cache(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
    ) -> Result<bool, Infallible> {
        Ok(builder.setup_expression_cache())
    }
    fn teardown_cache(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
    ) -> Result<(), Infallible> {
        builder.teardown_expression_cache();
        Ok(())
    }

    fn specialization(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        annotation: Type<'db>,
    ) -> Result<Option<Specialization<'db>>, Infallible> {
        Ok(annotation.known_specialization(
            builder.db(),
            builder.program_environment(),
            KnownClass::Tuple,
        ))
    }

    fn assignable(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        inferred: Type<'db>,
        target: Type<'db>,
    ) -> Result<bool, Infallible> {
        Ok(inferred.is_assignable_to(builder.db(), builder.program_environment(), target))
    }
    fn infer_impl(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        tuple: &ast::ExprTuple,
        context: TypeContext<'db>,
    ) -> Result<Type<'db>, Infallible> {
        Ok(local::tuple_value(builder, tuple, context))
    }
    fn filter_annotation(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        annotation: Type<'db>,
    ) -> Result<Type<'db>, Infallible> {
        Ok(crate::types::infer::type_context::filter_tuple_annotation(
            builder.db(),
            builder.program_environment(),
            annotation,
        ))
    }
    fn annotation_tuple(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        specialization: Specialization<'db>,
    ) -> Result<&'db TupleSpec<'db>, Infallible> {
        Ok(specialization
            .tuple(builder.db())
            .expect("the specialization of `KnownClass::Tuple` must have a tuple spec"))
    }
    fn resize_annotation(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        spec: &TupleSpec<'db>,
        length: usize,
    ) -> Result<Option<TupleSpec<'db>>, Infallible> {
        Ok(spec
            .resize(
                builder.db(),
                builder.program_environment(),
                TupleLength::Fixed(length),
            )
            .ok())
    }
    fn elements<'expr>(
        &self,
        tuple: &'expr ast::ExprTuple,
    ) -> Result<slice::Iter<'expr, ast::Expr>, Infallible> {
        Ok(tuple.elts.iter())
    }
    fn next_element<'expr>(
        &self,
        elements: &mut slice::Iter<'expr, ast::Expr>,
    ) -> Result<Option<&'expr ast::Expr>, Infallible> {
        Ok(elements.next())
    }
    fn annotation_elements(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        tuple: &TupleSpec<'db>,
    ) -> Result<vec::IntoIter<Type<'db>>, Infallible> {
        Ok(tuple
            .iter_element_types(builder.db())
            .collect::<Vec<_>>()
            .into_iter())
    }
    fn empty_annotation_elements(&self) -> Result<vec::IntoIter<Type<'db>>, Infallible> {
        Ok(Vec::new().into_iter())
    }

    fn iterable_context(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        element: Type<'db>,
    ) -> Result<Type<'db>, Infallible> {
        Ok(KnownClass::Iterable.to_specialized_instance(
            builder.db(),
            builder.program_environment(),
            &[element],
        ))
    }
    fn needs_promotion(&self, tuple: &ast::ExprTuple) -> Result<bool, Infallible> {
        Ok(tuple_literal_needs_promotion(&tuple.elts))
    }
    fn sequence(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        tuple: &ast::ExprTuple,
        promote: bool,
    ) -> Result<TupleSpec<'db>, Infallible> {
        let db = builder.db();
        let env = builder.program_environment();
        let inferred_type = |expression: &ast::Expr, promote| {
            let ty = builder.expression_type(expression);
            if promote { ty.promote(db, env) } else { ty }
        };
        Ok(sequence_from_literal_elements(
            &tuple.elts,
            promote,
            &inferred_type,
            &|expression, promote, known_length| {
                // Starred-expression inference has already reported iteration errors.
                let spec = inferred_type(expression, promote)
                    .iterate(db, env)
                    .into_owned();
                known_length
                    .and_then(|length| spec.resize(db, env, TupleLength::Fixed(length)).ok())
                    .unwrap_or(spec)
            },
            &|builder, unpacked| builder.concat(db, env, unpacked),
        ))
    }
    fn construct(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        spec: &TupleSpec<'db>,
    ) -> Result<Type<'db>, Infallible> {
        Ok(Type::tuple(TupleType::new(
            builder.db(),
            builder.program_environment(),
            spec,
        )))
    }
    fn initialize_targets<'expr>(
        &self,
        state: &mut State<'db, 'expr>,
        targets: Option<Cow<'db, [Type<'db>]>>,
    ) -> Result<(), Infallible> {
        state.targets = targets;
        Ok(())
    }
    fn select_phase<'expr>(
        &self,
        state: &mut State<'db, 'expr>,
        phase: Phase<'db>,
    ) -> Result<(), Infallible> {
        state.phase = phase;
        Ok(())
    }
    fn cache_ready<'expr>(
        &self,
        state: &mut State<'db, 'expr>,
        teardown: bool,
    ) -> Result<(), Infallible> {
        state.teardown_cache = teardown;
        state.phase = Phase::Select;
        Ok(())
    }
    fn next_state_target<'expr>(
        &self,
        state: &mut State<'db, 'expr>,
    ) -> Result<Option<Type<'db>>, Infallible> {
        let target = state
            .targets
            .as_deref()
            .and_then(|targets| targets.get(state.target_index))
            .copied();
        if target.is_some() {
            state.target_index += 1;
        }
        Ok(target)
    }
    fn speculate_store(
        &self,
        builders: &mut BuilderStore<'_, 'db, 'ast>,
        parent: BuilderId,
    ) -> Result<BuilderId, Infallible> {
        Ok(builders.speculate(parent, false))
    }
    fn finish_speculation(
        &self,
        builders: &mut BuilderStore<'_, 'db, 'ast>,
        parent: BuilderId,
        child: BuilderId,
        keep: bool,
    ) -> Result<(), Infallible> {
        let speculative = builders.take_speculative(child);
        if keep {
            builders.get_mut(parent).extend(speculative);
        }
        Ok(())
    }
    fn select_context<'expr>(
        &self,
        state: &mut State<'db, 'expr>,
        builder: BuilderId,
        context: TypeContext<'db>,
    ) -> Result<(), Infallible> {
        state.active = builder;
        state.context = context;
        state.phase = Phase::Prepare;
        Ok(())
    }
    fn prepare_context(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        tuple: &ast::ExprTuple,
        context: TypeContext<'db>,
    ) -> Result<Prepared<'db>, Infallible> {
        prepare_tuple_expression_sync(builder, tuple, context, TupleExpressionFacts, self)
    }
    fn install_prepared<'expr>(
        &self,
        state: &mut State<'db, 'expr>,
        prepared: Prepared<'db>,
    ) -> Result<(), Infallible> {
        state.prepared = prepared;
        state.remaining = &state.tuple.elts;
        state.phase = Phase::Elements;
        Ok(())
    }
    fn next_state_element<'expr>(
        &self,
        state: &mut State<'db, 'expr>,
    ) -> Result<Option<&'expr ast::Expr>, Infallible> {
        let Some((next, rest)) = state.remaining.split_first() else {
            return Ok(None);
        };
        state.remaining = rest;
        Ok(Some(next))
    }
    fn state_annotation<'expr>(
        &self,
        state: &mut State<'db, 'expr>,
    ) -> Result<Option<Type<'db>>, Infallible> {
        Ok(state.prepared.annotations.next())
    }
}
