//! Evaluation-local static MRO tasks, replies, and immutable answers.

use std::cell::Cell;
use std::rc::Rc;
use std::task::Poll;

use rustc_hash::{FxHashMap, FxHashSet};

use super::super::mapping::{MappingRootOwner, SemanticOwner, SemanticWork};
use super::super::member_lookup::{LookupFailure, LookupOperation};
use super::super::source::declarations::DeclarationKey;
use super::super::{Boundary, Effect, Key, Output, Router, Task};
use crate::types::StaticClassLiteral;
use crate::types::class_base::ClassBase;
use crate::types::generics::Specialization;
use crate::types::mro::{Mro, StaticMroError};
use crate::{Db, FxIndexSet, ProgramEnvironment};

#[cfg(test)]
mod tests;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(in crate::types::callable::scheduled_probe) struct StaticMroRequest<'db> {
    evaluation: usize,
    class: StaticClassLiteral<'db>,
    specialization: Option<Specialization<'db>>,
}

impl<'db> StaticMroRequest<'db> {
    pub(in crate::types::callable::scheduled_probe) fn class(self) -> StaticClassLiteral<'db> {
        self.class
    }

    pub(in crate::types::callable::scheduled_probe) fn specialization(
        self,
    ) -> Option<Specialization<'db>> {
        self.specialization
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(in crate::types::callable::scheduled_probe) struct StaticMroResultId {
    evaluation: usize,
    slot: usize,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(in crate::types::callable::scheduled_probe) struct MroNodeId {
    slot: usize,
    generation: usize,
}

pub(in crate::types::callable::scheduled_probe) type StaticMroOutcome<'db> =
    Result<Result<Mro<'db>, StaticMroError<'db>>, LookupFailure<'db>>;

#[derive(Clone, Copy)]
enum WorkOwner<'a, 'db> {
    Consumer,
    Task {
        request: StaticMroRequest<'db>,
        node: MroNodeId,
        sequence: &'a Cell<usize>,
    },
}

pub(in crate::types::callable::scheduled_probe) struct PreparedMroWork<'a, 'db, 'c> {
    router: &'a Router<'db, 'c>,
    owner: WorkOwner<'a, 'db>,
}

impl<'a, 'db, 'c> PreparedMroWork<'a, 'db, 'c> {
    pub(in crate::types::callable::scheduled_probe) fn consumer(
        router: &'a Router<'db, 'c>,
    ) -> Self {
        Self {
            router,
            owner: WorkOwner::Consumer,
        }
    }

    pub(in crate::types::callable::scheduled_probe) fn task(
        router: &'a Router<'db, 'c>,
        request: StaticMroRequest<'db>,
        node: MroNodeId,
        sequence: &'a Cell<usize>,
    ) -> Self {
        Self {
            router,
            owner: WorkOwner::Task {
                request,
                node,
                sequence,
            },
        }
    }

    pub(in crate::types::callable::scheduled_probe) fn router(&self) -> &'a Router<'db, 'c> {
        self.router
    }

    pub(in crate::types::callable::scheduled_probe) fn parent_key(&self) -> Key<'db> {
        match self.owner {
            WorkOwner::Consumer => Key::Consumer,
            WorkOwner::Task { request, .. } => Key::StaticMro(request),
        }
    }

    pub(in crate::types::callable::scheduled_probe) fn mapping_root_owner(
        &self,
    ) -> Result<MappingRootOwner<'db>, Boundary> {
        self.validate()?;
        Ok(match self.owner {
            WorkOwner::Consumer => MappingRootOwner::Consumer {
                evaluation: self.router.evaluation_domain.0.ok_or(Boundary::MroDomain)?,
            },
            WorkOwner::Task { request, node, .. } => MappingRootOwner::StaticMro { request, node },
        })
    }

    fn validate(&self) -> Result<(), Boundary> {
        if !self.router.driver_is_live() {
            return Err(Boundary::MroDomain);
        }
        match self.owner {
            WorkOwner::Consumer if self.router.consumer_active.get() => Ok(()),
            WorkOwner::Consumer => Err(Boundary::MroDomain),
            WorkOwner::Task { request, node, .. } => {
                self.router.validate_static_mro_owner(request, node)
            }
        }
    }

    pub(in crate::types::callable::scheduled_probe) async fn checkpoint(
        &self,
        units: usize,
    ) -> Result<(), Boundary> {
        self.validate()?;
        match self.owner {
            WorkOwner::Consumer => self.router.consumer_checkpoint(units).await?,
            WorkOwner::Task {
                request,
                node,
                sequence,
            } => {
                let next = sequence.get();
                sequence.set(next.checked_add(1).ok_or(Boundary::CostOverflow)?);
                self.router
                    .semantic_checkpoint(SemanticWork {
                        owner: SemanticOwner::StaticMro { request, node },
                        sequence: next,
                        units,
                    })
                    .await?;
            }
        }
        self.validate()
    }

    fn reply_owner(&self) -> ReplyOwner<'db> {
        match self.owner {
            WorkOwner::Consumer => ReplyOwner::Consumer,
            WorkOwner::Task { request, node, .. } => ReplyOwner::Task { request, node },
        }
    }
}

