//! Operations deferred until the cycle variables they consume have values.
//!
//! An operation whose operand is a cycle marker cannot be evaluated while the marker's value is
//! still being inferred. The query records the operation against a variable of its own and uses
//! that variable's marker as the operation's result.

use std::cell::{Cell, Ref, RefCell};

use ruff_python_ast::name::Name;
use ruff_python_ast::{Operator, UnaryOp};
use rustc_hash::{FxHashMap, FxHashSet};
use smallvec::SmallVec;
use ty_python_core::frozen::FrozenMap;
use ty_python_core::narrowing_constraints::ScopedNarrowingConstraint;
use ty_python_core::place::ScopedPlaceId;
use ty_python_core::scope::ScopeId;
use ty_python_core::{EvaluationMode, ExpressionNodeKey, NarrowingEvaluator, use_def_map};

use crate::reachability::predicate_scope;
use crate::types::call::{Argument, CallArguments};
use crate::types::cycle_variable::{CycleOwner, CycleSlot, CycleVariable};
use crate::types::infer::BinaryOperationContext;
use crate::types::narrow::NarrowingEvaluatorExtension;
use crate::types::tuple::{TupleLength, TupleSpecBuilder, TupleType};
use crate::types::visitor::any_over_type_for_cycle_markers;
use crate::types::{BoundTypeVarInstance, MemberLookupPolicy, Type, TypeContext, UnionType};
use crate::{Db, ProgramEnvironment};

/// A narrowing constraint on a place, identified by the scope whose use-def map defines it.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, get_size2::GetSize, salsa::SalsaValue)]
pub(crate) struct Narrowing<'db> {
    scope: ScopeId<'db>,
    place: ScopedPlaceId,
    constraint: ScopedNarrowingConstraint,
}

impl<'db> Narrowing<'db> {
    /// The narrowing that `evaluator` applies to `place`, or `None` when it applies none or
    /// makes the path unreachable, so that no operation is worth recording.
    pub(crate) fn new(
        db: &'db dyn Db,
        evaluator: &NarrowingEvaluator<'_, 'db>,
        place: ScopedPlaceId,
    ) -> Option<Self> {
        let constraint = evaluator.constraint();
        if constraint == ScopedNarrowingConstraint::ALWAYS_TRUE
            || constraint == ScopedNarrowingConstraint::ALWAYS_FALSE
        {
            return None;
        }
        let predicate = evaluator
            .narrowing_constraints()
            .get_interior_node(constraint)
            .atom;
        Some(Self {
            scope: predicate_scope(db, &evaluator.predicates()[predicate]),
            place,
            constraint,
        })
    }

    pub(crate) fn apply(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        value: Type<'db>,
    ) -> Type<'db> {
        use_def_map(db, self.scope)
            .narrowing_evaluator(self.constraint)
            .narrow(db, env, value, self.place)
    }
}

/// The arguments of a call, as inferred when the call was deferred.
#[salsa::interned(debug, heap_size=ruff_memory_usage::heap_size)]
pub(crate) struct CallSnapshot<'db> {
    #[returns(ref)]
    arguments: Box<[(CapturedArgument, Type<'db>)]>,
}

// The Salsa heap is tracked separately.
impl get_size2::GetSize for CallSnapshot<'_> {}

/// How an argument was passed.
#[derive(Clone, Debug, PartialEq, Eq, Hash, get_size2::GetSize, salsa::SalsaValue)]
pub(crate) enum CapturedArgument {
    Synthetic,
    Positional,
    Variadic,
    Keyword(Name),
    Keywords,
}

impl CapturedArgument {
    fn as_argument(&self) -> Argument<'_> {
        match self {
            Self::Synthetic => Argument::Synthetic,
            Self::Positional => Argument::Positional,
            Self::Variadic => Argument::Variadic,
            Self::Keyword(name) => Argument::Keyword(name),
            Self::Keywords => Argument::Keywords,
        }
    }
}

