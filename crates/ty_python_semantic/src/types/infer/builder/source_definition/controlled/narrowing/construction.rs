//! Admitted storage and traversal for the shared narrowing graph constructor.

use std::alloc::Layout;

use salsa::execution_probe::{ExecutionWork, RunError, RunResult, TaskEndpoint};
use smallvec::SmallVec;
use ty_python_core::Truthiness;
use ty_python_core::narrowing_constraints::{InteriorNode, ScopedNarrowingConstraint};
use ty_python_core::predicate::{Predicate, PredicateNode, ScopedPredicateId};

use super::super::storage::{StorageQuote, sequence_merge, slots, table_merge};
use super::{SourceAccess, SourceEffects, SourceOperation};
use crate::reachability::narrowing_construction::{
    Construction, Frame, NarrowingConstructionEffects, NarrowingConstructionFacts, build_with,
};
use crate::reachability::narrowing_predicate::{
    NarrowingPredicateEffects, PredicateConstraints, predicate_constraints_with,
};
use crate::reachability::source::{self, ReachabilityFacts};
use crate::reachability::{
    NarrowingProjector, ProjectedNarrowingCheckpoint, ProjectedNarrowingEntry,
    ProjectedNarrowingGraph, ProjectedNarrowingNode, ProjectedNarrowingNodeId,
};
use crate::types::narrow::admission::{clone_constraint, selected_constraint};
use crate::types::{NarrowingConstraint, Type};

type ProjectionKey<'db> = (ScopedNarrowingConstraint, Type<'db>);
type OrKey = (ProjectedNarrowingNodeId, ProjectedNarrowingNodeId);

pub(super) fn checked<T>(value: Option<T>) -> RunResult<T> {
    value.ok_or(RunError::Contract("narrowing storage quotation overflow"))
}

pub(super) fn admit(endpoint: &TaskEndpoint<'_, '_>, quote: StorageQuote) -> RunResult<()> {
    endpoint.admit_work(quote.work)?;
    if quote.bytes != 0 {
        endpoint.admit(ExecutionWork::Resource {
            requested_bytes: quote.bytes,
        })?;
    }
    endpoint.check_completion()
}

fn key_work<K>(inline_bytes: usize) -> Option<usize> {
    size_of::<K>()
        .checked_mul(2)?
        .checked_add(inline_bytes.checked_mul(2)?)?
        .checked_add(16)
}

pub(super) fn lookup_quote<K>(
    capacity: usize,
    retained_slots: usize,
    inline_bytes: usize,
) -> Option<StorageQuote> {
    let backing = slots(capacity)?.max(retained_slots);
    Some(StorageQuote {
        work: backing
            .checked_add(1)?
            .checked_mul(key_work::<K>(inline_bytes)?)?,
        bytes: 0,
    })
}

pub(super) fn insertion_quote<K, V>(
    len: usize,
    capacity: usize,
    retained_slots: usize,
    inline_bytes: usize,
) -> Option<(StorageQuote, usize)> {
    // HashMap can reserve before discovering that an insertion replaces an existing key.
    let (mut quote, backing) = table_merge::<(K, V)>(len, capacity, 1, retained_slots)?;
    let key_work = key_work::<K>(inline_bytes)?;
    let entry_work = size_of::<(K, V)>().checked_mul(3)?.checked_add(8)?;
    quote.work = quote
        .work
        .checked_add(backing.checked_add(1)?.checked_mul(key_work)?)?
        .checked_add(entry_work)?;
    if len.checked_add(1)? > capacity {
        // Rehashing hashes every retained key and can probe the entire replacement table for
        // each entry. The table's empty-slot search does not compare keys with one another.
        quote.work = quote
            .work
            .checked_add(len.checked_mul(backing.checked_add(key_work)?.checked_add(entry_work)?)?)?
            .checked_add(quote.bytes.checked_mul(2)?)?;
        Layout::from_size_align(quote.bytes, align_of::<(K, V)>().max(16)).ok()?;
    }
    Some((quote, backing))
}