#[derive(Clone, Copy)]
enum ReplyOwner<'db> {
    Consumer,
    Task {
        request: StaticMroRequest<'db>,
        node: MroNodeId,
    },
}

impl<'db> ReplyOwner<'db> {
    fn key(self) -> Key<'db> {
        match self {
            Self::Consumer => Key::Consumer,
            Self::Task { request, .. } => Key::StaticMro(request),
        }
    }
}

#[derive(Default)]
struct ReplyPollState {
    outstanding: Cell<usize>,
    pending: Cell<usize>,
    epoch: Cell<usize>,
}

pub(in crate::types::callable::scheduled_probe) struct MroReply<'db> {
    answer: Cell<Option<Result<StaticMroResultId, Boundary>>>,
    owner: ReplyOwner<'db>,
    poll: Rc<ReplyPollState>,
    future_live: Cell<bool>,
    global_index: Cell<Option<usize>>,
    child_index: Cell<Option<usize>>,
    child: Cell<Option<MroNodeId>>,
    cleanup_dirty: Rc<Cell<bool>>,
}

struct MroDemandLease<'db> {
    reply: Rc<MroReply<'db>>,
    released: bool,
}

impl MroDemandLease<'_> {
    fn release(&mut self) {
        if self.released {
            return;
        }
        self.released = true;
        self.reply.future_live.set(false);
        // Each successful outstanding-count increment creates exactly one unique lease.
        self.reply
            .poll
            .outstanding
            .set(self.reply.poll.outstanding.get() - 1);
        if self.reply.global_index.get().is_some() {
            self.reply.cleanup_dirty.set(true);
        }
    }
}

impl Drop for MroDemandLease<'_> {
    fn drop(&mut self) {
        self.release();
    }
}

#[derive(Clone, Copy)]
struct WaitEdge {
    child: MroNodeId,
    backlink: usize,
    instances: usize,
}

struct Node<'db> {
    request: StaticMroRequest<'db>,
    dense_index: usize,
    task_live: bool,
    waiting_on: Option<WaitEdge>,
    incoming: Vec<MroNodeId>,
    replies: Vec<Rc<MroReply<'db>>>,
    poll: Rc<ReplyPollState>,
    notifications: usize,
}

struct Slot<'db> {
    generation: usize,
    node: Option<Node<'db>>,
}

struct MroEntry<'db> {
    answer: Option<StaticMroResultId>,
    node: Option<MroNodeId>,
    dependents: FxIndexSet<Key<'db>>,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(in crate::types::callable::scheduled_probe) struct MroCounters {
    pub(in crate::types::callable::scheduled_probe) graph_passes: usize,
    pub(in crate::types::callable::scheduled_probe) graph_nodes: usize,
    pub(in crate::types::callable::scheduled_probe) sweeps: usize,
    pub(in crate::types::callable::scheduled_probe) swept_replies: usize,
    pub(in crate::types::callable::scheduled_probe) reply_allocations: usize,
    pub(in crate::types::callable::scheduled_probe) reply_deliveries: usize,
    pub(in crate::types::callable::scheduled_probe) reply_reads: usize,
    pub(in crate::types::callable::scheduled_probe) request_probes: usize,
    pub(in crate::types::callable::scheduled_probe) peak_nodes: usize,
    pub(in crate::types::callable::scheduled_probe) peak_replies: usize,
    pub(in crate::types::callable::scheduled_probe) result_publications: usize,
}

#[derive(Default)]
pub(in crate::types::callable::scheduled_probe) struct State<'db> {
    entries: FxHashMap<StaticMroRequest<'db>, MroEntry<'db>>,
    outcomes: Vec<StaticMroOutcome<'db>>,
    slots: Vec<Slot<'db>>,
    free_slots: Vec<usize>,
    active: Vec<MroNodeId>,
    pending: Vec<Rc<MroReply<'db>>>,
    consumer_poll: Option<Rc<ReplyPollState>>,
    consumer_epoch: usize,
    cleanup_dirty: Option<Rc<Cell<bool>>>,
    edges: usize,
    notifications: usize,
    counters: MroCounters,
    #[cfg(test)]
    controlled: FxHashMap<StaticMroRequest<'db>, tests::Script<'db>>,
}

fn checked_sum(values: impl IntoIterator<Item = usize>) -> Result<usize, Boundary> {
    values.into_iter().try_fold(0usize, |total, value| {
        total.checked_add(value).ok_or(Boundary::CostOverflow)
    })
}

fn scaled(count: usize, scale: usize, extra: usize) -> Result<usize, Boundary> {
    count
        .checked_mul(scale)
        .and_then(|value| value.checked_add(extra))
        .ok_or(Boundary::CostOverflow)
}

pub(in crate::types::callable::scheduled_probe) fn table_probe_units(
    capacity: usize,
) -> Result<usize, Boundary> {
    scaled(
        capacity.checked_add(1).ok_or(Boundary::CostOverflow)?,
        64,
        32,
    )
}

fn graph_pass_units(nodes: usize, edges: usize) -> Result<usize, Boundary> {
    checked_sum([
        scaled(nodes.checked_add(1).ok_or(Boundary::CostOverflow)?, 256, 0)?,
        scaled(edges, 64, 0)?,
    ])
}