impl<'db> CallSnapshot<'db> {
    /// Captures the arguments with the type each was inferred to have without a specific
    /// parameter context.
    pub(crate) fn capture(db: &'db dyn Db, arguments: &CallArguments<'_, 'db>) -> Self {
        let arguments = arguments
            .iter()
            .map(|(argument, types)| {
                let argument = match argument {
                    Argument::Synthetic => CapturedArgument::Synthetic,
                    Argument::Positional => CapturedArgument::Positional,
                    Argument::Variadic => CapturedArgument::Variadic,
                    Argument::Keyword(name) => CapturedArgument::Keyword(Name::new(name)),
                    Argument::Keywords => CapturedArgument::Keywords,
                };
                (argument, types.get_default().unwrap_or_else(Type::unknown))
            })
            .collect::<Box<[_]>>();
        Self::new(db, arguments)
    }

    /// The types of the arguments, in call order.
    pub(crate) fn types(self, db: &'db dyn Db) -> impl Iterator<Item = Type<'db>> + 'db {
        self.arguments(db).iter().map(|(_, ty)| *ty)
    }

    /// The snapshot with the type of the argument at `index` replaced by `ty`.
    pub(crate) fn with_argument(self, db: &'db dyn Db, index: usize, ty: Type<'db>) -> Self {
        self.map_types(
            db,
            |position, argument| if position == index { ty } else { argument },
        )
    }

    fn map_types(self, db: &'db dyn Db, mut f: impl FnMut(usize, Type<'db>) -> Type<'db>) -> Self {
        let arguments = self
            .arguments(db)
            .iter()
            .enumerate()
            .map(|(index, (argument, ty))| (argument.clone(), f(index, *ty)))
            .collect::<Box<[_]>>();
        Self::new(db, arguments)
    }

    fn try_map_types(
        self,
        db: &'db dyn Db,
        mut f: impl FnMut(Type<'db>) -> Option<Type<'db>>,
    ) -> Option<Self> {
        let arguments = self
            .arguments(db)
            .iter()
            .map(|(argument, ty)| Some((argument.clone(), f(*ty)?)))
            .collect::<Option<Box<[_]>>>()?;
        Some(Self::new(db, arguments))
    }

    /// The arguments for matching parameters and checking types again.
    pub(crate) fn to_arguments(self, db: &'db dyn Db) -> CallArguments<'db, 'db> {
        self.arguments(db)
            .iter()
            .map(|(argument, ty)| (argument.as_argument(), Some(*ty)))
            .collect()
    }
}

/// The elements of a tuple display, as inferred when the display was deferred.
#[salsa::interned(debug, heap_size=ruff_memory_usage::heap_size)]
pub(crate) struct TupleSnapshot<'db> {
    #[returns(ref)]
    elements: Box<[TupleSnapshotElement<'db>]>,
}

// The Salsa heap is tracked separately.
impl get_size2::GetSize for TupleSnapshot<'_> {}

/// One element of a deferred tuple display.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, get_size2::GetSize, salsa::SalsaValue)]
pub(crate) enum TupleSnapshotElement<'db> {
    Element(Type<'db>),
    /// A starred element, with the length of its literal iterable when known.
    Spread {
        iterable: Type<'db>,
        length: Option<usize>,
    },
}

impl<'db> TupleSnapshotElement<'db> {
    fn ty(self) -> Type<'db> {
        match self {
            Self::Element(ty) | Self::Spread { iterable: ty, .. } => ty,
        }
    }

    fn with_type(self, ty: Type<'db>) -> Self {
        match self {
            Self::Element(_) => Self::Element(ty),
            Self::Spread { length, .. } => Self::Spread {
                iterable: ty,
                length,
            },
        }
    }
}

impl<'db> TupleSnapshot<'db> {
    pub(crate) fn capture(
        db: &'db dyn Db,
        elements: impl IntoIterator<Item = TupleSnapshotElement<'db>>,
    ) -> Self {
        Self::new(db, elements.into_iter().collect::<Box<[_]>>())
    }

    /// The types of the elements, in display order.
    pub(crate) fn types(self, db: &'db dyn Db) -> impl Iterator<Item = Type<'db>> + 'db {
        self.elements(db).iter().map(|element| element.ty())
    }

