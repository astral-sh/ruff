//! Collect identity arguments through admitted flat storage and the canonical specialization interner.

use std::alloc::Layout;

use salsa::execution_probe::{RunError, RunResult};

use super::{SourceAccess, SourceEffects};
use crate::types::generics::binding::TypeVarBindingEffects;
use crate::types::generics::identity::{
    IdentityArguments, IdentitySpecializationEffects, identity_specialization_with,
};
use crate::types::{
    BoundTypeVarIdentity, BoundTypeVarInstance, GenericContext, Specialization, Type,
};

#[cfg(test)]
use crate::types::generics::identity::observations::{self, Event};

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> SourceEffects<'_, 'run, 'db, A> {
    /// Preserves every bound occurrence while constructing the context's canonical identity arguments.
    pub(in crate::types::infer) async fn identity_specialization(
        &self,
        context: GenericContext<'db>,
    ) -> RunResult<Specialization<'db>> {
        self.type_parameter_future(|| identity_specialization_with(context, self))
            .await?
            .await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> IdentitySpecializationEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn start(&self, context: GenericContext<'db>) -> RunResult<IdentityArguments<'db>> {
        let variables = TypeVarBindingEffects::variables(self, context).await?;
        let quote = self
            .local_with_fixed_transfers(
                32,
                32 * (size_of::<usize>() + size_of::<Option<usize>>()),
                || {
                    let len = variables.len();
                    Layout::array::<Type<'db>>(len)
                        .map_err(|_| RunError::Contract("identity argument layout overflow"))?;
                    let work = len.checked_mul(2).and_then(|work| work.checked_add(8));
                    let bytes = len.checked_mul(size_of::<Type<'db>>()).and_then(|bytes| {
                        bytes.checked_add(3 * size_of::<Vec<Type<'db>>>() + 8 * size_of::<usize>())
                    });
                    work.zip(bytes).ok_or(RunError::Contract(
                        "identity argument allocation quote overflow",
                    ))
                },
            )
            .await?;
        self.local_quoted_with_fixed_transfers(quote, || {
            let arguments = IdentityArguments::new(variables);
            #[cfg(test)]
            let arguments = {
                let mut arguments = arguments;
                arguments.observe(self.db());
                arguments
            };
            arguments
        })
        .await
    }

    async fn append_next(&self, arguments: &mut IdentityArguments<'db>) -> RunResult<Option<()>> {
        // One callback covers lookup, cursor advance and append; reserved arity prevents growth.
        let bytes = 2
            * size_of::<Option<(&BoundTypeVarIdentity<'db>, &BoundTypeVarInstance<'db>)>>()
            + 2 * size_of::<Option<BoundTypeVarInstance<'db>>>()
            + 2 * size_of::<Type<'db>>()
            + 8 * size_of::<usize>()
            + 2 * size_of::<bool>();
        #[cfg(test)]
        observations::record(self.db(), Event::BeforeAppend(arguments.storage()));
        self.local_with_fixed_transfers(24, bytes, || {
            let result = arguments.append_next();
            #[cfg(test)]
            observations::record(self.db(), Event::AfterAppend(arguments.storage()));
            result
        })
        .await
    }

    async fn finish(
        &self,
        context: GenericContext<'db>,
        arguments: IdentityArguments<'db>,
    ) -> RunResult<Specialization<'db>> {
        let quote = self
            .local_with_fixed_transfers(
                32,
                32 * (size_of::<usize>() + size_of::<Option<usize>>()),
                || {
                    let storage = arguments.storage();
                    let len = storage.len;
                    let capacity = storage.capacity;
                    if storage.expected != len {
                        return Err(RunError::Contract("identity specialization arity changed"));
                    }
                    let (work, bytes) = if len == capacity {
                        (Some(8), Some(0))
                    } else {
                        Layout::array::<Type<'db>>(len).map_err(|_| {
                            RunError::Contract("identity argument box layout overflow")
                        })?;
                        (
                            len.checked_mul(2)
                                .and_then(|work| work.checked_add(capacity))
                                .and_then(|work| work.checked_add(12)),
                            len.checked_mul(size_of::<Type<'db>>()),
                        )
                    };
                    let bytes = bytes.and_then(|bytes| {
                        bytes.checked_add(
                            2 * size_of::<Vec<Type<'db>>>()
                                + 2 * size_of::<Box<[Type<'db>]>>()
                                + 8 * size_of::<usize>(),
                        )
                    });
                    work.zip(bytes).ok_or(RunError::Contract(
                        "identity argument finish quote overflow",
                    ))
                },
            )
            .await?;
        #[cfg(test)]
        observations::record(self.db(), Event::BeforeBox(arguments.storage()));
        let types = self
            .local_quoted_with_fixed_transfers(quote, || {
                let types = arguments.into_types().into_boxed_slice();
                #[cfg(test)]
                observations::record(self.db(), Event::BoxTransferred);
                types
            })
            .await?;
        self.type_parameter_future(|| {
            self.access
                .intern_specialization(context, types, None, None)
        })
        .await?
        .await
    }
}