pub(in crate::types::callable::scheduled_probe) struct Budget<'a> {
    pub(in crate::types::callable::scheduled_probe) work: &'a mut usize,
    pub(in crate::types::callable::scheduled_probe) limit: usize,
}

impl Budget<'_> {
    pub(in crate::types::callable::scheduled_probe) fn reserve(
        &mut self,
        units: usize,
    ) -> Result<bool, Boundary> {
        let next = self.work.checked_add(units).ok_or(Boundary::CostOverflow)?;
        if next > self.limit {
            return Ok(false);
        }
        *self.work = next;
        Ok(true)
    }
}

pub(in crate::types::callable::scheduled_probe) struct Demand<'db> {
    pub(in crate::types::callable::scheduled_probe) child: StaticMroRequest<'db>,
    pub(in crate::types::callable::scheduled_probe) reply: Rc<MroReply<'db>>,
}

pub(in crate::types::callable::scheduled_probe) struct Completion<'db> {
    pub(in crate::types::callable::scheduled_probe) request: StaticMroRequest<'db>,
    pub(in crate::types::callable::scheduled_probe) node: MroNodeId,
    pub(in crate::types::callable::scheduled_probe) outcome: StaticMroOutcome<'db>,
}

#[derive(Default)]
pub(in crate::types::callable::scheduled_probe) struct Round<'db> {
    pub(in crate::types::callable::scheduled_probe) demands: Vec<Demand<'db>>,
    pub(in crate::types::callable::scheduled_probe) completed: Vec<Completion<'db>>,
}