    fn map_types(self, db: &'db dyn Db, mut f: impl FnMut(usize, Type<'db>) -> Type<'db>) -> Self {
        let elements = self
            .elements(db)
            .iter()
            .enumerate()
            .map(|(index, element)| element.with_type(f(index, element.ty())))
            .collect::<Box<[_]>>();
        Self::new(db, elements)
    }

    fn try_map_types(
        self,
        db: &'db dyn Db,
        mut f: impl FnMut(Type<'db>) -> Option<Type<'db>>,
    ) -> Option<Self> {
        let elements = self
            .elements(db)
            .iter()
            .map(|element| Some(element.with_type(f(element.ty())?)))
            .collect::<Option<Box<[_]>>>()?;
        Some(Self::new(db, elements))
    }

    /// The tuple the display evaluates to, expanding each starred element.
    pub(crate) fn build(self, db: &'db dyn Db, env: &ProgramEnvironment<'db>) -> Type<'db> {
        let elements = self.elements(db);
        let mut builder = TupleSpecBuilder::with_capacity(elements.len());
        for element in elements {
            match *element {
                TupleSnapshotElement::Element(ty) => builder.push(ty),
                TupleSnapshotElement::Spread { iterable, length } => {
                    let spec = iterable.iterate(db, env).into_owned();
                    let spec = length
                        .and_then(|length| spec.resize(db, env, TupleLength::Fixed(length)).ok())
                        .unwrap_or(spec);
                    builder = builder.concat(db, env, &spec);
                }
            }
        }
        Type::tuple(TupleType::new(db, env, &builder.build()))
    }
}

/// An operation whose result is a cycle variable of the query that recorded it.
///
/// Inputs are ordinary types that may contain cycle markers.
#[derive(Clone, Debug, Eq, PartialEq, Hash, get_size2::GetSize, salsa::SalsaValue)]
pub(crate) enum Operation<'db> {
    Subscript {
        value: Type<'db>,
        key: Type<'db>,
    },
    Member {
        value: Type<'db>,
        name: Name,
        policy: MemberLookupPolicy,
    },
    Iterate {
        value: Type<'db>,
        mode: EvaluationMode,
    },
    /// One target of an unpacking assignment with `length` targets.
    Unpack {
        value: Type<'db>,
        length: TupleLength,
        index: usize,
    },
    Enter {
        value: Type<'db>,
        mode: EvaluationMode,
    },
    Await {
        value: Type<'db>,
    },
    Binary {
        left: Type<'db>,
        right: Type<'db>,
        operator: Operator,
        context: BinaryOperationContext<'db>,
    },
    Unary {
        operand: Type<'db>,
        operator: UnaryOp,
    },
    /// The augmented assignment `left operator= right`.
    Augmented {
        left: Type<'db>,
        right: Type<'db>,
        operator: Operator,
        context: BinaryOperationContext<'db>,
    },
    /// A call of `callable` with `arguments`, inferred under the type context `tcx`.
    Call {
        callable: Type<'db>,
        arguments: CallSnapshot<'db>,
        tcx: TypeContext<'db>,
    },
    Narrow {
        value: Type<'db>,
        narrowing: Narrowing<'db>,
    },
    /// The keys of a mapping expanded by `**`.
    MappingKey {
        value: Type<'db>,
    },
    /// The values of a mapping expanded by `**`.
    MappingValue {
        value: Type<'db>,
    },
    /// The type argument `parameter` of the collection class that `value` specializes.
    TypeArgument {
        value: Type<'db>,
        parameter: BoundTypeVarInstance<'db>,
    },
    /// A tuple display, whose starred elements are expanded once their types are known.
    Tuple {
        elements: TupleSnapshot<'db>,
    },
    /// The value of an expression in a nested scope whose inference is still provisional.
    ///
    /// The scope's inference is read again when the operation is evaluated; the operation has
    /// no type inputs.
    ScopeExpression {
        scope: ScopeId<'db>,
        tcx: TypeContext<'db>,
        expression: ExpressionNodeKey,
    },
}