pub(super) fn sequence_quote<T>(len: usize, capacity: usize) -> Option<(StorageQuote, usize)> {
    let mut quote = sequence_merge::<T>(len, capacity, 1)?;
    quote.work = quote.work.checked_add(size_of::<T>().checked_mul(2)?)?;
    let additional = if quote.bytes == 0 {
        0
    } else {
        let requested = quote.bytes.checked_div(size_of::<T>())?;
        Layout::array::<T>(requested).ok()?;
        quote.work = quote
            .work
            .checked_add(capacity.checked_mul(size_of::<T>())?)?
            .checked_add(quote.bytes.checked_mul(2)?)?;
        requested.checked_sub(len)?
    };
    Some((quote, additional))
}

fn graph_append_quote(graph: &ProjectedNarrowingGraph<'_>) -> Option<(StorageQuote, [usize; 3])> {
    if graph.nodes.len() >= ProjectedNarrowingNodeId::ALWAYS_FALSE.0
        || graph.nodes.len() != graph.referenced.len()
        || graph.nodes.len() != graph.joins.len()
    {
        return None;
    }
    let (nodes, nodes_reserve) =
        sequence_quote::<ProjectedNarrowingEntry<'_>>(graph.nodes.len(), graph.nodes.capacity())?;
    let (references, references_reserve) =
        sequence_quote::<bool>(graph.referenced.len(), graph.referenced.capacity())?;
    let (joins, joins_reserve) = sequence_quote::<bool>(graph.joins.len(), graph.joins.capacity())?;
    Some((
        nodes.checked_add(references)?.checked_add(joins)?,
        [nodes_reserve, references_reserve, joins_reserve],
    ))
}

fn take_frame<'db>(frame: &mut Frame<'db>) -> Frame<'db> {
    // Keep payload ownership in the calling future until admission succeeds. The replacement
    // frame has no owned storage and is never scheduled.
    std::mem::replace(
        frame,
        Frame::Or(
            ProjectedNarrowingNodeId::ALWAYS_FALSE,
            ProjectedNarrowingNodeId::ALWAYS_FALSE,
        ),
    )
}