#[derive(Default)]
pub(in crate::types::callable::scheduled_probe) struct Commit<'db> {
    pub(in crate::types::callable::scheduled_probe) spawned:
        Vec<(StaticMroRequest<'db>, MroNodeId)>,
    pub(in crate::types::callable::scheduled_probe) completed: Vec<StaticMroRequest<'db>>,
    pub(in crate::types::callable::scheduled_probe) wakeups: FxIndexSet<Key<'db>>,
    deliveries: Vec<(Rc<MroReply<'db>>, Result<StaticMroResultId, Boundary>)>,
}

impl<'db> State<'db> {
    fn node(&self, id: MroNodeId) -> Result<&Node<'db>, Boundary> {
        self.slots
            .get(id.slot)
            .filter(|slot| slot.generation == id.generation)
            .and_then(|slot| slot.node.as_ref())
            .ok_or(Boundary::MroDomain)
    }

    fn node_mut(&mut self, id: MroNodeId) -> Result<&mut Node<'db>, Boundary> {
        self.slots
            .get_mut(id.slot)
            .filter(|slot| slot.generation == id.generation)
            .and_then(|slot| slot.node.as_mut())
            .ok_or(Boundary::MroDomain)
    }

    fn owner_live(&self, owner: ReplyOwner<'db>, consumer_live: bool) -> bool {
        match owner {
            ReplyOwner::Consumer => consumer_live,
            ReplyOwner::Task { request, node } => self
                .node(node)
                .is_ok_and(|node| node.task_live && node.request == request),
        }
    }

    pub(in crate::types::callable::scheduled_probe) fn activate(
        &mut self,
        id: MroNodeId,
    ) -> Result<(), Boundary> {
        self.node_mut(id)?.task_live = true;
        Ok(())
    }

    pub(in crate::types::callable::scheduled_probe) fn retire_owner(
        &mut self,
        id: Option<MroNodeId>,
    ) -> Result<(), Boundary> {
        let pending = if let Some(id) = id {
            let node = self.node_mut(id)?;
            node.task_live = false;
            node.poll.pending.get()
        } else {
            self.consumer_poll
                .as_ref()
                .map_or(0, |poll| poll.pending.get())
        };
        if pending != 0
            && let Some(dirty) = &self.cleanup_dirty
        {
            dirty.set(true);
        }
        Ok(())
    }

    pub(in crate::types::callable::scheduled_probe) fn poll_units(
        &self,
        id: Option<MroNodeId>,
    ) -> Result<usize, Boundary> {
        let count = match id {
            Some(id) => self.node(id)?.poll.outstanding.get(),
            None => self
                .consumer_poll
                .as_ref()
                .map_or(0, |poll| poll.outstanding.get()),
        };
        scaled(count, 32, 0)
    }

    pub(in crate::types::callable::scheduled_probe) fn begin_poll(
        &mut self,
        id: Option<MroNodeId>,
    ) -> Result<(), Boundary> {
        let poll = if let Some(id) = id {
            Some(&self.node(id)?.poll)
        } else {
            self.consumer_epoch = self
                .consumer_epoch
                .checked_add(1)
                .ok_or(Boundary::CostOverflow)?;
            self.consumer_poll.as_ref()
        };
        if let Some(poll) = poll {
            poll.epoch.set(if id.is_some() {
                poll.epoch
                    .get()
                    .checked_add(1)
                    .ok_or(Boundary::CostOverflow)?
            } else {
                self.consumer_epoch
            });
        }
        Ok(())
    }

    pub(in crate::types::callable::scheduled_probe) fn fanout(
        &self,
        id: MroNodeId,
    ) -> Result<usize, Boundary> {
        Ok(self.node(id)?.notifications)
    }

    fn allocate_node(&mut self, request: StaticMroRequest<'db>) -> Result<MroNodeId, Boundary> {
        let id = if let Some(slot) = self.free_slots.pop() {
            let generation = self.slots[slot]
                .generation
                .checked_add(1)
                .ok_or(Boundary::CostOverflow)?;
            self.slots[slot].generation = generation;
            MroNodeId { slot, generation }
        } else {
            let id = MroNodeId {
                slot: self.slots.len(),
                generation: 0,
            };
            self.slots.push(Slot {
                generation: 0,
                node: None,
            });
            id
        };
        self.slots[id.slot].node = Some(Node {
            request,
            dense_index: self.active.len(),
            task_live: false,
            waiting_on: None,
            incoming: Vec::new(),
            replies: Vec::new(),
            poll: Rc::new(ReplyPollState::default()),
            notifications: 0,
        });
        self.active.push(id);
        self.counters.peak_nodes = self.counters.peak_nodes.max(self.active.len());
        Ok(id)
    }

    fn detach_edge(&mut self, parent: MroNodeId) -> Result<(), Boundary> {
        let Some(edge) = self.node_mut(parent)?.waiting_on.take() else {
            return Ok(());
        };
        let moved = {
            let incoming = &mut self.node_mut(edge.child)?.incoming;
            if incoming.get(edge.backlink) != Some(&parent) {
                return Err(Boundary::MroDomain);
            }
            incoming.swap_remove(edge.backlink);
            incoming.get(edge.backlink).copied()
        };
        if let Some(moved) = moved
            && let Some(wait) = &mut self.node_mut(moved)?.waiting_on
        {
            wait.backlink = edge.backlink;
        }
        self.edges = self.edges.checked_sub(1).ok_or(Boundary::MroDomain)?;
        Ok(())
    }

    fn register_edge(&mut self, parent: MroNodeId, child: MroNodeId) -> Result<bool, Boundary> {
        if let Some(edge) = &mut self.node_mut(parent)?.waiting_on {
            if edge.child != child {
                return Ok(false);
            }
            edge.instances = edge
                .instances
                .checked_add(1)
                .ok_or(Boundary::CostOverflow)?;
            return Ok(true);
        }
        let backlink = self.node(child)?.incoming.len();
        self.node_mut(child)?.incoming.push(parent);
        self.node_mut(parent)?.waiting_on = Some(WaitEdge {
            child,
            backlink,
            instances: 1,
        });
        self.edges = self.edges.checked_add(1).ok_or(Boundary::CostOverflow)?;
        Ok(true)
    }

    fn unlink_reply(&mut self, reply: &Rc<MroReply<'db>>) -> Result<(), Boundary> {
        let global = reply.global_index.take().ok_or(Boundary::MroDomain)?;
        let child = reply.child.take().ok_or(Boundary::MroDomain)?;
        let child_index = reply.child_index.take().ok_or(Boundary::MroDomain)?;
        if self
            .pending
            .get(global)
            .is_none_or(|stored| !Rc::ptr_eq(stored, reply))
        {
            return Err(Boundary::MroDomain);
        }
        self.pending.swap_remove(global);
        if let Some(moved) = self.pending.get(global) {
            moved.global_index.set(Some(global));
        }
        let replies = &mut self.node_mut(child)?.replies;
        if replies
            .get(child_index)
            .is_none_or(|stored| !Rc::ptr_eq(stored, reply))
        {
            return Err(Boundary::MroDomain);
        }
        replies.swap_remove(child_index);
        if let Some(moved) = replies.get(child_index) {
            moved.child_index.set(Some(child_index));
        }
        reply.poll.pending.set(
            reply
                .poll
                .pending
                .get()
                .checked_sub(1)
                .ok_or(Boundary::MroDomain)?,
        );
        if let ReplyOwner::Task { node: parent, .. } = reply.owner
            && let Ok(parent_node) = self.node_mut(parent)
            && let Some(edge) = &mut parent_node.waiting_on
            && edge.child == child
        {
            edge.instances = edge.instances.checked_sub(1).ok_or(Boundary::MroDomain)?;
            if edge.instances == 0 {
                self.detach_edge(parent)?;
            }
        }
        Ok(())
    }

    pub(in crate::types::callable::scheduled_probe) fn cleanup(
        &mut self,
        consumer_live: bool,
        budget: &mut Budget<'_>,
    ) -> Result<bool, Boundary> {
        if self.cleanup_dirty.as_ref().is_none_or(|dirty| !dirty.get()) {
            return Ok(true);
        }
        if !budget.reserve(scaled(
            self.pending
                .len()
                .checked_add(1)
                .ok_or(Boundary::CostOverflow)?,
            128,
            0,
        )?)? {
            return Ok(false);
        }
        self.counters.sweeps += 1;
        let mut index = 0;
        while let Some(reply) = self.pending.get(index).cloned() {
            self.counters.swept_replies += 1;
            if !reply.future_live.get() || !self.owner_live(reply.owner, consumer_live) {
                self.unlink_reply(&reply)?;
            } else {
                index += 1;
            }
        }
        if let Some(dirty) = &self.cleanup_dirty {
            dirty.set(false);
        }
        Ok(true)
    }

    fn retire_node(&mut self, id: MroNodeId) -> Result<(), Boundary> {
        self.detach_edge(id)?;
        while let Some(parent) = self.node(id)?.incoming.last().copied() {
            self.detach_edge(parent)?;
        }
        let dense = self.node(id)?.dense_index;
        self.active.swap_remove(dense);
        if let Some(moved) = self.active.get(dense).copied() {
            self.node_mut(moved)?.dense_index = dense;
        }
        self.slots[id.slot].node = None;
        self.free_slots.push(id.slot);
        Ok(())
    }

    fn stage_reply(
        &mut self,
        commit: &mut Commit<'db>,
        reply: Rc<MroReply<'db>>,
        answer: Result<StaticMroResultId, Boundary>,
        consumer_live: bool,
    ) {
        if reply.future_live.get() && self.owner_live(reply.owner, consumer_live) {
            commit.wakeups.insert(reply.owner.key());
            commit.deliveries.push((reply, answer));
        }
    }

    pub(in crate::types::callable::scheduled_probe) fn commit_round(
        &mut self,
        round: Round<'db>,
        evaluation: usize,
        consumer_live: bool,
        budget: &mut Budget<'_>,
    ) -> Result<Option<Commit<'db>>, Boundary> {
        if !self.cleanup(consumer_live, budget)? {
            return Ok(None);
        }
        let d = round.demands.len();
        let k = round.completed.len();
        if d == 0 && k == 0 {
            return Ok(Some(Commit::default()));
        }
        // Reserve table capacity separately so every later probe is priced against its actual capacity.
        if d != 0 {
            let growth = checked_sum([
                table_probe_units(self.entries.capacity())?,
                table_probe_units(
                    self.entries
                        .capacity()
                        .checked_add(d)
                        .ok_or(Boundary::CostOverflow)?,
                )?,
            ])?;
            if !budget.reserve(growth)? {
                return Ok(None);
            }
            self.entries.reserve(d);
        }
        let all_replies = self
            .pending
            .len()
            .checked_add(d)
            .ok_or(Boundary::CostOverflow)?;
        let delivery_bound = if k == 0 { d } else { all_replies };
        let vertices = self
            .active
            .len()
            .checked_add(d)
            .ok_or(Boundary::CostOverflow)?;
        let slot_count = self
            .slots
            .len()
            .checked_add(d)
            .ok_or(Boundary::CostOverflow)?;
        let notification_bound = self
            .notifications
            .checked_add(d)
            .ok_or(Boundary::CostOverflow)?;
        let probes = d
            .checked_mul(3)
            .and_then(|n| n.checked_add(k))
            .ok_or(Boundary::CostOverflow)?;
        let per_demand = checked_sum([
            scaled(vertices, 64, 96)?,
            scaled(slot_count, 64, 32)?,
            scaled(all_replies, 32, 32)?,
            scaled(vertices, 16, 24)?,
            64,
        ])?;
        let units = checked_sum([
            table_probe_units(self.entries.capacity())?
                .checked_mul(probes)
                .ok_or(Boundary::CostOverflow)?,
            per_demand.checked_mul(d).ok_or(Boundary::CostOverflow)?,
            if k == 0 {
                0
            } else {
                scaled(
                    self.outcomes
                        .len()
                        .checked_add(k)
                        .ok_or(Boundary::CostOverflow)?,
                    64,
                    32,
                )?
            },
            scaled(k, 16, 0)?,
            scaled(delivery_bound, 96, 104)?,
            if k == 0 {
                0
            } else {
                scaled(
                    self.edges.checked_add(k).ok_or(Boundary::CostOverflow)?,
                    64,
                    64,
                )?
            },
            scaled(if k == 0 { d } else { notification_bound }, 32, 32)?,
            if k == 0 {
                0
            } else {
                scaled(
                    self.free_slots
                        .len()
                        .checked_add(k)
                        .ok_or(Boundary::CostOverflow)?,
                    16,
                    8,
                )?
            },
            scaled(d.checked_add(k).ok_or(Boundary::CostOverflow)?, 64, 32)?,
        ])?;
        if !budget.reserve(units)? {
            return Ok(None);
        }
        // An edge's instance count and an owner's pending count are bounded by the global
        // subscription count; each node's notification count is bounded by the global total.
        // Their checked batch bounds above cover every increment before answers are published.
        // Slot generations are independent of those counts, so check the slots this batch
        // could reuse, including slots released by its own completions.
        for slot in round
            .completed
            .iter()
            .rev()
            .map(|completion| completion.node.slot)
            .chain(self.free_slots.iter().rev().copied())
            .take(d)
        {
            self.slots
                .get(slot)
                .ok_or(Boundary::MroDomain)?
                .generation
                .checked_add(1)
                .ok_or(Boundary::CostOverflow)?;
        }
        self.outcomes.reserve(k);
        let mut commit = Commit::default();
        for completion in round.completed {
            self.counters.request_probes += 1;
            if completion.request.evaluation != evaluation
                || self.node(completion.node)?.request != completion.request
            {
                return Err(Boundary::MroDomain);
            }
            let result = StaticMroResultId {
                evaluation,
                slot: self.outcomes.len(),
            };
            let entry = self
                .entries
                .get_mut(&completion.request)
                .ok_or(Boundary::MroDomain)?;
            if entry.answer.is_some() {
                return Err(Boundary::MroDomain);
            }
            entry.answer = Some(result);
            entry.node = None;
            commit.wakeups.extend(entry.dependents.drain(..));
            self.outcomes.push(completion.outcome);
            self.counters.result_publications += 1;
            self.notifications = self
                .notifications
                .checked_sub(self.node(completion.node)?.notifications)
                .ok_or(Boundary::MroDomain)?;
            while let Some(reply) = self.node(completion.node)?.replies.last().cloned() {
                self.unlink_reply(&reply)?;
                self.stage_reply(&mut commit, reply, Ok(result), consumer_live);
            }
            self.retire_node(completion.node)?;
            commit.completed.push(completion.request);
        }
        for demand in round.demands {
            let reply = demand.reply;
            if !reply.future_live.get() || !self.owner_live(reply.owner, consumer_live) {
                continue;
            }
            if demand.child.evaluation != evaluation {
                self.stage_reply(&mut commit, reply, Err(Boundary::MroDomain), consumer_live);
                continue;
            }
            self.counters.request_probes += 1;
            let cached = self
                .entries
                .get(&demand.child)
                .map(|entry| (entry.answer, entry.node));
            if let Some((Some(result), _)) = cached {
                self.stage_reply(&mut commit, reply, Ok(result), consumer_live);
                continue;
            }
            if let ReplyOwner::Task { node: parent, .. } = reply.owner
                && let Some(edge) = self.node(parent)?.waiting_on
                && cached.and_then(|(_, node)| node) != Some(edge.child)
            {
                self.stage_reply(&mut commit, reply, Err(Boundary::MroDomain), consumer_live);
                continue;
            }
            let child = match cached {
                Some((None, Some(child))) => child,
                Some(_) => return Err(Boundary::MroDomain),
                None => {
                    let child = self.allocate_node(demand.child)?;
                    self.counters.request_probes += 1;
                    self.entries.insert(
                        demand.child,
                        MroEntry {
                            answer: None,
                            node: Some(child),
                            dependents: FxIndexSet::default(),
                        },
                    );
                    commit.spawned.push((demand.child, child));
                    child
                }
            };
            if let ReplyOwner::Task { node: parent, .. } = reply.owner
                && !self.register_edge(parent, child)?
            {
                self.stage_reply(&mut commit, reply, Err(Boundary::MroDomain), consumer_live);
                continue;
            }
            self.counters.request_probes += 1;
            if self
                .entries
                .get_mut(&demand.child)
                .ok_or(Boundary::MroDomain)?
                .dependents
                .insert(reply.owner.key())
            {
                let node = self.node_mut(child)?;
                node.notifications = node
                    .notifications
                    .checked_add(1)
                    .ok_or(Boundary::CostOverflow)?;
                self.notifications = self
                    .notifications
                    .checked_add(1)
                    .ok_or(Boundary::CostOverflow)?;
            }
            reply.poll.pending.set(
                reply
                    .poll
                    .pending
                    .get()
                    .checked_add(1)
                    .ok_or(Boundary::CostOverflow)?,
            );
            reply.global_index.set(Some(self.pending.len()));
            reply.child_index.set(Some(self.node(child)?.replies.len()));
            reply.child.set(Some(child));
            self.pending.push(Rc::clone(&reply));
            self.counters.peak_replies = self.counters.peak_replies.max(self.pending.len());
            self.node_mut(child)?.replies.push(reply);
        }
        Ok(Some(commit))
    }

    pub(in crate::types::callable::scheduled_probe) fn deliver(
        &mut self,
        commit: Commit<'db>,
        consumer_live: bool,
    ) {
        for (reply, answer) in commit.deliveries {
            if reply.future_live.get() && self.owner_live(reply.owner, consumer_live) {
                reply.answer.set(Some(answer));
                self.counters.reply_deliveries += 1;
            }
        }
    }

    pub(in crate::types::callable::scheduled_probe) fn cyclic_round(
        &mut self,
        budget: &mut Budget<'_>,
    ) -> Result<Option<Round<'db>>, Boundary> {
        if self.edges == 0 {
            return Ok(Some(Round::default()));
        }
        if !budget.reserve(graph_pass_units(self.active.len(), self.edges)?)? {
            return Ok(None);
        }
        self.counters.graph_passes += 1;
        let mut colors = vec![0u8; self.active.len()];
        let mut positions = vec![0usize; self.active.len()];
        let mut path = Vec::with_capacity(self.active.len());
        let mut cyclic = Vec::with_capacity(self.active.len());
        for start in self.active.iter().copied() {
            let mut current = Some(start);
            while let Some(id) = current {
                let (index, next) = {
                    let node = self.node(id)?;
                    (node.dense_index, node.waiting_on.map(|edge| edge.child))
                };
                match colors[index] {
                    0 => {
                        self.counters.graph_nodes += 1;
                        colors[index] = 1;
                        positions[index] = path.len();
                        path.push(id);
                        current = next;
                    }
                    1 => {
                        cyclic.extend_from_slice(&path[positions[index]..]);
                        break;
                    }
                    _ => break,
                }
            }
            for id in path.drain(..) {
                colors[self.node(id)?.dense_index] = 2;
            }
        }
        let mut round = Round::default();
        for node in cyclic {
            round.completed.push(Completion {
                request: self.node(node)?.request,
                node,
                outcome: Err(LookupFailure::Unsupported(
                    LookupOperation::MroCycleRecovery,
                )),
            });
        }
        Ok(Some(round))
    }

    pub(in crate::types::callable::scheduled_probe) fn values(
        &self,
    ) -> FxHashMap<StaticMroRequest<'db>, StaticMroResultId> {
        self.entries
            .iter()
            .filter_map(|(request, entry)| entry.answer.map(|result| (*request, result)))
            .collect()
    }

    pub(in crate::types::callable::scheduled_probe) fn pending(
        &self,
    ) -> FxHashSet<StaticMroRequest<'db>> {
        self.entries
            .iter()
            .filter_map(|(request, entry)| entry.answer.is_none().then_some(*request))
            .collect()
    }

    pub(in crate::types::callable::scheduled_probe) fn counters(&self) -> MroCounters {
        self.counters
    }

    pub(in crate::types::callable::scheduled_probe) fn finish_driver(&mut self) {
        // Storage admission includes this release even when no further budget is available.
        for reply in self.pending.drain(..) {
            reply.global_index.set(None);
            reply.child_index.set(None);
            reply.child.set(None);
            reply.poll.pending.set(reply.poll.pending.get() - 1);
        }
        for slot in &mut self.slots {
            slot.node = None;
        }
        self.active.clear();
        self.free_slots.clear();
        self.edges = 0;
        self.notifications = 0;
        if let Some(dirty) = &self.cleanup_dirty {
            dirty.set(false);
        }
    }
}