impl<'db> Operation<'db> {
    /// The types the operation consumes.
    pub(crate) fn inputs(&self, db: &'db dyn Db) -> SmallVec<[Type<'db>; 2]> {
        match self {
            Self::Subscript { value, key } => SmallVec::from_buf([*value, *key]),
            Self::Binary { left, right, .. } | Self::Augmented { left, right, .. } => {
                SmallVec::from_buf([*left, *right])
            }
            Self::Member { value, .. }
            | Self::Iterate { value, .. }
            | Self::Unpack { value, .. }
            | Self::Enter { value, .. }
            | Self::Await { value }
            | Self::MappingKey { value }
            | Self::MappingValue { value }
            | Self::TypeArgument { value, .. } => SmallVec::from_slice(&[*value]),
            Self::Unary { operand, .. } | Self::Narrow { value: operand, .. } => {
                SmallVec::from_slice(&[*operand])
            }
            Self::Call {
                callable,
                arguments,
                ..
            } => std::iter::once(*callable)
                .chain(arguments.types(db))
                .collect(),
            Self::Tuple { elements } => elements.types(db).collect(),
            Self::ScopeExpression { .. } => SmallVec::new(),
        }
    }

    /// Replaces every type the operation consumes, or returns `None` if `f` rejects one.
    pub(crate) fn map_inputs(
        &self,
        db: &'db dyn Db,
        mut f: impl FnMut(Type<'db>) -> Option<Type<'db>>,
    ) -> Option<Self> {
        Some(match self {
            Self::Subscript { value, key } => Self::Subscript {
                value: f(*value)?,
                key: f(*key)?,
            },
            Self::Member {
                value,
                name,
                policy,
            } => Self::Member {
                value: f(*value)?,
                name: name.clone(),
                policy: *policy,
            },
            Self::Iterate { value, mode } => Self::Iterate {
                value: f(*value)?,
                mode: *mode,
            },
            Self::Unpack {
                value,
                length,
                index,
            } => Self::Unpack {
                value: f(*value)?,
                length: *length,
                index: *index,
            },
            Self::Enter { value, mode } => Self::Enter {
                value: f(*value)?,
                mode: *mode,
            },
            Self::Await { value } => Self::Await { value: f(*value)? },
            Self::Binary {
                left,
                right,
                operator,
                context,
            } => Self::Binary {
                left: f(*left)?,
                right: f(*right)?,
                operator: *operator,
                context: *context,
            },
            Self::Unary { operand, operator } => Self::Unary {
                operand: f(*operand)?,
                operator: *operator,
            },
            Self::Augmented {
                left,
                right,
                operator,
                context,
            } => Self::Augmented {
                left: f(*left)?,
                right: f(*right)?,
                operator: *operator,
                context: *context,
            },
            Self::Call {
                callable,
                arguments,
                tcx,
            } => Self::Call {
                callable: f(*callable)?,
                arguments: arguments.try_map_types(db, &mut f)?,
                tcx: *tcx,
            },
            Self::Narrow { value, narrowing } => Self::Narrow {
                value: f(*value)?,
                narrowing: *narrowing,
            },
            Self::MappingKey { value } => Self::MappingKey { value: f(*value)? },
            Self::MappingValue { value } => Self::MappingValue { value: f(*value)? },
            Self::TypeArgument { value, parameter } => Self::TypeArgument {
                value: f(*value)?,
                parameter: *parameter,
            },
            Self::Tuple { elements } => Self::Tuple {
                elements: elements.try_map_types(db, &mut f)?,
            },
            Self::ScopeExpression { .. } => self.clone(),
        })
    }

    /// Combines the inputs recorded for the same variable by two cycle iterations.
    fn cycle_normalized(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        previous: &Self,
    ) -> Self {
        let join = |current: Type<'db>, previous: Type<'db>| {
            if current == previous {
                current
            } else {
                UnionType::from_elements_cycle_recovery(db, env, [previous, current])
            }
        };
        match (self, previous) {
            (
                Self::Subscript { value, key },
                Self::Subscript {
                    value: previous_value,
                    key: previous_key,
                },
            ) => Self::Subscript {
                value: join(value, *previous_value),
                key: join(key, *previous_key),
            },
            (
                Self::Member {
                    value,
                    name,
                    policy,
                },
                Self::Member {
                    value: previous_value,
                    ..
                },
            ) => Self::Member {
                value: join(value, *previous_value),
                name,
                policy,
            },
            (
                Self::Iterate { value, mode },
                Self::Iterate {
                    value: previous_value,
                    ..
                },
            ) => Self::Iterate {
                value: join(value, *previous_value),
                mode,
            },
            (
                Self::Unpack {
                    value,
                    length,
                    index,
                },
                Self::Unpack {
                    value: previous_value,
                    ..
                },
            ) => Self::Unpack {
                value: join(value, *previous_value),
                length,
                index,
            },
            (
                Self::Enter { value, mode },
                Self::Enter {
                    value: previous_value,
                    ..
                },
            ) => Self::Enter {
                value: join(value, *previous_value),
                mode,
            },
            (
                Self::Await { value },
                Self::Await {
                    value: previous_value,
                },
            ) => Self::Await {
                value: join(value, *previous_value),
            },
            (
                Self::Binary {
                    left,
                    right,
                    operator,
                    context,
                },
                Self::Binary {
                    left: previous_left,
                    right: previous_right,
                    ..
                },
            ) => Self::Binary {
                left: join(left, *previous_left),
                right: join(right, *previous_right),
                operator,
                context,
            },
            (
                Self::Unary { operand, operator },
                Self::Unary {
                    operand: previous_operand,
                    ..
                },
            ) => Self::Unary {
                operand: join(operand, *previous_operand),
                operator,
            },
            (
                Self::Augmented {
                    left,
                    right,
                    operator,
                    context,
                },
                Self::Augmented {
                    left: previous_left,
                    right: previous_right,
                    ..
                },
            ) => Self::Augmented {
                left: join(left, *previous_left),
                right: join(right, *previous_right),
                operator,
                context,
            },
            (
                Self::Call {
                    callable,
                    arguments,
                    tcx,
                },
                Self::Call {
                    callable: previous_callable,
                    arguments: previous_arguments,
                    ..
                },
            ) => {
                let previous_types: Vec<_> = previous_arguments.types(db).collect();
                let arguments = if previous_types.len() == arguments.arguments(db).len() {
                    arguments.map_types(db, |index, ty| join(ty, previous_types[index]))
                } else {
                    arguments
                };
                Self::Call {
                    callable: join(callable, *previous_callable),
                    arguments,
                    tcx,
                }
            }
            (
                Self::Narrow { value, narrowing },
                Self::Narrow {
                    value: previous_value,
                    ..
                },
            ) => Self::Narrow {
                value: join(value, *previous_value),
                narrowing,
            },
            (
                Self::MappingKey { value },
                Self::MappingKey {
                    value: previous_value,
                },
            ) => Self::MappingKey {
                value: join(value, *previous_value),
            },
            (
                Self::MappingValue { value },
                Self::MappingValue {
                    value: previous_value,
                },
            ) => Self::MappingValue {
                value: join(value, *previous_value),
            },
            (
                Self::TypeArgument { value, parameter },
                Self::TypeArgument {
                    value: previous_value,
                    ..
                },
            ) => Self::TypeArgument {
                value: join(value, *previous_value),
                parameter,
            },
            (
                Self::Tuple { elements },
                Self::Tuple {
                    elements: previous_elements,
                },
            ) => {
                let previous_types: Vec<_> = previous_elements.types(db).collect();
                let elements = if previous_types.len() == elements.elements(db).len() {
                    elements.map_types(db, |index, ty| join(ty, previous_types[index]))
                } else {
                    elements
                };
                Self::Tuple { elements }
            }
            (current, _) => current,
        }
    }
}

/// Replaces every unmaterialized marker at the top level of `result` that is also a top-level
/// element of one of `operands` by the opaque marker of its cycle head.
///
/// An operation that passes a marker through unchanged without recording an equation leaves the
/// marker naming the operand rather than the result. The opaque marker names neither, so the
/// solver leaves it alone and cycle recovery treats it like the head's own marker.
pub(crate) fn opaque_passthrough<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    operands: impl IntoIterator<Item = Type<'db>>,
    result: Type<'db>,
) -> Type<'db> {
    let mut passed = FxHashSet::default();
    for operand in operands {
        let single = [operand];
        let elements: &[Type<'db>] = match operand {
            Type::Union(union) => union.elements(db),
            _ => &single,
        };
        for element in elements {
            if let Type::Divergent(marker) = element
                && marker.materialization_kind().is_none()
                && let Some(variable) = marker.variable()
            {
                passed.insert(variable);
            }
        }
    }
    if passed.is_empty() {
        return result;
    }
    let replace = |ty: Type<'db>| match ty {
        Type::Divergent(marker)
            if marker.materialization_kind().is_none()
                && marker
                    .variable()
                    .is_some_and(|variable| passed.contains(&variable)) =>
        {
            Type::Divergent(marker.opaque(db))
        }
        _ => ty,
    };
    match result {
        Type::Union(union) => union.map(db, env, |element| replace(*element)),
        _ => replace(result),
    }
}

