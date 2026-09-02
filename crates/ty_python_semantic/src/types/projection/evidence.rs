//! Inference-time evidence for projection cycle recovery.
//!
//! Evidence records projection results that were already observed during normal
//! inference. Cycle recovery reuses these facts instead of calling inference
//! queries while solving projection equations.
//!
//! Evidence must also be collected during normal inference. Replaying a
//! projection path can invoke another inference query, for example while
//! resolving a class decorator. Doing that from a Salsa cycle-recovery callback
//! introduces a new dependency into the active cycle, which Salsa rejects.

use rustc_hash::FxHashMap;

use crate::types::{DivergentType, ProgramEnvironment, StaticClassLiteral, Type};
use crate::{Db, FxIndexSet};

use super::artifact::{ProjectionOp, ProjectionPath};
use super::container::ProjectionContainer;
use super::term::ProjectionTerm;

/// Projection facts computed during normal inference and reused during cycle recovery.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, salsa::SalsaValue)]
pub(crate) struct ProjectionEvidenceSet<'db>(ProjectionEvidenceSetInterned<'db>);

// The Salsa heap is tracked separately.
impl get_size2::GetSize for ProjectionEvidenceSet<'_> {}

/// Mutable inference-time accumulator for projection evidence.
#[derive(Debug, Clone, Default)]
pub(super) struct ProjectionEvidenceBuilder<'db> {
    projection_facts: FxIndexSet<ProjectionEvidenceFact<'db>>,
    container_facts: FxIndexSet<ProjectionContainerFact<'db>>,
    // Share operation results across paths only within this collection: cycle approximations can
    // change between inference runs. Facts are still recorded separately for each root and path.
    inferred_operations: FxHashMap<(Type<'db>, ProjectionOp<'db>), Option<ProjectionTerm<'db>>>,
}

impl<'db> ProjectionEvidenceBuilder<'db> {
    /// Inference-time API: records facts needed by projection cycle recovery.
    fn extend_from_types(
        &mut self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        types: impl IntoIterator<Item = Type<'db>>,
    ) {
        for ty in types {
            let demands = ty.projection_demands(db, env);
            for (root, path) in demands {
                self.record_projection_path(db, env, root, ty, &path);
            }
        }
    }

    fn record_projection_path(
        &mut self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        root: DivergentType,
        ty: Type<'db>,
        path: &ProjectionPath<'db>,
    ) -> Option<ProjectionTerm<'db>> {
        if let Type::Union(union) = ty {
            let mut terms = Vec::new();
            let mut all_arms_projected = true;
            for element in union.elements(db) {
                if let Some(term) = self.record_projection_path(db, env, root, *element, path) {
                    terms.push(term);
                } else {
                    all_arms_projected = false;
                }
            }

            // Evidence remains useful for arms that projected successfully; the union result is
            // valid only when every arm supports the operation.
            return all_arms_projected
                .then(|| ProjectionTerm::from_union_terms(db, env, &terms))
                .flatten();
        }

        let ops = path.ops();
        let (&op, tail) = ops.split_first()?;
        let projected = *self
            .inferred_operations
            .entry((ty, op))
            .or_insert_with(|| ProjectionContainer::infer_projection_op(db, env, ty, op));
        let projected = projected?;
        let term = if tail.is_empty() {
            projected
        } else {
            self.record_projection_term_path(db, env, root, projected, tail)?
        };

