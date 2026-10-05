//! Construct inferable-variable sets from a borrowed canonical generic context.

use std::iter::{Copied, Peekable};

use salsa::execution_probe::{
    ExecutionWork, FieldReadProfile, FieldReturnMode, NativeValueQuote, RunError, RunResult,
    TaskEndpoint,
};

use super::class_selection::FixedFieldCopy;
use super::storage::{StorageQuote, dense_finish, ordered_merge, slots, table_merge};
use super::{SourceAccess, SourceEffects};
use crate::FxOrderMap;
use crate::types::generics::GenericContext;
use crate::types::typevar::construction::{
    TypeVarSetConstructionEffects, TypeVarSetVariables, typevar_set_from_typevars_with,
};
use crate::types::typevar::{TypeVarSet, TypeVarSetInner};
use crate::types::{BoundTypeVarIdentity, BoundTypeVarInstance};

pub(in crate::types) type TypeVarSetInput<'db> = Peekable<
    Copied<ordermap::map::Values<'db, BoundTypeVarIdentity<'db>, BoundTypeVarInstance<'db>>>,
>;

type Entry<'db> = (BoundTypeVarIdentity<'db>, BoundTypeVarInstance<'db>);
type OrderedEntry<'db> = (usize, Entry<'db>);

/// Quotes borrowing the context's canonical map without copying or visiting its entries.
#[derive(Debug)]
struct VariablesBorrow;

impl<'variables> FieldReadProfile<TypeVarSetVariables<'variables>> for VariablesBorrow {
    async fn quote<'call, 'run: 'call, 'db: 'run>(
        &'call self,
        endpoint: &'call TaskEndpoint<'run, 'db>,
        _stored: &'call TypeVarSetVariables<'variables>,
        mode: FieldReturnMode,
    ) -> RunResult<NativeValueQuote> {
        Ok(endpoint
            .local_call(|| {
                endpoint.admit_work(2)?;
                endpoint.admit(ExecutionWork::Resource {
                    requested_bytes: size_of::<NativeValueQuote>()
                        + size_of::<RunResult<NativeValueQuote>>(),
                })?;
                endpoint.check_completion()?;
                if mode != FieldReturnMode::Ref {
                    return Err(RunError::Contract(
                        "typevar set input requires a borrowed map",
                    ));
                }
                Ok(NativeValueQuote {
                    work: 1,
                    requested_bytes: size_of::<&TypeVarSetVariables<'_>>(),
                    cleanup_work: 0,
                })
            })
            .await)
    }
}