/// The operations one query deferred, keyed by the variable standing for each result.
pub(crate) type CycleEquations<'db> = FrozenMap<CycleVariable<'db>, Operation<'db>>;

/// Retains the operations recorded by both cycle iterations.
///
/// An iteration can leave out an operation that an earlier iteration recorded, and the inputs of
/// an operation can change between iterations. Keeping every operation and joining changed inputs
/// makes the table grow monotonically, so the fixed-point iteration converges.
pub(crate) fn cycle_normalized_equations<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    current: CycleEquations<'db>,
    previous: &CycleEquations<'db>,
) -> CycleEquations<'db> {
    if previous.is_empty() {
        return current;
    }
    let mut merged: FxHashMap<_, _> = previous.iter().cloned().collect();
    for (variable, operation) in current {
        let operation = match merged.get(&variable) {
            Some(previous_operation) => operation.cycle_normalized(db, env, previous_operation),
            None => operation,
        };
        merged.insert(variable, operation);
    }
    FrozenMap::from(merged)
}

/// Collects the operations a query defers while it infers types.
///
/// Inference records operations from methods that only read the builder, so the table is
/// interior-mutable.
#[derive(Debug)]
pub(crate) struct DeferredOperations<'db> {
    owner: CycleOwner<'db>,
    equations: RefCell<FxHashMap<CycleVariable<'db>, Operation<'db>>>,
}

