use super::{ArgumentMatchResult, ArgumentMatcher, MatchedArgument};
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

/// Returns a candidate mapping covering every combination of tuple members.
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
        || parameters
            .keyword_variadic()
            .is_some_and(|(_, parameter)| parameter.has_starred_annotation())
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

    let mut result = ArgumentMatchResult {
        argument_matches: vec![MatchedArgument::default(); arguments.len()].into_boxed_slice(),
        variadic_argument_matched_to_variadic_parameter: false,
    };
    for arguments in arguments.expand_tuple_unions(db, env)?.iter() {
        let alternative = match_single(db, env, arguments, signature)?;
        result.variadic_argument_matched_to_variadic_parameter |=
            alternative.variadic_argument_matched_to_variadic_parameter;
        // Every combination needs a complete placement. Since the signature is non-generic,
        // the ordinary type checker can check all contributed types together.
        for (combined, argument) in result
            .argument_matches
            .iter_mut()
            .zip(alternative.argument_matches)
        {
            combined.matched = true;
            for parameter in argument.parameters {
                if !combined.parameters.contains(&parameter) {
                    combined.parameters.push(parameter);
                }
            }
        }
    }
    Some(result)
}

/// Matches one combination, searching only when a repeated segment has later positional input.
fn match_single<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    arguments: &CallArguments<'_, 'db>,
    signature: &Signature<'db>,
) -> Option<ArgumentMatchResult<'db>> {
    let parameters = signature.parameters();
    let variadic = parameters.variadic();

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
                let source = argument.source_type()?.resolve_type_alias(db);
                if source.as_paramspec_typevar(db).is_some() {
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

    let mut errors = Vec::new();
    let mut matcher = ArgumentMatcher::new(arguments, parameters, &mut errors);

    // Fixed tuples and trailing homogeneous unpacks use ordinary matching. In particular,
    // a trailing unpack must still check its element type against optional parameters.
    if !segments.windows(2).any(|pair| pair[0].repeated) {
        matcher.match_arguments(db, env);
        let result = matcher.finish(db, env);
        return errors.is_empty().then_some(result);
    }

    let parameter_count = parameters.positional().count();
    // Bound the table's memory and the number of assignability checks for unusually large calls.
    if (segments.len() + 1).checked_mul(parameter_count + 1)? > 65_536 {
        return None;
    }

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
                    || parameter.is_keyword_variadic()
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