        self.record_inferred_projection_fact(db, env, root, ty, path, term);
        Some(term)
    }

    // Follow the demanded path once and record generic containers encountered along it. This
    // avoids trying every suffix of every nested container.
    fn record_projection_term_path(
        &mut self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        root: DivergentType,
        term: ProjectionTerm<'db>,
        path: &[ProjectionOp<'db>],
    ) -> Option<ProjectionTerm<'db>> {
        let (&op, tail) = path.split_first()?;
        let projected = match term {
            ProjectionTerm::List(element) => {
                ProjectionContainer::project_list_op(db, env, element, op)?
            }
            _ => {
                return self.record_projection_path(
                    db,
                    env,
                    root,
                    term.ty(db, env),
                    &ProjectionPath::from_ops(path.iter().copied()),
                );
            }
        };

        if tail.is_empty() {
            return Some(projected);
        }

        self.record_projection_term_path(db, env, root, projected, tail)
    }

    fn record_inferred_projection_fact(
        &mut self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        root: DivergentType,
        arm: Type<'db>,
        path: &ProjectionPath<'db>,
        term: ProjectionTerm<'db>,
    ) {
        if term.is_ambiguous(db, env) {
            return;
        }

        if let Some(container_fact) = ProjectionContainerFact::try_from_inference_type(db, env, arm)
        {
            self.push_container_fact(container_fact);
            self.push_projection_fact(ProjectionEvidenceFact {
                root,
                arm,
                path: path.clone(),
                term,
            });
        }
    }

    /// Inference-time API: records the observed result of projecting a non-cycle arm.
    pub(super) fn record_projected_arm(
        &mut self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        roots: impl IntoIterator<Item = DivergentType>,
        arm: Type<'db>,
        path: &ProjectionPath<'db>,
        term: ProjectionTerm<'db>,
    ) {
        if term.is_ambiguous(db, env) {
            return;
        }

        if let Some(container_fact) = ProjectionContainerFact::try_from_inference_type(db, env, arm)
        {
            self.push_container_fact(container_fact);
        }
        for root in roots {
            self.push_projection_fact(ProjectionEvidenceFact {
                root,
                arm,
                path: path.clone(),
                term,
            });
        }
    }

    fn push_projection_fact(&mut self, fact: ProjectionEvidenceFact<'db>) {
        self.projection_facts.insert(fact);
    }

    fn push_container_fact(&mut self, fact: ProjectionContainerFact<'db>) {
        self.container_facts.insert(fact);
    }

    pub(super) fn finish(self, db: &'db dyn Db) -> Option<ProjectionEvidenceSet<'db>> {
        ProjectionEvidenceSet::new(db, self.projection_facts, self.container_facts)
    }
}

impl<'db> ProjectionEvidenceSet<'db> {
    /// Inference-time API: eagerly collects projection evidence for later cycle recovery.
    ///
    /// Use this when the projection demand can be introduced after the inference result is
    /// produced, so the result cannot know ahead of time whether evidence will be needed.
    /// Do not call this from cycle recovery: collecting evidence may invoke inference queries.
    pub(crate) fn from_types(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        types: impl IntoIterator<Item = Type<'db>>,
    ) -> Option<Self> {
        let mut builder = ProjectionEvidenceBuilder::default();
        builder.extend_from_types(db, env, types);
        builder.finish(db)
    }

    /// Inference-time API: conditionally collects projection evidence.
    ///
    /// Use this only when every projection demand that may need facts from these types has already
    /// been observed before the inference result is produced. `should_collect == false` is a
    /// promise that the produced types contain no projection demands, so the negative path does
    /// not walk them. If an external consumer can later introduce a new demand for the produced
    /// result, use [`ProjectionEvidenceSet::from_types`] instead.
    /// Do not call this from cycle recovery: collecting evidence may invoke inference queries.
    pub(crate) fn from_types_if_needed(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        should_collect: bool,
        types: impl IntoIterator<Item = Type<'db>>,
    ) -> Option<Self> {
        if !should_collect {
            return None;
        }

        Self::from_types(db, env, types)
    }

    pub(crate) fn merged(
        db: &'db dyn Db,
        current: Option<Self>,
        previous: Option<Self>,
    ) -> Option<Self> {
        match (current, previous) {
            (None, None) => None,
            (Some(evidence), None) | (None, Some(evidence)) => Some(evidence),
            (Some(current), Some(previous)) if current == previous => Some(current),
            (Some(current), Some(previous)) => {
                let mut projection_evidence = ProjectionEvidenceBuilder::default();
                for fact in current
                    .projection_facts(db)
                    .iter()
                    .chain(previous.projection_facts(db))
                    .cloned()
                {
                    projection_evidence.push_projection_fact(fact);
                }

                for fact in current
                    .container_facts(db)
                    .iter()
                    .chain(previous.container_facts(db))
                    .cloned()
                {
                    projection_evidence.push_container_fact(fact);
                }

                projection_evidence.finish(db)
            }
        }
    }

    fn new(
        db: &'db dyn Db,
        projection_facts: FxIndexSet<ProjectionEvidenceFact<'db>>,
        container_facts: FxIndexSet<ProjectionContainerFact<'db>>,
    ) -> Option<Self> {
        (!projection_facts.is_empty() || !container_facts.is_empty()).then(|| {
            Self(ProjectionEvidenceSetInterned::new(
                db,
                projection_facts
                    .into_iter()
                    .collect::<Vec<_>>()
                    .into_boxed_slice(),
                container_facts
                    .into_iter()
                    .collect::<Vec<_>>()
                    .into_boxed_slice(),
            ))
        })
    }