pub(super) async fn clone_predicate_constraints<'db>(
    endpoint: &TaskEndpoint<'_, 'db>,
    constraints: &PredicateConstraints<'db>,
) -> RunResult<PredicateConstraints<'db>> {
    endpoint
        .local_call(|| {
            let work = checked(size_of::<PredicateConstraints<'db>>().checked_mul(2))?;
            admit(endpoint, StorageQuote { work, bytes: 0 })
        })
        .await;
    let positive = match &constraints.0 {
        Some(constraint) => Some(clone_constraint(endpoint, constraint).await?),
        None => None,
    };
    let negative = match &constraints.1 {
        Some(constraint) => Some(clone_constraint(endpoint, constraint).await?),
        None => None,
    };
    Ok((positive, negative))
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> SourceEffects<'_, 'run, 'db, A> {
    pub(in crate::types::infer) async fn build_narrowing_graph(
        &self,
        projector: &mut NarrowingProjector<'_, 'db>,
        mut initial: Frame<'db>,
    ) -> RunResult<ProjectedNarrowingNodeId> {
        self.allocate_future(|| {
            build_with(
                projector,
                take_frame(&mut initial),
                NarrowingConstructionFacts,
                self,
            )
        })
        .await?
        .await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> NarrowingConstructionEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn start(&self, mut initial: Frame<'db>) -> RunResult<Construction<'db>> {
        let endpoint = self.access.endpoint();
        Ok(endpoint
            .local_call(|| {
                endpoint.admit_work(1)?;
                let work = checked(size_of::<Construction<'db>>().checked_mul(2))?;
                admit(endpoint, StorageQuote { work, bytes: 0 })?;
                let mut frames = SmallVec::new();
                frames.push(take_frame(&mut initial));
                Ok(Construction { frames })
            })
            .await)
    }

    async fn next(&self, construction: &mut Construction<'db>) -> RunResult<Option<Frame<'db>>> {
        self.local(size_of::<Frame<'db>>() + 1, 0, || construction.frames.pop())
            .await
    }

    async fn push(
        &self,
        construction: &mut Construction<'db>,
        mut frame: Frame<'db>,
    ) -> RunResult<()> {
        let endpoint = self.access.endpoint();
        Ok(endpoint
            .local_call(|| {
                endpoint.admit_work(1)?;
                let (quote, additional) = checked(sequence_quote::<Frame<'db>>(
                    construction.frames.len(),
                    construction.frames.capacity(),
                ))?;
                admit(endpoint, quote)?;
                construction.frames.reserve_exact(additional);
                construction.frames.push(take_frame(&mut frame));
                Ok(())
            })
            .await)
    }

    async fn finish(&self, construction: Construction<'db>) -> RunResult<()> {
        self.work(1).await?;
        drop(construction);
        Ok(())
    }

    async fn retire_predicate_constraints(
        &self,
        positive: Option<NarrowingConstraint<'db>>,
        negative: Option<NarrowingConstraint<'db>>,
    ) -> RunResult<()> {
        // The producer must prepay payload disposal before handing ownership to a continuation.
        self.work(1).await?;
        drop(negative);
        drop(positive);
        Ok(())
    }

    async fn has_projection(
        &self,
        projector: &NarrowingProjector<'_, 'db>,
        id: ScopedNarrowingConstraint,
    ) -> RunResult<bool> {
        let endpoint = self.access.endpoint();
        Ok(endpoint
            .local_call(|| {
                endpoint.admit_work(1)?;
                let quote = checked(lookup_quote::<ProjectionKey<'db>>(
                    projector.project_cache.capacity(),
                    projector.source_project_backing,
                    projector
                        .source_project_key_bytes
                        .max(projector.base_ty.inline_payload_bytes()),
                ))?;
                admit(endpoint, quote)?;
                Ok(projector
                    .project_cache
                    .contains_key(&(id, projector.base_ty)))
            })
            .await)
    }

    async fn cached_projection(
        &self,
        projector: &NarrowingProjector<'_, 'db>,
        id: ScopedNarrowingConstraint,
    ) -> RunResult<Option<ProjectedNarrowingNodeId>> {
        let endpoint = self.access.endpoint();
        Ok(endpoint
            .local_call(|| {
                endpoint.admit_work(1)?;
                let quote = checked(lookup_quote::<ProjectionKey<'db>>(
                    projector.project_cache.capacity(),
                    projector.source_project_backing,
                    projector
                        .source_project_key_bytes
                        .max(projector.base_ty.inline_payload_bytes()),
                ))?;
                admit(endpoint, quote)?;
                Ok(projector
                    .project_cache
                    .get(&(id, projector.base_ty))
                    .copied())
            })
            .await)
    }

    async fn projected_node(
        &self,
        projector: &NarrowingProjector<'_, 'db>,
        id: ScopedNarrowingConstraint,
    ) -> RunResult<ProjectedNarrowingNodeId> {
        let endpoint = self.access.endpoint();
        Ok(endpoint
            .local_call(|| {
                endpoint.admit_work(1)?;
                let quote = if id.is_terminal() {
                    StorageQuote { work: 1, bytes: 0 }
                } else {
                    checked(lookup_quote::<ProjectionKey<'db>>(
                        projector.project_cache.capacity(),
                        projector.source_project_backing,
                        projector
                            .source_project_key_bytes
                            .max(projector.base_ty.inline_payload_bytes()),
                    ))?
                };
                admit(endpoint, quote)?;
                Ok(projector.projected_node(id))
            })
            .await)
    }

    async fn publish_projection(
        &self,
        projector: &mut NarrowingProjector<'_, 'db>,
        id: ScopedNarrowingConstraint,
        projected: ProjectedNarrowingNodeId,
    ) -> RunResult<()> {
        let endpoint = self.access.endpoint();
        Ok(endpoint
            .local_call(|| {
                endpoint.admit_work(1)?;
                let previous_backing = checked(slots(projector.project_cache.capacity()))?
                    .max(projector.source_project_backing);
                let key_bytes = projector
                    .source_project_key_bytes
                    .max(projector.base_ty.inline_payload_bytes());
                let (quote, backing) = checked(insertion_quote::<
                    ProjectionKey<'db>,
                    ProjectedNarrowingNodeId,
                >(
                    projector.project_cache.len(),
                    projector.project_cache.capacity(),
                    previous_backing,
                    key_bytes,
                ))?;
                admit(endpoint, quote)?;
                projector.project_cache.reserve(1);
                projector
                    .project_cache
                    .insert((id, projector.base_ty), projected);
                projector.source_project_backing = slots(projector.project_cache.capacity())
                    .map_or(backing, |observed| previous_backing.max(observed));
                projector.source_project_key_bytes = key_bytes;
                Ok(())
            })
            .await)
    }

    async fn remove_projection(
        &self,
        projector: &mut NarrowingProjector<'_, 'db>,
        id: ScopedNarrowingConstraint,
    ) -> RunResult<()> {
        let endpoint = self.access.endpoint();
        Ok(endpoint
            .local_call(|| {
                endpoint.admit_work(1)?;
                let backing = checked(slots(projector.project_cache.capacity()))?
                    .max(projector.source_project_backing);
                let quote = checked(lookup_quote::<ProjectionKey<'db>>(
                    projector.project_cache.capacity(),
                    backing,
                    projector
                        .source_project_key_bytes
                        .max(projector.base_ty.inline_payload_bytes()),
                ))?;
                admit(endpoint, quote)?;
                projector.project_cache.remove(&(id, projector.base_ty));
                projector.source_project_backing = backing;
                Ok(())
            })
            .await)
    }

    async fn constraint_node(
        &self,
        projector: &NarrowingProjector<'_, 'db>,
        id: ScopedNarrowingConstraint,
    ) -> RunResult<InteriorNode> {
        self.local(2, 0, || projector.constraints.get_interior_node(id))
            .await
    }

    async fn predicate(
        &self,
        projector: &NarrowingProjector<'_, 'db>,
        id: ScopedPredicateId,
    ) -> RunResult<Predicate<'db>> {
        self.local(1, 0, || projector.predicates[id]).await
    }

    async fn targets_place(
        &self,
        projector: &NarrowingProjector<'_, 'db>,
        id: ScopedPredicateId,
    ) -> RunResult<bool> {
        let endpoint = self.access.endpoint();
        Ok(endpoint
            .local_call(|| {
                endpoint.admit_work(1)?;
                let work = checked(
                    projector
                        .predicate_narrowing_targets
                        .contains_place_work()
                        .checked_mul(2),
                )?;
                admit(endpoint, StorageQuote { work, bytes: 0 })?;
                Ok(projector
                    .predicate_narrowing_targets
                    .contains(id, projector.place))
            })
            .await)
    }

    async fn graph_node(
        &self,
        projector: &NarrowingProjector<'_, 'db>,
        id: ProjectedNarrowingNodeId,
    ) -> RunResult<ProjectedNarrowingEntry<'db>> {
        self.local(1, 0, || projector.graph.node(id)).await
    }

    async fn cached_or(
        &self,
        projector: &NarrowingProjector<'_, 'db>,
        key: OrKey,
    ) -> RunResult<Option<ProjectedNarrowingNodeId>> {
        let endpoint = self.access.endpoint();
        Ok(endpoint
            .local_call(|| {
                endpoint.admit_work(1)?;
                let quote = checked(lookup_quote::<OrKey>(
                    projector.graph.or_cache.capacity(),
                    0,
                    0,
                ))?;
                admit(endpoint, quote)?;
                Ok(projector.graph.or_cache.get(&key).copied())
            })
            .await)
    }

    async fn publish_or(
        &self,
        projector: &mut NarrowingProjector<'_, 'db>,
        key: OrKey,
        projected: ProjectedNarrowingNodeId,
    ) -> RunResult<()> {
        let endpoint = self.access.endpoint();
        Ok(endpoint
            .local_call(|| {
                endpoint.admit_work(1)?;
                let (quote, _) = checked(insertion_quote::<OrKey, ProjectedNarrowingNodeId>(
                    projector.graph.or_cache.len(),
                    projector.graph.or_cache.capacity(),
                    0,
                    0,
                ))?;
                admit(endpoint, quote)?;
                projector.graph.or_cache.reserve(1);
                projector.graph.or_cache.insert(key, projected);
                Ok(())
            })
            .await)
    }

    async fn cached_node(
        &self,
        projector: &NarrowingProjector<'_, 'db>,
        node: ProjectedNarrowingNode,
    ) -> RunResult<Option<ProjectedNarrowingNodeId>> {
        let endpoint = self.access.endpoint();
        Ok(endpoint
            .local_call(|| {
                endpoint.admit_work(1)?;
                let quote = checked(lookup_quote::<ProjectedNarrowingNode>(
                    projector.graph.node_cache.capacity(),
                    0,
                    0,
                ))?;
                admit(endpoint, quote)?;
                Ok(projector.graph.node_cache.get(&node).copied())
            })
            .await)
    }

    async fn append_checkpoint(
        &self,
        projector: &mut NarrowingProjector<'_, 'db>,
        constraint: ScopedNarrowingConstraint,
        ty: Type<'db>,
    ) -> RunResult<ProjectedNarrowingNodeId> {
        let endpoint = self.access.endpoint();
        Ok(endpoint
            .local_call(|| {
                endpoint.admit_work(1)?;
                let (quote, reserve) = checked(graph_append_quote(&projector.graph))?;
                admit(endpoint, quote)?;
                projector.graph.nodes.reserve_exact(reserve[0]);
                projector.graph.referenced.reserve_exact(reserve[1]);
                projector.graph.joins.reserve_exact(reserve[2]);
                let id = ProjectedNarrowingNodeId(projector.graph.nodes.len());
                projector
                    .graph
                    .nodes
                    .push(ProjectedNarrowingEntry::Checkpoint { constraint, ty });
                projector.graph.referenced.push(false);
                projector.graph.joins.push(false);
                Ok(id)
            })
            .await)
    }

    async fn append_predicate(
        &self,
        projector: &mut NarrowingProjector<'_, 'db>,
        node: ProjectedNarrowingNode,
    ) -> RunResult<ProjectedNarrowingNodeId> {
        let endpoint = self.access.endpoint();
        Ok(endpoint
            .local_call(|| {
                endpoint.admit_work(1)?;
                let (quote, reserve) = checked(graph_append_quote(&projector.graph))?;
                let (cache, _) = checked(insertion_quote::<
                    ProjectedNarrowingNode,
                    ProjectedNarrowingNodeId,
                >(
                    projector.graph.node_cache.len(),
                    projector.graph.node_cache.capacity(),
                    0,
                    0,
                ))?;
                let quote = checked(
                    quote
                        .checked_add(cache)
                        .and_then(|quote| quote.checked_add(StorageQuote { work: 9, bytes: 0 })),
                )?;
                admit(endpoint, quote)?;
                projector.graph.nodes.reserve_exact(reserve[0]);
                projector.graph.referenced.reserve_exact(reserve[1]);
                projector.graph.joins.reserve_exact(reserve[2]);
                projector.graph.node_cache.reserve(1);
                let id = ProjectedNarrowingNodeId(projector.graph.nodes.len());
                projector
                    .graph
                    .nodes
                    .push(ProjectedNarrowingEntry::Predicate(node));
                projector.graph.referenced.push(false);
                projector.graph.joins.push(false);
                projector.graph.node_cache.insert(node, id);
                for next in [node.if_true, node.if_uncertain, node.if_false] {
                    projector.graph.record_reference(next);
                }
                Ok(id)
            })
            .await)
    }

    async fn checkpoint(
        &self,
        _projector: &NarrowingProjector<'_, 'db>,
        _predicate: Predicate<'db>,
        _constraint: ScopedNarrowingConstraint,
    ) -> RunResult<ProjectedNarrowingCheckpoint<'db>> {
        self.unavailable(SourceOperation::Narrowing).await
    }

    async fn analyze(
        &self,
        projector: &NarrowingProjector<'_, 'db>,
        predicate: Predicate<'db>,
    ) -> RunResult<Truthiness> {
        source::analyze_single_with(projector.env, &predicate, ReachabilityFacts, self).await
    }

    async fn predicate_constraints(
        &self,
        projector: &mut NarrowingProjector<'_, 'db>,
        id: ScopedPredicateId,
    ) -> RunResult<(
        Option<NarrowingConstraint<'db>>,
        Option<NarrowingConstraint<'db>>,
    )> {
        predicate_constraints_with(projector, id, self).await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> NarrowingPredicateEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn targets_place(
        &self,
        projector: &NarrowingProjector<'_, 'db>,
        id: ScopedPredicateId,
    ) -> RunResult<bool> {
        NarrowingConstructionEffects::targets_place(self, projector, id).await
    }

    async fn cached_constraints(
        &self,
        projector: &NarrowingProjector<'_, 'db>,
        id: ScopedPredicateId,
    ) -> RunResult<Option<PredicateConstraints<'db>>> {
        let endpoint = self.access.endpoint();
        let cached = endpoint
            .local_call(|| {
                endpoint.admit_work(1)?;
                let quote = checked(lookup_quote::<ScopedPredicateId>(
                    projector.graph.predicate_constraints_cache.capacity(),
                    0,
                    0,
                ))?;
                admit(endpoint, quote)?;
                Ok(projector.graph.predicate_constraints_cache.get(&id))
            })
            .await;
        match cached {
            Some(cached) => Ok(Some(clone_predicate_constraints(endpoint, cached).await?)),
            None => Ok(None),
        }
    }

    async fn infer_constraints(
        &self,
        projector: &NarrowingProjector<'_, 'db>,
        id: ScopedPredicateId,
    ) -> RunResult<PredicateConstraints<'db>> {
        let (predicate, place) = self
            .local(3, 0, || (projector.predicates[id], projector.place))
            .await?;
        let constraints = match predicate.node {
            PredicateNode::Expression(expression)
            | PredicateNode::Condition(expression)
            | PredicateNode::ChainedComparisonCondition(expression) => {
                let constraints = self
                    .access
                    .expression_narrowing_constraints(expression)
                    .await?;
                let endpoint = self.access.endpoint();
                let positive = selected_constraint(endpoint, constraints, place, true).await?;
                let negative = selected_constraint(endpoint, constraints, place, false).await?;
                (positive, negative)
            }
            PredicateNode::Pattern(_) | PredicateNode::SubjectElementPattern(_) => {
                return self.unavailable(SourceOperation::Narrowing).await;
            }
            PredicateNode::ContextManagerSuppresses { .. }
            | PredicateNode::FinallyNormalPathImpossible { .. }
            | PredicateNode::IsNonTerminalCall(_)
            | PredicateNode::IsNonEmptyIterable(_)
            | PredicateNode::OrPatternAlternative(_)
            | PredicateNode::StarImportPlaceholder(_) => (None, None),
        };

        if predicate.is_positive {
            Ok(constraints)
        } else {
            Ok((constraints.1, constraints.0))
        }
    }

    async fn cache_constraints(
        &self,
        projector: &mut NarrowingProjector<'_, 'db>,
        id: ScopedPredicateId,
        constraints: &PredicateConstraints<'db>,
    ) -> RunResult<()> {
        let endpoint = self.access.endpoint();
        let mut retained = clone_predicate_constraints(endpoint, constraints).await?;
        Ok(endpoint
            .local_call(|| {
                endpoint.admit_work(1)?;
                let (quote, _) = checked(insertion_quote::<
                    ScopedPredicateId,
                    PredicateConstraints<'db>,
                >(
                    projector.graph.predicate_constraints_cache.len(),
                    projector.graph.predicate_constraints_cache.capacity(),
                    0,
                    0,
                ))?;
                admit(endpoint, quote)?;
                projector
                    .graph
                    .predicate_constraints_cache
                    .insert(id, std::mem::replace(&mut retained, (None, None)));
                Ok(())
            })
            .await)
    }
}