impl<'db, 'c> Router<'db, 'c> {
    pub(in crate::types::callable::scheduled_probe) fn driver_is_live(&self) -> bool {
        self.driver_live.get()
    }

    pub(in crate::types::callable::scheduled_probe) fn validate_static_mro_owner(
        &self,
        request: StaticMroRequest<'db>,
        node: MroNodeId,
    ) -> Result<(), Boundary> {
        if !self.driver_is_live() || self.evaluation_domain.0 != Some(request.evaluation) {
            return Err(Boundary::MroDomain);
        }
        let state = self.static_mro.borrow();
        let owner = state.node(node)?;
        if owner.task_live && owner.request == request {
            Ok(())
        } else {
            Err(Boundary::MroDomain)
        }
    }

    pub(in crate::types::callable::scheduled_probe) async fn demand_static_mro(
        &self,
        db: &'db dyn Db,
        work: &PreparedMroWork<'_, 'db, 'c>,
        class: StaticClassLiteral<'db>,
        specialization: Option<Specialization<'db>>,
    ) -> Result<StaticMroResultId, LookupFailure<'db>> {
        if !std::ptr::eq(self, work.router) {
            return Err(Boundary::MroDomain.into());
        }
        work.checkpoint(256).await?;
        let env = ProgramEnvironment::from_scope(class.body_scope(db));
        let declarations = self
            .declarations
            .as_ref()
            .ok_or(Boundary::SourcePreparation)?;
        let program = declarations.program();
        if env.program(db) != program
            || specialization.is_some_and(|s| s.generic_context(db).program(db) != program)
        {
            return Err(Boundary::ProgramDomain.into());
        }
        self.validate_declarations(db, &env)?;
        let request = StaticMroRequest {
            evaluation: self.evaluation_domain.0.ok_or(Boundary::MroDomain)?,
            class,
            specialization,
        };
        let owner = work.reply_owner();
        let (poll, dirty) = {
            let mut state = self.static_mro.borrow_mut();
            let dirty = Rc::clone(
                state
                    .cleanup_dirty
                    .get_or_insert_with(|| Rc::new(Cell::new(false))),
            );
            let poll = match owner {
                ReplyOwner::Consumer => {
                    let epoch = state.consumer_epoch;
                    Rc::clone(state.consumer_poll.get_or_insert_with(|| {
                        Rc::new(ReplyPollState {
                            epoch: Cell::new(epoch),
                            ..ReplyPollState::default()
                        })
                    }))
                }
                ReplyOwner::Task { node, .. } => Rc::clone(&state.node(node)?.poll),
            };
            state.counters.reply_allocations += 1;
            (poll, dirty)
        };
        poll.outstanding.set(
            poll.outstanding
                .get()
                .checked_add(1)
                .ok_or(Boundary::CostOverflow)?,
        );
        let reply = Rc::new(MroReply {
            answer: Cell::new(None),
            owner,
            poll,
            future_live: Cell::new(true),
            global_index: Cell::new(None),
            child_index: Cell::new(None),
            child: Cell::new(None),
            cleanup_dirty: dirty,
        });
        let mut lease = MroDemandLease {
            reply: Rc::clone(&reply),
            released: false,
        };
        let mut registered = false;
        let mut epoch = reply.poll.epoch.get();
        std::future::poll_fn(move |_| {
            if let Err(error) = work.validate() {
                return Poll::Ready(Err(error.into()));
            }
            if !registered {
                let Some(count) = self.mro_effect_count.get().checked_add(1) else {
                    return Poll::Ready(Err(Boundary::CostOverflow.into()));
                };
                self.mro_effect_count.set(count);
                self.effects
                    .borrow_mut()
                    .push(Effect::DemandStaticMro(Demand {
                        child: request,
                        reply: Rc::clone(&reply),
                    }));
                registered = true;
                return Poll::Pending;
            }
            let current = reply.poll.epoch.get();
            if current == epoch {
                return Poll::Pending;
            }
            epoch = current;
            self.static_mro.borrow_mut().counters.reply_reads += 1;
            match reply.answer.take() {
                Some(result) => {
                    lease.release();
                    Poll::Ready(result.map_err(Into::into))
                }
                None => Poll::Pending,
            }
        })
        .await
    }

    async fn view_static_mro<T>(
        &self,
        work: &PreparedMroWork<'_, 'db, 'c>,
        id: StaticMroResultId,
        view: impl FnOnce(&Result<Mro<'db>, StaticMroError<'db>>) -> T,
    ) -> Result<T, LookupFailure<'db>> {
        if !std::ptr::eq(self, work.router) {
            return Err(Boundary::MroDomain.into());
        }
        work.checkpoint(8).await?;
        if self.evaluation_domain.0 != Some(id.evaluation) {
            return Err(Boundary::MroDomain.into());
        }
        let copy_units = {
            let state = self.static_mro.borrow();
            match state.outcomes.get(id.slot).ok_or(Boundary::MroDomain)? {
                Err(failure) => Some(lookup_failure_copy_units(failure)),
                Ok(_) => None,
            }
        };
        if let Some(units) = copy_units {
            work.checkpoint(units).await?;
        }
        work.validate()?;
        let state = self.static_mro.borrow();
        match state.outcomes.get(id.slot).ok_or(Boundary::MroDomain)? {
            Ok(result) => Ok(view(result)),
            Err(failure) => Err(failure.clone()),
        }
    }

    pub(in crate::types::callable::scheduled_probe) async fn static_mro_is_cycle(
        &self,
        work: &PreparedMroWork<'_, 'db, 'c>,
        id: StaticMroResultId,
    ) -> Result<bool, LookupFailure<'db>> {
        self.view_static_mro(work, id, |result| {
            result.as_ref().is_err_and(StaticMroError::is_cycle)
        })
        .await
    }

    pub(in crate::types::callable::scheduled_probe) async fn static_mro_len(
        &self,
        work: &PreparedMroWork<'_, 'db, 'c>,
        id: StaticMroResultId,
    ) -> Result<usize, LookupFailure<'db>> {
        self.view_static_mro(work, id, |result| selected_entries(result).len())
            .await
    }

    pub(in crate::types::callable::scheduled_probe) async fn static_mro_entry(
        &self,
        work: &PreparedMroWork<'_, 'db, 'c>,
        id: StaticMroResultId,
        index: usize,
    ) -> Result<Option<ClassBase<'db>>, LookupFailure<'db>> {
        self.view_static_mro(work, id, |result| {
            selected_entries(result).get(index).copied()
        })
        .await
    }
}