/// Quotes ordered lookup, possible growth, entry transfer, and newly acquired cleanup work.
fn insertion_quote(len: usize, capacity: usize) -> Option<StorageQuote> {
    let mut quote = ordered_merge::<Entry<'_>>(len, capacity, 1)?;
    let required = len.checked_add(1)?;
    // Each new entry prepays its disposal. Previously retained entries keep that prepayment
    // when growth moves them; only the replacement backing acquires a new cleanup obligation.
    quote.work = quote.work.checked_add(9)?;
    quote.bytes = quote.bytes.checked_add(size_of::<OrderedEntry<'_>>())?;
    if required > capacity {
        let (_, replacement_slots) = table_merge::<usize>(len, capacity, 1, 0)?;
        quote.work = quote.work.checked_add(replacement_slots)?.checked_add(1)?;
        quote.bytes = quote
            .bytes
            .checked_add(len.checked_mul(size_of::<OrderedEntry<'_>>())?)?;
    }
    Some(quote)
}

/// Quotes shrinking both ordered entries and their index table while retaining their order.
fn shrink_quote(len: usize, capacity: usize) -> Option<StorageQuote> {
    let entries = dense_finish::<OrderedEntry<'_>>(len, len)?;
    let old_slots = slots(capacity)?;
    let replacement_slots = slots(len)?;
    let table = StorageQuote {
        work: old_slots
            .checked_add(replacement_slots.checked_mul(2)?)?
            .checked_add(len.checked_mul(8)?)?
            .checked_add(5)?,
        bytes: replacement_slots
            .checked_mul(size_of::<usize>().checked_add(1)?)?
            .checked_add(len.checked_mul(size_of::<OrderedEntry<'_>>())?)?,
    };
    // The ordered vector relocates only initialized entries; its old allocation's disposal
    // was prepaid. The index table can scan old slots while shrinking. The second
    // replacement-slot term prepays only the new backing, preserving existing entry credit.
    entries.checked_add(table)
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> SourceEffects<'_, 'run, 'db, A> {
    /// Constructs the inferable set from the variables directly bound by `context`.
    pub(in crate::types::infer::builder) async fn inferable_typevars(
        &self,
        context: GenericContext<'db>,
    ) -> RunResult<TypeVarSet<'db>> {
        let variables = self
            .field_with_profile(
                context.variables_request(self.access.endpoint().field_request_context()),
                &VariablesBorrow,
            )
            .await?;
        let input = self
            .local_with_fixed_transfers(3, size_of::<TypeVarSetInput<'db>>(), || {
                variables.values().copied().peekable()
            })
            .await?;
        self.allocate_future(|| typevar_set_from_typevars_with(input, self))
            .await?
            .await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> TypeVarSetConstructionEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;
    type Input = TypeVarSetInput<'db>;

    async fn is_empty(&self, input: &mut Self::Input) -> RunResult<bool> {
        self.local_with_fixed_transfers(3, size_of::<Option<BoundTypeVarInstance<'db>>>(), || {
            input.peek().is_none()
        })
        .await
    }

    async fn empty(&self) -> RunResult<TypeVarSet<'db>> {
        self.local_with_fixed_transfers(1, size_of::<TypeVarSet<'db>>(), || TypeVarSet::None)
            .await
    }

    async fn new_variables(&self) -> RunResult<TypeVarSetVariables<'db>> {
        self.local_with_fixed_transfers(
            2,
            size_of::<TypeVarSetVariables<'db>>(),
            FxOrderMap::default,
        )
        .await
    }

    async fn next_variable(
        &self,
        input: &mut Self::Input,
    ) -> RunResult<Option<BoundTypeVarInstance<'db>>> {
        self.local_with_fixed_transfers(2, size_of::<Option<BoundTypeVarInstance<'db>>>(), || {
            input.next()
        })
        .await
    }

    async fn identity(
        &self,
        variable: BoundTypeVarInstance<'db>,
    ) -> RunResult<BoundTypeVarIdentity<'db>> {
        self.field_with_profile(
            variable.identity_request(self.access.endpoint().field_request_context()),
            &FixedFieldCopy,
        )
        .await
    }

    async fn insert(
        &self,
        variables: &mut TypeVarSetVariables<'db>,
        identity: BoundTypeVarIdentity<'db>,
        variable: BoundTypeVarInstance<'db>,
    ) -> RunResult<()> {
        let quote = self
            .local_with_fixed_transfers(96, size_of::<Option<StorageQuote>>(), || {
                insertion_quote(variables.len(), variables.capacity())
            })
            .await?
            .ok_or(RunError::Contract(
                "typevar set insertion quotation overflow",
            ))?;
        self.local_with_fixed_transfers(quote.work, quote.bytes, || {
            variables.entry(identity).or_insert(variable);
        })
        .await
    }

    async fn shrink(&self, variables: &mut TypeVarSetVariables<'db>) -> RunResult<()> {
        let quote = self
            .local_with_fixed_transfers(48, size_of::<Option<StorageQuote>>(), || {
                shrink_quote(variables.len(), variables.capacity())
            })
            .await?
            .ok_or(RunError::Contract(
                "typevar set shrinking quotation overflow",
            ))?;
        self.local_with_fixed_transfers(quote.work, quote.bytes, || variables.shrink_to_fit())
            .await
    }

    async fn intern(&self, variables: TypeVarSetVariables<'db>) -> RunResult<TypeVarSetInner<'db>> {
        self.local_with_fixed_transfers(2, size_of::<Option<TypeVarSetVariables<'db>>>(), || ())
            .await?;
        let mut variables = Some(variables);
        self.allocate_future(|| async {
            let variables = self
                .local_with_fixed_transfers(2, size_of::<TypeVarSetVariables<'db>>(), || {
                    variables.take()
                })
                .await?
                .ok_or(RunError::Contract("typevar set owner was consumed"))?;
            self.access.intern_typevar_set(variables).await
        })
        .await?
        .await
    }

    async fn publish(&self, inner: TypeVarSetInner<'db>) -> RunResult<TypeVarSet<'db>> {
        self.local_with_fixed_transfers(1, size_of::<TypeVarSet<'db>>(), || TypeVarSet::Some(inner))
            .await
    }
}
