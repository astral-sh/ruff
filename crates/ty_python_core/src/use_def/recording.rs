//! Defer constructing flow state while a function only records straight-line operations.
//! Scope IDs for definitions and uses are append-only, so we can give the AST visitor their
//! final IDs before building any place states. A flow-dependent observation replays the prefix.
use super::*;
use crate::predicate::{CallableAndCallExpr, Predicate, PredicateNode};
use std::cell::{OnceCell, RefCell};
use std::ops::{Deref, DerefMut};
use std::sync::Mutex;

#[derive(Debug)]
pub(crate) struct RecordingBuilder<'db> {
    builder: OnceCell<Box<UseDefMapBuilder<'db>>>,
    recorded: RefCell<Option<RecordedScope<'db>>>,
}

#[derive(Debug, get_size2::GetSize, salsa::SalsaValue)]
pub(crate) struct RecordedScope<'db> {
    kind: ScopeKind,
    operations: Vec<Operation<'db>>,
    definitions: usize,
    last_range: Option<usize>,
    uses: usize,
}

#[derive(Clone, Copy, Debug, get_size2::GetSize, salsa::SalsaValue)]
enum Operation<'db> {
    AddPlace(ScopedPlaceId),
    Bind(
        ScopedPlaceId,
        Definition<'db>,
        PreviousDefinitions,
        FutureDefinitions,
        ImportedQualifierAction,
    ),
    Declare(ScopedPlaceId, Definition<'db>),
    Combined(ScopedPlaceId, Definition<'db>, DefinitionCategory),
    Delete(ScopedPlaceId),
    Use(ScopedPlaceId),
    Range(TextRange, bool),
    BooleanRoot(NodeIndex),
    Call(CallableAndCallExpr<'db>),
    Unreachable,
}

impl<'db> Operation<'db> {
    fn apply(self, builder: &mut UseDefMapBuilder<'db>) {
        match self {
            Self::AddPlace(place) => builder.add_place(place),
            Self::Bind(place, definition, previous, future, import) => {
                builder.record_binding(place, definition, previous, future, import);
            }
            Self::Declare(place, definition) => builder.record_declaration(place, definition),
            Self::Combined(place, definition, category) => {
                builder.record_combined_definition(place, definition, category);
            }
            Self::Delete(place) => builder.delete_binding(place),
            Self::Use(place) => {
                builder.record_use(place);
            }
            Self::Range(range, in_type_checking) => {
                builder.record_range_reachability(range, in_type_checking);
            }
            Self::BooleanRoot(node) => builder.record_boolean_test_root(node),
            Self::Call(call) => {
                let id = builder.add_predicate(PredicateOrLiteral::Predicate(Predicate {
                    node: PredicateNode::IsNonTerminalCall(call),
                    is_positive: true,
                }));
                let narrowing = builder.narrowing_constraints.add_atom(id);
                let reachability = builder.reachability_constraints.add_atom(id);
                builder.record_non_terminal_call_constraints(reachability, narrowing);
            }
            Self::Unreachable => builder.mark_unreachable(),
        }
    }
}

impl<'db> RecordedScope<'db> {
    fn replay(&self) -> Box<UseDefMapBuilder<'db>> {
        let mut builder = Box::new(UseDefMapBuilder::new(self.kind));
        for &operation in &self.operations {
            operation.apply(&mut builder);
        }
        debug_assert_eq!(builder.next_definition_id().index(), self.definitions);
        debug_assert_eq!(builder.bindings_by_use.len(), self.uses);
        builder
    }
}

impl<'db> RecordingBuilder<'db> {
    pub(crate) fn new(kind: ScopeKind) -> Self {
        if matches!(kind, ScopeKind::Function | ScopeKind::Lambda) {
            Self {
                builder: OnceCell::new(),
                recorded: RefCell::new(Some(RecordedScope {
                    kind,
                    operations: Vec::new(),
                    definitions: 1,
                    last_range: None,
                    uses: 0,
                })),
            }
        } else {
            Self {
                builder: OnceCell::from(Box::new(UseDefMapBuilder::new(kind))),
                recorded: RefCell::new(None),
            }
        }
    }

    fn record(&mut self, operation: Operation<'db>) {
        if let Some(recorded) = self.recorded.get_mut() {
            match operation {
                Operation::Bind(..)
                | Operation::Declare(..)
                | Operation::Combined(..)
                | Operation::Delete(_) => recorded.definitions += 1,
                Operation::Use(_) => recorded.uses += 1,
                Operation::Call(_) | Operation::Unreachable => recorded.last_range = None,
                _ => {}
            }
            recorded.operations.push(operation);
        } else {
            operation.apply(self.builder.get_mut().unwrap());
        }
    }

    pub(crate) fn add_place(&mut self, place: ScopedPlaceId) {
        self.record(Operation::AddPlace(place));
    }

    pub(crate) fn next_definition_id(&self) -> ScopedDefinitionId {
        if let Some(builder) = self.builder.get() {
            builder.next_definition_id()
        } else {
            ScopedDefinitionId::new(self.recorded.borrow().as_ref().unwrap().definitions)
        }
    }

    pub(crate) fn record_binding(
        &mut self,
        place: ScopedPlaceId,
        definition: Definition<'db>,
        previous: PreviousDefinitions,
        future: FutureDefinitions,
        import: ImportedQualifierAction,
    ) {
        self.record(Operation::Bind(place, definition, previous, future, import));
    }
    pub(crate) fn record_declaration(&mut self, place: ScopedPlaceId, definition: Definition<'db>) {
        self.record(Operation::Declare(place, definition));
    }
    pub(crate) fn record_combined_definition(
        &mut self,
        place: ScopedPlaceId,
        definition: Definition<'db>,
        part: DefinitionCategory,
    ) {
        self.record(Operation::Combined(place, definition, part));
    }
    pub(crate) fn delete_binding(&mut self, place: ScopedPlaceId) {
        self.record(Operation::Delete(place));
    }
    pub(crate) fn record_range_reachability(&mut self, range: TextRange, in_type_checking: bool) {
        if let Some(recorded) = self.recorded.get_mut() {
            if let Some(index) = recorded.last_range
                && let Operation::Range(last, previous) = &mut recorded.operations[index]
                && *previous == in_type_checking
            {
                *last = last.cover(range);
                return;
            }
            recorded.last_range = Some(recorded.operations.len());
            recorded
                .operations
                .push(Operation::Range(range, in_type_checking));
        } else {
            self.builder
                .get_mut()
                .unwrap()
                .record_range_reachability(range, in_type_checking);
        }
    }
    pub(crate) fn record_nonterminal_call(&mut self, call: CallableAndCallExpr<'db>) {
        self.record(Operation::Call(call));
    }
    pub(crate) fn record_boolean_test_root(&mut self, node: NodeIndex) {
        self.record(Operation::BooleanRoot(node));
    }
    pub(crate) fn mark_unreachable(&mut self) {
        self.record(Operation::Unreachable);
    }
    pub(crate) fn record_use(&mut self, place: ScopedPlaceId) -> ScopedUseId {
        if let Some(builder) = self.builder.get_mut() {
            return builder.record_use(place);
        }
        let recorded = self.recorded.get_mut().as_mut().unwrap();
        let id = ScopedUseId::new(recorded.uses);
        recorded.uses += 1;
        recorded.operations.push(Operation::Use(place));
        id
    }

    pub(crate) fn finish(self, interner: &Arc<Mutex<UseDefMapInterner>>) -> ScopeMap<'db> {
        if let Some(builder) = self.builder.into_inner() {
            ScopeMap::Built(Arc::new(builder.finish(Arc::clone(interner))))
        } else {
            let scope = self.recorded.into_inner().unwrap();
            ScopeMap::Recorded(Box::new(scope), Arc::clone(interner))
        }
    }
}

