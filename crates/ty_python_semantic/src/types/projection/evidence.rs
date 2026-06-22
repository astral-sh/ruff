//! Inference-time evidence for projection cycle recovery.
//!
//! Evidence records projection results that were already observed during normal
//! inference. Cycle recovery reuses these facts instead of calling inference
//! queries while solving projection equations.

use crate::types::{DivergentType, StaticClassLiteral, Type};
use crate::{Db, FxIndexSet};

use super::{ProjectionContainer, ProjectionOp, ProjectionPath, ProjectionTerm};

/// Projection facts computed during normal inference and reused during cycle recovery.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, salsa::Update)]
pub(crate) struct ProjectionEvidenceSet<'db>(ProjectionEvidenceSetInterned<'db>);

// The Salsa heap is tracked separately.
impl get_size2::GetSize for ProjectionEvidenceSet<'_> {}

/// Mutable inference-time accumulator for projection evidence.
#[derive(Debug, Clone, Default)]
pub(super) struct ProjectionEvidenceBuilder<'db> {
    projection_facts: FxIndexSet<ProjectionEvidenceFact<'db>>,
    container_facts: FxIndexSet<ProjectionContainerFact<'db>>,
}

impl<'db> ProjectionEvidenceBuilder<'db> {
    /// Inference-time API: records facts needed by projection cycle recovery.
    fn extend_from_types(&mut self, db: &'db dyn Db, types: impl IntoIterator<Item = Type<'db>>) {
        for ty in types {
            let demands = ty.projection_demands(db);
            for (root, path) in demands {
                self.record_projection_path(db, root, ty, &path);
            }
        }
    }

    fn record_projection_path(
        &mut self,
        db: &'db dyn Db,
        root: DivergentType,
        ty: Type<'db>,
        path: &ProjectionPath<'db>,
    ) -> Option<ProjectionTerm<'db>> {
        if let Type::Union(union) = ty {
            let mut terms = Vec::new();
            let mut all_arms_projected = true;
            for element in union.elements(db) {
                if let Some(term) = self.record_projection_path(db, root, *element, path) {
                    terms.push(term);
                } else {
                    all_arms_projected = false;
                }
            }

            // Evidence remains useful for arms that projected successfully; the union result is
            // valid only when every arm supports the operation.
            return all_arms_projected
                .then(|| ProjectionTerm::from_union_terms(db, &terms))
                .flatten();
        }

        let ops = path.ops();
        let (&op, tail) = ops.split_first()?;
        let projected = ProjectionContainer::infer_projection_op(db, ty, op)?;
        let term = if tail.is_empty() {
            projected
        } else {
            self.record_projection_term_path(db, root, projected, tail)?
        };