    pub(super) fn projection_facts(self, db: &'db dyn Db) -> &'db [ProjectionEvidenceFact<'db>] {
        self.0.projection_facts(db)
    }

    fn container_facts(self, db: &'db dyn Db) -> &'db [ProjectionContainerFact<'db>] {
        self.0.container_facts(db)
    }

    /// Cycle-recovery-time API: looks up a previously collected container fact.
    pub(super) fn container_fact_for_arm(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        root: DivergentType,
        arm: Type<'db>,
    ) -> Option<&'db ProjectionContainerFact<'db>> {
        let normalized_arm = arm
            .replace_projection_artifacts_with_root(db, env, root)
            .unwrap_or(arm);
        self.container_facts(db).iter().find(|fact| {
            if fact.arm == arm {
                return true;
            }

            let fact_arm = fact
                .arm
                .replace_projection_artifacts_with_root(db, env, root)
                .unwrap_or(fact.arm);
            fact_arm == normalized_arm
        })
    }

    /// Cycle-recovery-time API: replays a projection from inference-time evidence.
    pub(super) fn project_arm_path(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        root: DivergentType,
        arm: Type<'db>,
        path: &ProjectionPath<'db>,
    ) -> Option<ProjectionTerm<'db>> {
        let normalized_arm = arm
            .replace_projection_artifacts_with_root(db, env, root)
            .unwrap_or(arm);
        self.projection_facts(db).iter().find_map(|fact| {
            if !fact.root.same_marker(root) || fact.path != *path {
                return None;
            }
            if fact.arm == arm {
                return Some(fact.term);
            }

            let fact_arm = fact
                .arm
                .replace_projection_artifacts_with_root(db, env, root)
                .unwrap_or(fact.arm);
            (fact_arm == normalized_arm).then_some(fact.term)
        })
    }
}

/// Interned storage for [`ProjectionEvidenceSet`].
#[salsa::interned(debug, heap_size=ruff_memory_usage::heap_size)]
struct ProjectionEvidenceSetInterned<'db> {
    #[returns(deref)]
    projection_facts: Box<[ProjectionEvidenceFact<'db>]>,
    #[returns(deref)]
    container_facts: Box<[ProjectionContainerFact<'db>]>,
}

// The Salsa heap is tracked separately.
impl get_size2::GetSize for ProjectionEvidenceSetInterned<'_> {}

/// The result of projecting one non-cycle arm during inference.
#[derive(Debug, Clone, PartialEq, Eq, Hash, salsa::SalsaValue, get_size2::GetSize)]
pub(super) struct ProjectionEvidenceFact<'db> {
    pub(super) root: DivergentType,
    pub(super) arm: Type<'db>,
    pub(super) path: ProjectionPath<'db>,
    pub(super) term: ProjectionTerm<'db>,
}

/// A generic container specialization computed during inference.
#[derive(Debug, Clone, PartialEq, Eq, Hash, salsa::SalsaValue, get_size2::GetSize)]
pub(super) struct ProjectionContainerFact<'db> {
    pub(super) arm: Type<'db>,
    pub(super) class: StaticClassLiteral<'db>,
    pub(super) arguments: Box<[Type<'db>]>,
}

impl<'db> ProjectionContainerFact<'db> {
    fn try_from_parts(
        arm: Type<'db>,
        class: StaticClassLiteral<'db>,
        arguments: &[Type<'db>],
    ) -> Option<Self> {
        (!arguments.is_empty()).then(|| Self {
            arm,
            class,
            arguments: arguments.to_vec().into_boxed_slice(),
        })
    }

    /// Cycle-recovery-time API: builds a fact from direct specialization only.
    pub(super) fn try_from_recovery_type(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        ty: Type<'db>,
    ) -> Option<Self> {
        if ty.exact_tuple_instance_spec(db).is_some() {
            return None;
        }

        let (class, specialization) = ty.direct_class_specialization(db, env)?;
        Self::try_from_parts(ty, class, specialization.types(db))
    }

    /// Inference-time API: builds a fact from the full specialization view.
    ///
    /// This may expand aliases, bounds, and fallbacks.
    fn try_from_inference_type(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        ty: Type<'db>,
    ) -> Option<Self> {
        if ty.exact_tuple_instance_spec(db).is_some() {
            return None;
        }

        let (class, specialization) = ty.class_specialization(db, env)?;
        Self::try_from_parts(ty, class, specialization.types(db))
    }
}