impl<'db> Deref for RecordingBuilder<'db> {
    type Target = UseDefMapBuilder<'db>;

    fn deref(&self) -> &Self::Target {
        self.builder
            .get_or_init(|| self.recorded.take().unwrap().replay())
    }
}

impl DerefMut for RecordingBuilder<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        if self.builder.get().is_none() {
            let recorded = self
                .recorded
                .get_mut()
                .take()
                .expect("a recording scope has its operations");
            self.builder = OnceCell::from(recorded.replay());
        }
        self.builder.get_mut().unwrap()
    }
}

/// The index owns the ordered operations. The per-scope Salsa query owns the replayed map,
/// so its synchronized cache and its revision dependencies are both managed by Salsa.
#[derive(Debug, get_size2::GetSize, salsa::SalsaValue)]
pub(crate) enum ScopeMap<'db> {
    Built(Arc<UseDefMap<'db>>),
    Recorded(Box<RecordedScope<'db>>, Arc<Mutex<UseDefMapInterner>>),
}

impl<'db> ScopeMap<'db> {
    pub(crate) fn materialize(&self) -> Arc<UseDefMap<'db>> {
        match self {
            Self::Built(map) => Arc::clone(map),
            Self::Recorded(scope, interner) => {
                Arc::new(scope.replay().finish(Arc::clone(interner)))
            }
        }
    }
}
