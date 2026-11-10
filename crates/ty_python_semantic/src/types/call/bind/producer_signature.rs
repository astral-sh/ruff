use itertools::Itertools;

use super::producer::{MAX_LOCAL_TYPEVARS, MAX_PRODUCER_RELATIONS};
use super::{Bindings, matched_argument_relations};
use crate::Db;
use crate::types::call::CallArguments;
use crate::types::generics::{GenericContext, TypeVarInference};
use crate::types::visitor::any_over_type_expanding_aliases;
use crate::types::{ProgramEnvironment, Type};

#[derive(Debug)]
pub(super) struct ProducerArgument<'db> {
    pub(super) source_index: Option<usize>,
    pub(super) actual: Type<'db>,
    pub(super) formal: Type<'db>,
}

/// Raw signature and operand types of a successfully resolved generic call.
#[derive(Debug)]
pub(crate) struct ProducerSignature<'db> {
    pub(super) inference: TypeVarInference<'db>,
    pub(super) generic_context: GenericContext<'db>,
    pub(super) return_ty: Type<'db>,
    pub(super) relations: Box<[ProducerArgument<'db>]>,
}

impl<'db> ProducerSignature<'db> {
    pub(crate) fn from_call(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        bindings: &Bindings<'db>,
        arguments: &CallArguments<'_, 'db>,
    ) -> Option<Self> {
        let callable = bindings.single_element()?;
        if callable.overload_call_result.is_some() {
            // Argument expansion and ambiguous results can each involve multiple signatures.
            return None;
        }
        let (_, selected) = callable.selected_overloads().exactly_one().ok()?;
        if let Some(constructor) = bindings.single_item()?.as_constructor()
            && constructor.downstream_constructor.is_some()
        {
            // A downstream initializer can impose additional requirements on this constructor.
            return None;
        }
        let generic_context = selected.signature.generic_context?;
        let return_ty = selected.unspecialized_return_type(db);
        if arguments.len() > MAX_PRODUCER_RELATIONS
            || generic_context.len(db) > MAX_LOCAL_TYPEVARS
            || generic_context
                .variables(db)
                .any(|v| v.is_paramspec(db) || v.is_typevartuple(db))
            || !any_over_type_expanding_aliases(db, env, return_ty, |ty| {
                ty.as_typevar()
                    .is_some_and(|v| generic_context.contains(db, v.identity(db)))
            })
        {
            return None;
        }
        let first_pass = selected.inference?;
        let arguments = arguments.with_self(callable.bound_type);
        let relations: Box<[_]> = matched_argument_relations(
            selected.signature.parameters(),
            &selected.argument_matches,
            &arguments,
        )
        .map(|rel| ProducerArgument {
            source_index: rel.adjusted_argument_index,
            actual: rel.argument_type,
            formal: rel.declared_type,
        })
        .take(MAX_PRODUCER_RELATIONS + 1)
        .collect();
        if relations.len() > MAX_PRODUCER_RELATIONS {
            return None;
        }
        Some(Self {
            inference: first_pass,
            generic_context,
            return_ty,
            relations,
        })
    }
}
