use super::{ArgumentMatchResult, ArgumentMatcher};
use crate::Db;
use crate::ProgramEnvironment;
use crate::types::call::arguments::{CallArgument, CallArguments};
use crate::types::tuple::{TupleSpec, VariableSegment};
use crate::types::visitor::any_over_type;
use crate::types::{Signature, Type};

/// One required value or a homogeneous, possibly empty sequence of values.
struct Segment<'db> {
    argument_index: usize,
    element_type: Option<Type<'db>>,
    repeated: bool,
}

/// Returns a candidate placement of already inferred positional arguments.
///
/// For example, `f(*before, "text", *after)` can match `(int, str, int)` when both
/// unpacks contain integers. A repeated segment can consume the next parameter or end,
/// while a fixed segment must consume exactly one. Dynamic programming finds a complete
/// placement without enumerating combinations of unpack lengths.
///
/// This does not infer arguments or type variables. The caller must type-check the returned
/// mapping before accepting the call. Unsupported calls and unsuccessful searches retain the
/// ordinary matcher's diagnostics.
pub(super) fn match_arguments<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    arguments: &CallArguments<'_, 'db>,
    signature: &Signature<'db>,
) -> Option<ArgumentMatchResult<'db>> {
    let parameters = signature.parameters();
    let variadic = parameters.variadic();
    if signature.generic_context.is_some()
        || !parameters.is_standard()
        || variadic.is_some_and(|(_, parameter)| parameter.has_starred_annotation())
        || parameters.keyword_variadic().is_some()
        || !arguments
            .iter()
            .any(|argument| matches!(argument, CallArgument::Variadic(_)))
    {
        return None;
    }

    // Unresolved argument types and type expressions need contextual inference, which this
    // search cannot provide. In particular, a provisional lambda is not a checked callable.
    if arguments.iter().any(|argument| {
        argument
            .source_type()
            .is_none_or(|ty| ty.has_provisional_marker(db, env))
    }) || parameters.iter().any(|parameter| {
        any_over_type(db, env, parameter.annotated_type(), false, |ty| {
            matches!(ty, Type::TypeForm(_))
        })
    }) {
        return None;
    }

    let mut segments = Vec::new();
    for (argument_index, argument) in arguments.iter().enumerate() {
        let mut push = |element_type, repeated| {
            segments.push(Segment {
                argument_index,
                element_type,
                repeated,
            });
        };
        match argument {
            CallArgument::Synthetic(_) | CallArgument::Positional(_) => push(None, false),
            CallArgument::Variadic(variadic) => {
                // Every member of a union must work; choosing one placement for the merged
                // element types would lose the correlation between its lengths and types.
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
            CallArgument::Keywords(_) => return None,
        }
    }

    // Preserve ordinary checking of a trailing homogeneous unpack, including its element
    // type against optional parameters. Only search when there is later positional input.
    if !segments.windows(2).any(|pair| pair[0].repeated) {
        return None;
    }

    let parameter_count = parameters.positional().count();
    // Bound the table's memory and the number of assignability checks for unusually large calls.
    if (segments.len() + 1).checked_mul(parameter_count + 1)? > 65_536 {
        return None;
    }
    let mut errors = Vec::new();
    let mut matcher = ArgumentMatcher::new(arguments, parameters, &mut errors);

    // can_finish[s][p] means that segments[s..] can fill parameters[p..]. Parameters
    // left after the final segment must have defaults or be supplied by keyword.
    let mut can_finish = vec![vec![false; parameter_count + 1]; segments.len() + 1];
    let terminal = &mut can_finish[segments.len()];
    terminal[parameter_count] =
        parameters
            .iter()
            .enumerate()
            .skip(parameter_count)
            .all(|(index, parameter)| {
                parameter.is_variadic()
                    || parameter.has_default()
                    || matcher.explicit_keyword_parameters.contains(&index)
            });
    for p in (0..parameter_count).rev() {
        terminal[p] = terminal[p + 1]
            && (parameters[p].has_default() || matcher.explicit_keyword_parameters.contains(&p));
    }
    for (s, segment) in segments.iter().enumerate().rev() {
        let argument = arguments.get(segment.argument_index)?;
        let matches_parameter = |expected| {
            argument
                .matched_type(expected, segment.element_type)
                .map(|actual| actual.is_assignable_to(db, env, expected))
        };
        for p in (0..=parameter_count).rev() {
            if p == parameter_count
                && let Some((_, parameter)) = variadic
            {
                // At *args, one comparison covers a whole homogeneous segment. Even if
                // the unpack could be empty, its element type must match the parameter.
                can_finish[s][p] =
                    can_finish[s + 1][p] && matches_parameter(parameter.annotated_type())?;
                continue;
            }
            can_finish[s][p] = (segment.repeated && can_finish[s + 1][p])
                || (p < parameter_count
                    && !matcher.explicit_keyword_parameters.contains(&p)
                    && can_finish[if segment.repeated { s } else { s + 1 }][p + 1]
                    && matches_parameter(parameters[p].annotated_type())?);
        }
    }
    if !can_finish[0][0] {
        return None;
    }

    // Follow one successful path and let the ordinary matcher record the chosen positions.
    let mut p = 0;
    for (s, segment) in segments.iter().enumerate() {
        loop {
            let at_variadic = p == parameter_count && variadic.is_some();
            if segment.repeated && !at_variadic && can_finish[s + 1][p] {
                break;
            }
            matcher
                .match_positional(
                    segment.argument_index,
                    arguments.get(segment.argument_index)?.kind(),
                    segment.element_type,
                    segment.repeated,
                )
                .ok()?;
            if at_variadic {
                break;
            }
            p += 1;
            if !segment.repeated {
                break;
            }
        }
    }
    for (index, argument) in arguments.iter().enumerate() {
        if let CallArgument::Keyword { name, .. } = argument {
            matcher
                .match_keyword(index, argument.kind(), None, name)
                .ok()?;
        }
        // A successful placement can leave an unpack empty, with no parameter matches.
        matcher.argument_matches[index].matched = true;
    }
    let result = matcher.finish(db, env);
    errors.is_empty().then_some(result)
}
