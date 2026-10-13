use rustc_hash::FxHashSet;

use super::{InvalidArgumentTypeProvenance, MatchedArgument, MatchedParameter};
use crate::Db;
use crate::ProgramEnvironment;
use crate::types::call::arguments::{CallArgument, CallArguments};
use crate::types::constraints::{ConstraintSet, ConstraintSetBuilder};
use crate::types::tuple::{TupleSpec, VariableSegment};
use crate::types::typevar::TypeVarSet;
use crate::types::{Parameters, Type};

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

/// Possible ordered bindings of positional arguments to a fixed positional signature.
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
    parameter_count: usize,
    can_consume: Box<[bool]>,
    can_finish: Box<[bool]>,
    pub(super) arguments: Box<[MatchedArgument<'db>]>,
}

pub(super) struct PositionalEvaluation<'db, 'c> {
    pub(super) constraints: Option<ConstraintSet<'db, 'c>>,
    pub(super) arguments: Box<[MatchedArgument<'db>]>,
}

impl<'db> PositionalMatches<'db> {
    /// Builds the possible placements from argument lengths, before checking their types.
    ///
    /// Returns `None` when the ordinary matcher should handle the call, including unsupported
    /// unpacking forms and argument counts that cannot fit. The initial matches retain every
    /// possible parameter for argument inference; [`Self::evaluate`] then checks complete placements.
    pub(super) fn new(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        arguments: &CallArguments<'_, 'db>,
        parameters: &Parameters<'db>,
        explicit_keywords: &FxHashSet<usize>,
    ) -> Option<Self> {
        if parameters.variadic().is_some()
            || !arguments
                .iter()
                .any(|argument| matches!(argument, CallArgument::Variadic(_)))
        {
            return None;
        }

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
        let width = parameter_count + 1;
        let can_consume = (0..parameter_count)
            .map(|index| !explicit_keywords.contains(&index))
            .collect::<Box<[_]>>();
        let mut can_finish = vec![false; (segments.len() + 1) * width];
        can_finish[segments.len() * width + parameter_count] = true;
        for position in (0..parameter_count).rev() {
            can_finish[segments.len() * width + position] = can_finish
                [segments.len() * width + position + 1]
                && (parameters[position].has_default() || explicit_keywords.contains(&position));
        }
        for (index, segment) in segments.iter().enumerate().rev() {
            for position in (0..=parameter_count).rev() {
                let skip = segment.repeated && can_finish[(index + 1) * width + position];
                let next = if segment.repeated { index } else { index + 1 };
                let consume = position < parameter_count
                    && can_consume[position]
                    && can_finish[next * width + position + 1];
                can_finish[index * width + position] = skip || consume;
            }
        }
        if !can_finish[0] {
            // The ordinary matcher provides arity and duplicate-argument diagnostics when
            // even the argument shapes cannot fit this signature.
            return None;
        }

        let mut matches = Self {
            segments: segments.into_boxed_slice(),
            parameter_count,
            can_consume,
            can_finish: can_finish.into_boxed_slice(),
            arguments: Self::empty_matches(arguments),
        };
        let mut reachable = vec![false; matches.can_finish.len()];
        reachable[0] = true;
        for (index, segment) in matches.segments.iter().enumerate() {
            for position in 0..=parameter_count {
                if !reachable[index * width + position] {
                    continue;
                }
                if segment.repeated && matches.can_finish[(index + 1) * width + position] {
                    reachable[(index + 1) * width + position] = true;
                }
                let next = if segment.repeated { index } else { index + 1 };
                if position < parameter_count
                    && matches.can_consume[position]
                    && matches.can_finish[next * width + position + 1]
                {
                    reachable[next * width + position + 1] = true;
                    segment.add_match(&mut matches.arguments, position);
                }
            }
        }
        Self::deduplicate_matches(&mut matches.arguments);
        Some(matches)
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

    /// Whether a non-unpacked argument has more than one possible contextual parameter type.
    pub(super) fn has_ambiguous_argument_contexts(&self) -> bool {
        self.segments.iter().any(|segment| {
            segment.argument_type.is_none()
                && self.arguments[segment.argument_index].parameters.len() > 1
        })
    }

    /// Checks argument types across complete placements, keeping each placement's constraints
    /// together when combining alternatives.
    ///
    /// Returns positional constraints and matches that can participate in a valid complete
    /// placement. Keyword arguments filter these matches, but the caller still infers their
    /// constraints separately.
    /// On failure, returns one complete placement for diagnostics and `None` for the constraints.
    pub(super) fn evaluate<'c>(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        constraints: &'c ConstraintSetBuilder<'db>,
        arguments: &CallArguments<'_, 'db>,
        parameters: &Parameters<'db>,
        inferable: TypeVarSet<'db>,
    ) -> PositionalEvaluation<'db, 'c> {
        let width = self.parameter_count + 1;
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
        let mut edges = vec![never; self.segments.len() * width];
        for (index, segment) in self.segments.iter().enumerate() {
            for position in 0..self.parameter_count {
                if !self.can_finish[index * width + position] || !self.can_consume[position] {
                    continue;
                }
                let expected = parameters[position].annotated_type();
                edges[index * width + position] = arguments
                    .get(segment.argument_index)
                    .and_then(|argument| argument.matched_type(expected, segment.argument_type))
                    .map_or(always, |actual| argument_constraints(actual, expected));
            }
        }

        let mut suffix = vec![never; self.can_finish.len()];
        for position in 0..=self.parameter_count {
            suffix[self.segments.len() * width + position] = ConstraintSet::from_bool(
                constraints,
                self.can_finish[self.segments.len() * width + position],
            );
        }
        for (index, segment) in self.segments.iter().enumerate().rev() {
            for position in (0..=self.parameter_count).rev() {
                let mut condition = if segment.repeated {
                    suffix[(index + 1) * width + position]
                } else {
                    never
                };
                if position < self.parameter_count && self.can_consume[position] {
                    let next = if segment.repeated { index } else { index + 1 };
                    condition.union(
                        db,
                        constraints,
                        edges[index * width + position]
                            .and(db, constraints, || suffix[next * width + position + 1]),
                    );
                }
                suffix[index * width + position] = condition;
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
            return PositionalEvaluation {
                constraints: None,
                arguments: self.best_failing_matches(arguments, &edges, possible),
            };
        }

        let mut matched = Self::empty_matches(arguments);
        let mut prefix = vec![never; self.can_finish.len()];
        prefix[0] = common;
        for (index, segment) in self.segments.iter().enumerate() {
            for position in 0..=self.parameter_count {
                let previous = prefix[index * width + position];
                if previous.is_trivially_never_satisfied() {
                    continue;
                }
                if segment.repeated {
                    prefix[(index + 1) * width + position].union(db, constraints, previous);
                }
                if position < self.parameter_count && self.can_consume[position] {
                    let next = if segment.repeated { index } else { index + 1 };
                    let condition =
                        previous.and(db, constraints, || edges[index * width + position]);
                    if possible(
                        condition.and(db, constraints, || suffix[next * width + position + 1]),
                    ) {
                        segment.add_match(&mut matched, position);
                        prefix[next * width + position + 1].union(db, constraints, condition);
                    }
                }
            }
        }
        Self::deduplicate_matches(&mut matched);
        PositionalEvaluation {
            constraints: Some(suffix[0]),
            arguments: matched,
        }
    }

    /// Selects a complete placement with the fewest incompatible argument/parameter pairs,
    /// ending the current unpack sooner when costs tie. The caller validates that placement to
    /// report errors, rather than combining diagnostics from mutually exclusive placements.
    fn best_failing_matches<'c>(
        &self,
        arguments: &CallArguments<'_, 'db>,
        edges: &[ConstraintSet<'db, 'c>],
        possible: impl Fn(ConstraintSet<'db, 'c>) -> bool,
    ) -> Box<[MatchedArgument<'db>]> {
        let width = self.parameter_count + 1;
        let mut costs = vec![usize::MAX; self.can_finish.len()];
        for position in 0..=self.parameter_count {
            if self.can_finish[self.segments.len() * width + position] {
                costs[self.segments.len() * width + position] = 0;
            }
        }
        for (index, segment) in self.segments.iter().enumerate().rev() {
            for position in (0..=self.parameter_count).rev() {
                let mut cost = if segment.repeated {
                    costs[(index + 1) * width + position]
                } else {
                    usize::MAX
                };
                if position < self.parameter_count && self.can_consume[position] {
                    let next = if segment.repeated { index } else { index + 1 };
                    cost = cost.min(
                        costs[next * width + position + 1].saturating_add(usize::from(!possible(
                            edges[index * width + position],
                        ))),
                    );
                }
                costs[index * width + position] = cost;
            }
        }

        let mut matched = Self::empty_matches(arguments);
        let (mut index, mut position) = (0, 0);
        while let Some(segment) = self.segments.get(index) {
            if segment.repeated
                && costs[index * width + position] == costs[(index + 1) * width + position]
            {
                index += 1;
            } else if position < self.parameter_count && self.can_consume[position] {
                segment.add_match(&mut matched, position);
                position += 1;
                if !segment.repeated {
                    index += 1;
                }
            } else {
                break;
            }
        }
        Self::deduplicate_matches(&mut matched);
        matched
    }
}