fn selected_entries<'a, 'db>(result: &'a Result<Mro<'db>, StaticMroError<'db>>) -> &'a Mro<'db> {
    match result {
        Ok(mro) => mro,
        Err(error) => error.fallback_mro(),
    }
}

fn lookup_failure_copy_units(failure: &LookupFailure<'_>) -> usize {
    match failure {
        LookupFailure::Boundary(_) | LookupFailure::Unsupported(_) => 8,
        LookupFailure::Missing(missing) => match &missing.0 {
            DeclarationKey::Global(_)
            | DeclarationKey::Namespace(_, _)
            | DeclarationKey::MemberInput(_, _, _) => 16,
            DeclarationKey::File(_)
            | DeclarationKey::Context(_)
            | DeclarationKey::ExplicitBases(_)
            | DeclarationKey::ConvertedExplicitBase(_, _)
            | DeclarationKey::ObjectBase(_)
            | DeclarationKey::ProperMro(_)
            | DeclarationKey::ClassInput(_, _)
            | DeclarationKey::Signature(_)
            | DeclarationKey::Promotion(_) => 8,
        },
    }
}

pub(in crate::types::callable::scheduled_probe) fn task<'eval, 'db: 'eval, 'c: 'eval, R: 'eval>(
    db: &'db dyn Db,
    router: &'eval Router<'db, 'c>,
    request: StaticMroRequest<'db>,
    node: MroNodeId,
) -> Task<'eval, 'db, 'c, R> {
    Box::pin(async move {
        #[cfg(test)]
        let controlled = router.static_mro.borrow().controlled.get(&request).cloned();
        #[cfg(test)]
        if let Some(script) = controlled {
            return Output::StaticMro(
                tests::controlled_task(db, router, request, node, script).await,
            );
        }
        let sequence = Cell::new(0);
        let work = PreparedMroWork::task(router, request, node, &sequence);
        Output::StaticMro(
            super::evaluate_static_mro(db, &work, request.class, request.specialization).await,
        )
    })
}