impl<'db> DeferredOperations<'db> {
    pub(crate) fn new(owner: CycleOwner<'db>) -> Self {
        Self {
            owner,
            equations: RefCell::new(FxHashMap::default()),
        }
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.equations.borrow().is_empty()
    }

    /// The operations recorded so far.
    pub(crate) fn equations(&self) -> Ref<'_, FxHashMap<CycleVariable<'db>, Operation<'db>>> {
        self.equations.borrow()
    }

    /// Records the operations that `other` has recorded, which must belong to the same owner.
    pub(crate) fn extend_from(&mut self, other: &Self) {
        debug_assert_eq!(self.owner, other.owner);
        let other = other.equations.borrow();
        self.equations.get_mut().extend(
            other
                .iter()
                .map(|(variable, operation)| (*variable, operation.clone())),
        );
    }

    pub(crate) fn extend(
        &mut self,
        equations: impl IntoIterator<Item = (CycleVariable<'db>, Operation<'db>)>,
    ) {
        self.equations.get_mut().extend(equations);
    }

    pub(crate) fn finish(self) -> CycleEquations<'db> {
        FrozenMap::from(self.equations.into_inner())
    }

    /// Records `operation` as the definition of a new variable derived from `input` for `slot`,
    /// and returns the variable's marker.
    pub(crate) fn defer(
        &self,
        db: &'db dyn Db,
        slot: CycleSlot,
        input: CycleVariable<'db>,
        operation: Operation<'db>,
    ) -> Type<'db> {
        let variable = CycleVariable::derived(db, self.owner, slot, input);
        // The operation's own result from an earlier cycle iteration flows back into the
        // operation. The variable stands for the operation applied to its original input, and
        // the current iteration records that equation again: a query retains only the
        // operations its current iteration records.
        let operation = if variable == input
            && let Some(original) = variable.input(db)
        {
            let marker = Type::divergent_variable(variable);
            let original = Type::divergent_variable(original);
            operation
                .map_inputs(db, |ty| Some(if ty == marker { original } else { ty }))
                .unwrap_or(operation)
        } else {
            operation
        };
        self.equations.borrow_mut().insert(variable, operation);
        Type::divergent_variable(variable)
    }

    /// Replaces every cycle marker that an operation passed through unchanged from `operand` to
    /// `result` with the marker of a new variable, defined by the operation applied to that
    /// marker alone.
    ///
    /// Materialized markers already behave like `object` or `Never`, and markers that belong to
    /// no cycle head cannot be resolved; both are left as they are.
    pub(crate) fn defer_passthrough(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        operand: Type<'db>,
        result: Type<'db>,
        slot: CycleSlot,
        operation: impl Fn(Type<'db>) -> Operation<'db>,
    ) -> Type<'db> {
        let single = [operand];
        let operand_elements: &[Type<'db>] = match operand {
            Type::Union(union) => union.elements(db),
            _ => &single,
        };
        let replace = |ty: Type<'db>| match ty {
            Type::Divergent(marker)
                if marker.materialization_kind().is_none() && operand_elements.contains(&ty) =>
            {
                let Some(input) = marker.variable() else {
                    return ty;
                };
                self.defer(db, slot, input, operation(ty))
            }
            _ => ty,
        };
        // The operand can carry the result of an earlier cycle iteration of this operation on
        // this operand, nested in it; the variable's equation is recorded again then.
        self.retain(db, env, slot, operand, &operation);
        match result {
            Type::Union(union) => union.map(db, env, |element| replace(*element)),
            _ => replace(result),
        }
    }

    /// Records again the equations of this owner's variables for `slot` that `ty` mentions at
    /// any depth, directly or through the variables derived from them.
    ///
    /// Such a value feeds the slot's result of an earlier cycle iteration back into the slot.
    /// A query retains only the operations its current iteration records, so the equation is
    /// recorded again, as the operation applied to the variable's original input. `ty` is the
    /// operand that produced the variable, so that the operation is applied to the input in
    /// the operand's position.
    ///
    /// Returns the first such variable whose own marker `ty` mentions nested inside a
    /// constructed type, rather than at its top level. A value that nests its own variable is
    /// a recursive type: `x = [[x]]` nests the previous element inside the new one. Its
    /// representation is the marker itself; embedding the value again would unfold the
    /// recursion by one level on every cycle iteration. A variable reached only through the
    /// input chain of a nested marker, such as the result of a call on the value, is not
    /// returned: the value is then the productive part of a recursive type the solver folds.
    pub(crate) fn retain(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        slot: CycleSlot,
        ty: Type<'db>,
        operation: impl Fn(Type<'db>) -> Operation<'db>,
    ) -> Option<CycleVariable<'db>> {
        self.retain_where(
            db,
            env,
            |variable| variable.owner(db) == self.owner && variable.slot(db) == slot,
            ty,
            operation,
        )
    }

    /// [`Self::retain`] for the variables `is_own` accepts, rather than those of one slot of
    /// this query.
    ///
    /// The element variables of a collection literal are owned by the literal's inference
    /// without a type context, while the literal is also inferred under the declared type of
    /// the definition it is assigned to; both inferences retain those variables.
    pub(crate) fn retain_where(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        is_own: impl Fn(CycleVariable<'db>) -> bool,
        ty: Type<'db>,
        operation: impl Fn(Type<'db>) -> Operation<'db>,
    ) -> Option<CycleVariable<'db>> {
        let single = [ty];
        let elements: &[Type<'db>] = match ty {
            Type::Union(union) => union.elements(db),
            _ => &single,
        };
        let nested = Cell::new(None);
        for element in elements {
            let top_level = matches!(element, Type::Divergent(_));
            any_over_type_for_cycle_markers(db, env, *element, |ty| {
                if let Type::Divergent(marker) = ty
                    && marker.materialization_kind().is_none()
                {
                    let own = marker.variable();
                    let mut variable = own;
                    while let Some(current) = variable {
                        let input = current.input(db);
                        if is_own(current)
                            && let Some(input) = input
                        {
                            self.equations
                                .borrow_mut()
                                .insert(current, operation(Type::divergent_variable(input)));
                            if !top_level && own == Some(current) && nested.get().is_none() {
                                nested.set(Some(current));
                            }
                        }
                        variable = input;
                    }
                }
                false
            });
        }
        nested.get()
    }
}

impl<'db> IntoIterator for DeferredOperations<'db> {
    type Item = (CycleVariable<'db>, Operation<'db>);
    type IntoIter = std::collections::hash_map::IntoIter<CycleVariable<'db>, Operation<'db>>;

    fn into_iter(self) -> Self::IntoIter {
        self.equations.into_inner().into_iter()
    }
}
