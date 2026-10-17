use rustc_hash::FxHashSet;

use super::{InvalidArgumentTypeProvenance, MatchedArgument, MatchedParameter};
use crate::Db;
use crate::ProgramEnvironment;
use crate::types::call::arguments::{CallArgument, CallArguments};
use crate::types::constraints::{ConstraintSet, ConstraintSetBuilder};
use crate::types::tuple::{TupleSpec, VariableSegment};
use crate::types::typevar::TypeVarSet;
use crate::types::visitor::any_over_type;
use crate::types::{Parameters, Type};

/// Bound the dense tables used for matching and constraint propagation. Larger calls use the
/// ordinary matcher, whose memory use does not grow with the product of both call dimensions.
const MAX_POSITIONAL_STATES: usize = 65_536;

/// One required value or a homogeneous, possibly empty sequence of values.
#[derive(Clone, Copy, Debug)]
struct Segment<'db> {
    argument_index: usize,
    argument_type: Option<Type<'db>>,
    repeated: bool,
}

impl<'db> Segment<'db> {
    fn add_match(self, arguments: &mut [MatchedArgument<'db>], parameter_index: usize) {
        arguments[self.argument_index]
            .parameters
            .push(MatchedParameter {
                index: parameter_index,
                argument_type: self.argument_type,
                expected_type: None,
                provenance: InvalidArgumentTypeProvenance::Argument,
            });
    }
}

/// Possible ordered bindings of positional arguments to a positional signature.
///
/// Each state identifies an argument segment and the next parameter. A repeated segment can
/// either finish or consume one parameter without advancing to the next segment. Keeping these
/// choices until argument types are available allows `f(*before, text, *after)` to place `text`
/// at a compatible parameter without enumerating combinations of unpack lengths.
///
/// ```python
/// def f(a: int, text: str, b: int): ...
/// def call(before: list[int], after: list[int]):
///     f(*before, "text", *after)
/// ```
#[derive(Clone, Debug)]
pub(super) struct PositionalMatches<'db> {
    segments: Box<[Segment<'db>]>,
    /// The number of parameters before an optional variadic parameter.
    parameter_count: usize,
    variadic: bool,
    /// A parameter already supplied by keyword cannot also accept a positional argument.
    can_consume: Box<[bool]>,
    /// From each (segment, parameter) state, can the rest of the call fit this signature?
    /// This checks argument counts only; `evaluate` checks the types later.
    can_finish: Box<[bool]>,
    pub(super) arguments: Box<[MatchedArgument<'db>]>,
    pub(super) variadic_argument_matched: bool,
}

pub(super) struct PositionalEvaluation<'db, 'c> {
    pub(super) constraints: ConstraintSet<'db, 'c>,
    pub(super) arguments: Box<[MatchedArgument<'db>]>,
    pub(super) variadic_argument_matched: bool,
}

impl<'db> PositionalMatches<'db> {
    /// Whether argument types are available without provisional inference or `TypeForm` context.
    pub(super) fn supports_arguments(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        arguments: &CallArguments<'_, 'db>,
        parameters: &Parameters<'db>,
    ) -> bool {
        arguments.iter().all(|argument| {
            argument.source_type().is_some_and(|ty| !ty.has_provisional_marker(db, env))
        })
            // A name or string literal can become a type expression in a `TypeForm` context.
            && !parameters.iter().any(|parameter| {
                any_over_type(db, env, parameter.annotated_type(), false, |ty| {
                    matches!(ty, Type::TypeForm(_))
                })
            })
    }

    /// Builds the possible placements from argument lengths, before checking their types.
    ///
    /// Returns `None` when the ordinary matcher should handle the call, including unsupported
    /// unpacking forms and argument counts that cannot fit. The initial matches describe possible
    /// parameters based on arity; [`Self::evaluate`] checks type compatibility for complete placements.
    pub(super) fn new(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        arguments: &CallArguments<'_, 'db>,
        parameters: &Parameters<'db>,
        explicit_keywords: &FxHashSet<usize>,
    ) -> Option<Self> {
        if !Self::supports_arguments(db, env, arguments, parameters)
            || parameters
                .variadic()
                .is_some_and(|(_, parameter)| parameter.has_starred_annotation())
            || !arguments
                .iter()
                .any(|argument| matches!(argument, CallArgument::Variadic(_)))
        {
            return None;
        }

        // Split tuple unpacks into their fixed elements and, if present, one repeated element.
        // For example, *tuple[int, *tuple[str, ...], bool] has three such segments. Keep the
        // original argument index so that we can give its possible parameter types to inference.
        let mut segments = Vec::new();
        for (argument_index, argument) in arguments.iter().enumerate() {
            let mut push = |argument_type, repeated| {
                segments.push(Segment {
                    argument_index,
                    argument_type,
                    repeated,
                });
            };
            match argument {
                CallArgument::Synthetic(_) | CallArgument::Positional(_) => push(None, false),
                CallArgument::Variadic(variadic) => {
                    // A known union is universally quantified, unlike the possible lengths of
                    // one iterable. Keep its members on the existing argument-expansion path.
                    let source = argument.source_type()?.resolve_type_alias(db);
                    if source.is_union() || source.as_paramspec_typevar(db).is_some() {
                        return None;
                    }
                    match variadic.sequence(db, env)?.as_ref() {
                        TupleSpec::Fixed(tuple) => {
                            for ty in tuple.iter_all_elements() {
                                push(Some(ty), false);
                            }
                        }
                        TupleSpec::Variable(tuple) => {
                            let VariableSegment::Homogeneous(element) = tuple.variable() else {
                                return None;
                            };
                            for ty in tuple.prefix_elements() {
                                push(Some(*ty), false);
                            }
                            push(Some(element), true);
                            for ty in tuple.suffix_elements() {
                                push(Some(*ty), false);
                            }
                        }
                    }
                }
                CallArgument::Keyword { .. } => {}
                // The parameters supplied by an unknown keyword unpack can depend on where
                // positional unpacking stops. Preserve the existing handling of that case.
                CallArgument::Keywords(_) => return None,
            }
        }

        // With no later positional input, preserve the existing treatment of a single
        // homogeneous unpack, including checking its elements against defaulted parameters.
        if !segments
            .iter()
            .take(segments.len().saturating_sub(1))
            .any(|segment| segment.repeated)
        {
            return None;
        }

        let parameter_count = parameters.positional().count();
        let state_count = (segments.len() + 1).checked_mul(parameter_count + 1)?;
        if state_count > MAX_POSITIONAL_STATES {
            return None;
        }
        let variadic = parameters.variadic().is_some();
        // Explicit keywords occupy their parameter even when an earlier unpack could reach it.
        let can_consume = (0..parameter_count + usize::from(variadic))
            .map(|index| !explicit_keywords.contains(&index))
            .collect::<Box<[_]>>();
        let mut matches = Self {
            can_finish: vec![false; state_count].into_boxed_slice(),
            segments: segments.into_boxed_slice(),
            parameter_count,
            variadic,
            can_consume,
            arguments: Self::empty_matches(arguments),
            variadic_argument_matched: false,
        };
        matches.mark_possible_finishes(parameters);
        if !matches.can_finish[0] {
            // Let the ordinary matcher diagnose impossible arities or duplicate arguments.
            return None;
        }
        matches.collect_possible_matches();
        Some(matches)
    }

    /// Indexes the same (segment, next parameter) state in each of the algorithm's tables.
    /// The final row means all segments are exhausted; the final column means no fixed
    /// positional parameters remain.
    fn state(&self, segment: usize, parameter: usize) -> usize {
        segment * (self.parameter_count + 1) + parameter
    }

    /// Match one shared choice of unpack lengths without changing the source argument indexes.
    /// A repeated segment that reaches a variadic parameter still checks its element type, even
    /// when its chosen length leaves no values for that parameter.
    pub(super) fn at_lengths(
        &self,
        arguments: &CallArguments<'_, 'db>,
        lengths: &[usize],
    ) -> Option<(Box<[MatchedArgument<'db>]>, bool)> {
        let mut matched = Self::empty_matches(arguments);
        let mut position = 0;
        let mut variadic_argument_matched = false;
        for (index, segment) in self.segments.iter().enumerate() {
            if segment.repeated {
                let mut remaining = *lengths.get(segment.argument_index)?;
                while remaining > 0 && position < self.parameter_count {
                    if !self.can_consume[position] {
                        return None;
                    }
                    segment.add_match(&mut matched, position);
                    position += 1;
                    remaining -= 1;
                }
                if self.variadic && position == self.parameter_count {
                    segment.add_match(&mut matched, position);
                    variadic_argument_matched = true;
                } else if remaining > 0 {
                    return None;
                }
            } else {
                let next = self.next_state(index, position)?;
                segment.add_match(&mut matched, position);
                position = next % (self.parameter_count + 1);
            }
        }
        Self::deduplicate_matches(&mut matched);
        self.can_finish[self.state(self.segments.len(), position)]
            .then_some((matched, variadic_argument_matched))
    }

    /// Whether an unpack can stop before matching the current parameter.
    ///
    /// At a variadic parameter, we check its element type even if the iterable might be empty:
    ///
    /// ```python
    /// def ints(*args: int): ...
    /// def call(strings: list[str]):
    ///     ints(*strings, 1)  # str is not assignable to int
    /// ```
    fn can_skip(&self, index: usize, position: usize) -> bool {
        self.segments[index].repeated && !(self.variadic && position == self.parameter_count)
    }

    /// Returns the next state after consuming a value, or `None` if the parameter cannot accept it
    /// positionally. At a variadic parameter, one comparison covers the entire repeated segment;
    /// advancing past that segment keeps the state graph acyclic.
    fn next_state(&self, index: usize, position: usize) -> Option<usize> {
        if !self.can_consume.get(position).copied().unwrap_or(false) {
            return None;
        }
        if position == self.parameter_count {
            Some(self.state(index + 1, position))
        } else {
            let next_index = if self.segments[index].repeated {
                index
            } else {
                index + 1
            };
            Some(self.state(next_index, position + 1))
        }
    }

    /// Records whether the remaining arguments can fill the remaining required parameters.
    fn mark_possible_finishes(&mut self, parameters: &Parameters<'db>) {
        // With no segments left, we can stop if all remaining parameters have defaults or
        // were supplied by keyword. An omitted required parameter prevents every earlier stop.
        let last_row = self.state(self.segments.len(), 0);
        self.can_finish[last_row + self.parameter_count] = true;
        for position in (0..self.parameter_count).rev() {
            let already_supplied = !self.can_consume[position];
            self.can_finish[last_row + position] = self.can_finish[last_row + position + 1]
                && (parameters[position].has_default() || already_supplied);
        }

        // Work backward: every transition goes right to a later parameter or down to a later
        // segment. A repeated segment may stop here, or supply one more value. It can finish
        // only if at least one of those choices leads to a complete call.
        for index in (0..self.segments.len()).rev() {
            for position in (0..=self.parameter_count).rev() {
                let current = self.state(index, position);
                let after_segment = self.state(index + 1, position);
                let stop_unpack = self.can_skip(index, position) && self.can_finish[after_segment];
                let consume = self
                    .next_state(index, position)
                    .is_some_and(|next| self.can_finish[next]);
                self.can_finish[current] = stop_unpack || consume;
            }
        }
    }

    /// Collects each argument's possible parameter types before inferring the arguments.
    fn collect_possible_matches(&mut self) {
        // A state that can finish is useful only if we can reach it from the start of this call.
        // Retain a match only when it belongs to at least one complete placement.
        let mut reachable = vec![false; self.can_finish.len()];
        reachable[0] = true;
        for (index, segment) in self.segments.iter().enumerate() {
            for position in 0..=self.parameter_count {
                if !reachable[self.state(index, position)] {
                    continue;
                }
                let after_segment = self.state(index + 1, position);
                if self.can_skip(index, position) && self.can_finish[after_segment] {
                    reachable[after_segment] = true;
                }
                if let Some(next) = self.next_state(index, position)
                    && self.can_finish[next]
                {
                    reachable[next] = true;
                    segment.add_match(&mut self.arguments, position);
                    self.variadic_argument_matched |=
                        segment.repeated && position == self.parameter_count;
                }
            }
        }
        Self::deduplicate_matches(&mut self.arguments);
    }

    fn empty_matches(arguments: &CallArguments<'_, 'db>) -> Box<[MatchedArgument<'db>]> {
        arguments
            .iter()
            .map(|argument| MatchedArgument {
                matched: matches!(
                    argument,
                    CallArgument::Synthetic(_)
                        | CallArgument::Positional(_)
                        | CallArgument::Variadic(_)
                ),
                ..MatchedArgument::default()
            })
            .collect()
    }

    fn deduplicate_matches(arguments: &mut [MatchedArgument<'db>]) {
        let mut seen = FxHashSet::default();
        for argument in arguments {
            if argument.parameters.len() > 1 {
                seen.clear();
                argument
                    .parameters
                    .retain(|parameter| seen.insert((parameter.index, parameter.argument_type)));
            }
        }
    }

    /// Checks argument types across complete placements, keeping each placement's constraints
    /// together when combining alternatives.
    ///
    /// Returns positional constraints and matches that can participate in a valid complete
    /// placement. Keyword arguments filter these matches, but the caller still infers their
    /// constraints separately.
    /// Returns `None` when the ordinary matcher should diagnose the call.
    pub(super) fn evaluate<'c>(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        constraints: &'c ConstraintSetBuilder<'db>,
        arguments: &CallArguments<'_, 'db>,
        parameters: &Parameters<'db>,
        inferable: TypeVarSet<'db>,
    ) -> Option<PositionalEvaluation<'db, 'c>> {
        let never = ConstraintSet::from_bool(constraints, false);
        let always = ConstraintSet::from_bool(constraints, true);
        // Keep inference evidence while allowing only this call's type variables to be solved.
        // The lazy relation alone could specialize an unrelated type variable from the caller.
        let argument_constraints = |actual: Type<'db>, expected: Type<'db>| {
            actual
                .when_assignable_to(db, env, expected, constraints, inferable)
                .and(db, constraints, || {
                    actual.when_constraint_set_assignable_to(db, env, expected, constraints)
                })
        };
        // An edge's constraints describe assigning this segment's element type to this
        // parameter. Compute them once; different unpack lengths may use the same edge.
        let mut edges = vec![never; self.state(self.segments.len(), 0)];
        for (index, segment) in self.segments.iter().enumerate() {
            for position in 0..=self.parameter_count {
                let current = self.state(index, position);
                if !self.can_finish[current] || self.next_state(index, position).is_none() {
                    continue;
                }
                let expected = parameters[position].annotated_type();
                edges[current] = arguments
                    .get(segment.argument_index)
                    .and_then(|argument| argument.matched_type(expected, segment.argument_type))
                    .map_or(always, |actual| argument_constraints(actual, expected));
            }
        }

        // For each state, build the constraints for completing the rest of the call. Intersect
        // along a placement (all arguments must fit) and union alternatives (any length may fit).
        let mut suffix = vec![never; self.can_finish.len()];
        for position in 0..=self.parameter_count {
            let terminal = self.state(self.segments.len(), position);
            suffix[terminal] = ConstraintSet::from_bool(constraints, self.can_finish[terminal]);
        }
        for index in (0..self.segments.len()).rev() {
            for position in (0..=self.parameter_count).rev() {
                let current = self.state(index, position);
                let mut condition = if self.can_skip(index, position) {
                    suffix[self.state(index + 1, position)]
                } else {
                    never
                };
                if let Some(next) = self.next_state(index, position) {
                    condition.union(
                        db,
                        constraints,
                        edges[current].and(db, constraints, || suffix[next]),
                    );
                }
                suffix[current] = condition;
            }
        }

        // Keyword arguments constrain the same type variables, so they must participate when
        // deciding which positional placements remain possible.
        let mut common = always;
        for argument in arguments.iter() {
            if let CallArgument::Keyword { name, .. } = argument
                && let Some((_, parameter)) = parameters.keyword_by_name(name)
                && let Some(actual) = argument.matched_type(parameter.annotated_type(), None)
            {
                common.intersect(
                    db,
                    constraints,
                    argument_constraints(actual, parameter.annotated_type()),
                );
            }
        }
        let possible = |condition: ConstraintSet<'db, 'c>| {
            !condition.is_never_satisfied(db, env, inferable)
                && !condition.has_no_valid_solutions(db, env)
        };
        if !possible(common.and(db, constraints, || suffix[0])) {
            return None;
        }

        // Keep an individual match only if both the arguments before it and those after it can
        // satisfy the same constraints. Testing an edge alone would mix incompatible placements.
        let mut matched = Self::empty_matches(arguments);
        let mut variadic_argument_matched = false;
        let mut prefix = vec![never; self.can_finish.len()];
        prefix[0] = common;
        for (index, segment) in self.segments.iter().enumerate() {
            for position in 0..=self.parameter_count {
                let current = self.state(index, position);
                let previous = prefix[current];
                if previous.is_trivially_never_satisfied() {
                    continue;
                }
                if self.can_skip(index, position) {
                    prefix[self.state(index + 1, position)].union(db, constraints, previous);
                }
                if let Some(next) = self.next_state(index, position) {
                    let condition = previous.and(db, constraints, || edges[current]);
                    if possible(condition.and(db, constraints, || suffix[next])) {
                        segment.add_match(&mut matched, position);
                        variadic_argument_matched |=
                            segment.repeated && position == self.parameter_count;
                        prefix[next].union(db, constraints, condition);
                    }
                }
            }
        }
        Self::deduplicate_matches(&mut matched);
        Some(PositionalEvaluation {
            constraints: suffix[0],
            arguments: matched,
            variadic_argument_matched,
        })
    }
}