        self.record_inferred_projection_fact(db, root, ty, path, term);
        Some(term)
    }

    // Follow the demanded path once and record generic containers encountered along it. This
    // avoids trying every suffix of every nested container.
    fn record_projection_term_path(
        &mut self,
        db: &'db dyn Db,
        root: DivergentType,
        term: ProjectionTerm<'db>,
        path: &[ProjectionOp<'db>],
    ) -> Option<ProjectionTerm<'db>> {
        let (&op, tail) = path.split_first()?;
        let projected = match term {
            ProjectionTerm::List(element) => ProjectionContainer::project_list_op(db, element, op)?,
            _ => {
                return self.record_projection_path(
                    db,
                    root,
                    term.ty(db),
                    &ProjectionPath::from_ops(path.iter().copied()),
                );
            }
        };

        if tail.is_empty() {
            return Some(projected);
        }

        self.record_projection_term_path(db, root, projected, tail)
    }

    fn record_inferred_projection_fact(
        &mut self,
        db: &'db dyn Db,
        root: DivergentType,
        arm: Type<'db>,
        path: &ProjectionPath<'db>,
        term: ProjectionTerm<'db>,
    ) {
        if term.is_ambiguous(db) {
            return;
        }

        if let Some(container_fact) = ProjectionContainerFact::try_from_inference_type(db, arm) {
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
        roots: impl IntoIterator<Item = DivergentType>,
        arm: Type<'db>,
        path: &ProjectionPath<'db>,
        term: ProjectionTerm<'db>,
    ) {
        if term.is_ambiguous(db) {
            return;
        }

        if let Some(container_fact) = ProjectionContainerFact::try_from_inference_type(db, arm) {
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

    fn is_empty(&self) -> bool {
        self.projection_facts.is_empty() && self.container_facts.is_empty()
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
    pub(crate) fn from_types(
        db: &'db dyn Db,
        types: impl IntoIterator<Item = Type<'db>>,
    ) -> Option<Self> {
        let mut builder = ProjectionEvidenceBuilder::default();
        builder.extend_from_types(db, types);
        builder.finish(db)
    }

    /// Inference-time API: conditionally collects projection evidence.
    ///
    /// Use this only when every projection demand that may need facts from these types has already
    /// been observed before the inference result is produced. The collection still runs if the
    /// produced types already contain projection demands; `should_collect` controls only demands
    /// that may be introduced later by an external consumer. If an external consumer can later
    /// introduce a new demand for the produced result, use [`ProjectionEvidenceSet::from_types`]
    /// instead.
    pub(crate) fn from_types_if_needed(
        db: &'db dyn Db,
        should_collect: bool,
        types: impl IntoIterator<Item = Type<'db>>,
    ) -> Option<Self> {
        if should_collect {
            return Self::from_types(db, types);
        }

        let mut builder = ProjectionEvidenceBuilder::default();
        for ty in types {
            let demands = ty.projection_demands(db);
            for (root, path) in demands {
                builder.record_projection_path(db, root, ty, &path);
            }
        }

        if builder.is_empty() {
            None
        } else {
            builder.finish(db)
        }
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
        root: DivergentType,
        arm: Type<'db>,
    ) -> Option<&'db ProjectionContainerFact<'db>> {
        let normalized_arm = arm
            .replace_projection_artifacts_with_root(db, root)
            .unwrap_or(arm);
        self.container_facts(db).iter().find(|fact| {
            if fact.arm == arm {
                return true;
            }

            let fact_arm = fact
                .arm
                .replace_projection_artifacts_with_root(db, root)
                .unwrap_or(fact.arm);
            fact_arm == normalized_arm
        })
    }

    /// Cycle-recovery-time API: replays a projection from inference-time evidence.
    pub(super) fn project_arm_path(
        self,
        db: &'db dyn Db,
        root: DivergentType,
        arm: Type<'db>,
        path: &ProjectionPath<'db>,
    ) -> Option<ProjectionTerm<'db>> {
        let normalized_arm = arm
            .replace_projection_artifacts_with_root(db, root)
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
                .replace_projection_artifacts_with_root(db, root)
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
#[derive(Debug, Clone, PartialEq, Eq, Hash, salsa::Update, get_size2::GetSize)]
pub(super) struct ProjectionEvidenceFact<'db> {
    pub(super) root: DivergentType,
    pub(super) arm: Type<'db>,
    pub(super) path: ProjectionPath<'db>,
    pub(super) term: ProjectionTerm<'db>,
}

/// A generic container specialization computed during inference.
#[derive(Debug, Clone, PartialEq, Eq, Hash, salsa::Update, get_size2::GetSize)]
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
    pub(super) fn try_from_recovery_type(db: &'db dyn Db, ty: Type<'db>) -> Option<Self> {
        if ty.exact_tuple_instance_spec(db).is_some() {
            return None;
        }

        let (class, specialization) = ty.direct_class_specialization(db)?;
        Self::try_from_parts(ty, class, specialization.types(db))
    }

    /// Inference-time API: builds a fact from the full specialization view.
    ///
    /// This may expand aliases, bounds, and fallbacks.
    fn try_from_inference_type(db: &'db dyn Db, ty: Type<'db>) -> Option<Self> {
        if ty.exact_tuple_instance_spec(db).is_some() {
            return None;
        }

        let (class, specialization) = ty.class_specialization(db)?;
        Self::try_from_parts(ty, class, specialization.types(db))
    }
}
