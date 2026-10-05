//! Base-expression collection retains its result and recursion state across admitted operations.

mod storage;

use salsa::execution_probe::{RunError, RunResult};

use super::ClassCheckEffects;
use crate::types::class::base_typevars::{
    BaseTypeVarCollector, BaseTypeVarEffects, BaseTypeVarFacts, BaseTypeVarWalkEffects,
    next_base_type, visit_base_typevars_with,
};
use crate::types::class::context::explicit_class_bases_with;
use crate::types::infer::builder::source_definition::controlled::{SourceAccess, SourceEffects};
use crate::types::local_transfer::collections::{CALL_1, CALL_2, add_quotes, checked, event_quote};
use crate::types::local_transfer::context_variables::context_variable_at_quote;
use crate::types::visitor::runtime::RuntimeTypeWalk;
use crate::types::visitor::{
    NonAtomicType, TypeKind, TypeWalkEffects, TypeWalkEvent, TypeWalkPolicy, WalkAction,
};
use crate::types::{BoundTypeVarInstance, StaticClassLiteral, Type};
use crate::{FxIndexSet, ProgramEnvironment};

/// Quotes the provider receiver and policy built inside a funded forwarding future.
const fn walk_forward_quote<T>() -> RunResult<(usize, usize)> {
    checked(event_quote(
        3 * CALL_1 + CALL_2 + 24,
        &[size_of::<T>(), size_of::<TypeWalkPolicy>()],
    ))
}

/// Quotes the first visit action before the shared reduction reaches its event admissions.
const fn visit_base_quote() -> RunResult<(usize, usize)> {
    checked(event_quote(
        CALL_2 + CALL_1 + 12,
        &[
            size_of::<Type<'_>>(),
            size_of::<WalkAction<'_>>(),
            size_of::<BaseTypeVarFacts>(),
        ],
    ))
}

/// Prepays the shared event reducer's classification, bindings, scheduling and loop back-edge.
const fn next_event_quote<T>() -> RunResult<(usize, usize)> {
    let forwarding = match walk_forward_quote::<T>() {
        Ok(quote) => Some(quote),
        Err(error) => return Err(error),
    };
    checked(add_quotes(
        forwarding,
        event_quote(
            4 * CALL_1 + 3 * CALL_2 + 40,
            &[
                size_of::<TypeWalkEvent<'_>>(),
                size_of::<Type<'_>>(),
                size_of::<TypeKind<'_>>(),
                size_of::<WalkAction<'_>>(),
                size_of::<NonAtomicType<'_>>(),
                size_of::<bool>(),
            ],
        ),
    ))
}

/// Quotes one copied base and the shared outer loop's cursor, branch and return operations.
const fn next_base_quote() -> RunResult<(usize, usize)> {
    let lookup = match context_variable_at_quote() {
        Ok(quote) => Some(quote),
        Err(error) => return Err(error),
    };
    // The context lookup includes an ordered-map wrapper around the same checked slice access.
    checked(add_quotes(
        lookup,
        event_quote(
            3 * CALL_1 + CALL_2 + 26,
            &[
                size_of::<(&[Type<'_>], &mut usize)>(),
                size_of::<Option<&Type<'_>>>(),
                size_of::<Option<Type<'_>>>(),
                size_of::<(usize, bool)>(),
            ],
        ),
    ))
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> BaseTypeVarEffects<'db>
    for ClassCheckEffects<'_, '_, 'run, 'db, '_, A>
{
    type Error = RunError;

    async fn new_collector(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> RunResult<BaseTypeVarCollector<'db>> {
        let scope = self
            .source
            .boxed_future_with_fixed_transfers(
                Ok((10, size_of::<[(StaticClassLiteral<'db>, &Self); 2]>())),
                || self.generic_class_body_scope(class),
            )
            .await?
            .await?;
        let file = self
            .source
            .boxed_future_with_fixed_transfers(
                Ok((
                    10,
                    size_of::<[(&Self, ty_python_core::scope::ScopeId<'db>); 2]>(),
                )),
                || self.generic_scope_file(scope),
            )
            .await?
            .await?;
        self.source
            .boxed_future_with_fixed_transfers(
                Ok((
                    10,
                    size_of::<[(&Self, ty_python_core::ProgramFile<'db>); 2]>(),
                )),
                || self.check_generic_file_program(file),
            )
            .await?
            .await?;
        self.source
            .boxed_future_with_fixed_transfers(
                Ok((6, size_of::<[StaticClassLiteral<'db>; 2]>())),
                || {
                    self.source.new_base_variable_collector(
                        || ProgramEnvironment::from_program(self.source.program),
                        #[cfg(test)]
                        class,
                    )
                },
            )
            .await?
            .await
    }

    async fn explicit_bases(&self, class: StaticClassLiteral<'db>) -> RunResult<&'db [Type<'db>]> {
        self.source
            .boxed_future_with_fixed_transfers(
                Ok((10, size_of::<[(StaticClassLiteral<'db>, &Self); 2]>())),
                || explicit_class_bases_with(class, self.source),
            )
            .await?
            .await
    }

    async fn next_base(
        &self,
        bases: &[Type<'db>],
        cursor: &mut usize,
    ) -> RunResult<Option<Type<'db>>> {
        self.source
            .local_quoted_with_fixed_transfers(const { next_base_quote() }, || {
                next_base_type(bases, cursor)
            })
            .await
    }

    async fn visit_base(
        &self,
        collector: &mut BaseTypeVarCollector<'db>,
        base: Type<'db>,
    ) -> RunResult<()> {
        self.source
            .boxed_future_with_fixed_transfers(const { visit_base_quote() }, || {
                visit_base_typevars_with(collector, base, BaseTypeVarFacts, self.source)
            })
            .await?
            .await
    }

    async fn finish_collector(
        &self,
        collector: BaseTypeVarCollector<'db>,
    ) -> RunResult<FxIndexSet<BoundTypeVarInstance<'db>>> {
        self.source
            .boxed_future_with_fixed_transfers(Ok((6, 0)), || {
                self.source.finish_base_variable_collector(collector)
            })
            .await?
            .await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> SourceEffects<'_, 'run, 'db, A> {
    /// Constructs the environment and empty base collector after admitting their ownership.
    async fn new_base_variable_collector(
        &self,
        environment: impl FnOnce() -> ProgramEnvironment<'db>,
        #[cfg(test)] class: StaticClassLiteral<'db>,
    ) -> RunResult<BaseTypeVarCollector<'db>> {
        self.local_quoted_with_fixed_transfers(const { storage::initial_quote() }, || {
            BaseTypeVarCollector::new(
                environment(),
                #[cfg(test)]
                class,
            )
        })
        .await
    }

    /// Transfers the retained variables after all bases finish; insertions prepaid backing cleanup.
    async fn finish_base_variable_collector(
        &self,
        collector: BaseTypeVarCollector<'db>,
    ) -> RunResult<FxIndexSet<BoundTypeVarInstance<'db>>> {
        self.local_with_fixed_transfers(CALL_1 + 8, 0, || {
            #[cfg(test)]
            crate::types::infer::source_runtime::tests::class_generic_validation::base_typevars::collector_finished(
                collector.class, &collector.typevars,
            );
            collector.into_typevars()
        }).await
    }

    /// Runs the production collector over supplied base values for traversal and interruption controls.
    #[cfg(test)]
    pub(in crate::types::infer) async fn collect_base_variables_from_types(
        &self,
        class: StaticClassLiteral<'db>,
        env: ProgramEnvironment<'db>,
        bases: &[Type<'db>],
    ) -> RunResult<FxIndexSet<BoundTypeVarInstance<'db>>> {
        let mut collector = self
            .boxed_future_with_fixed_transfers(Ok((6, 0)), || {
                self.new_base_variable_collector(|| env, class)
            })
            .await?
            .await?;
        let mut cursor = 0;
        while let Some(base) = self
            .local_quoted_with_fixed_transfers(const { next_base_quote() }, || {
                next_base_type(bases, &mut cursor)
            })
            .await?
        {
            self.boxed_future_with_fixed_transfers(const { visit_base_quote() }, || {
                visit_base_typevars_with(&mut collector, base, BaseTypeVarFacts, self)
            })
            .await?
            .await?;
        }
        self.boxed_future_with_fixed_transfers(Ok((6, 0)), || {
            self.finish_base_variable_collector(collector)
        })
        .await?
        .await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> BaseTypeVarWalkEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn push(
        &self,
        collector: &mut BaseTypeVarCollector<'db>,
        action: WalkAction<'db>,
    ) -> RunResult<()> {
        self.local_quoted_with_fixed_transfers(
            const { walk_forward_quote::<RuntimeTypeWalk<'_, 'run, 'db, (), &Self>>() },
            || async {
                let mut fields = RuntimeTypeWalk {
                    db: self.db(),
                    endpoint: self.access.endpoint(),
                    query: (),
                    unavailable: self,
                };
                fields.push_action(&mut collector.cursor, action).await
            },
        )
        .await?
        .await
    }

    async fn next_event(
        &self,
        collector: &mut BaseTypeVarCollector<'db>,
    ) -> RunResult<Option<TypeWalkEvent<'db>>> {
        let quote = const { next_event_quote::<RuntimeTypeWalk<'_, 'run, 'db, (), &Self>>() };
        self.local_quoted_with_fixed_transfers(quote, || async {
            let mut fields = RuntimeTypeWalk {
                db: self.db(),
                endpoint: self.access.endpoint(),
                query: (),
                unavailable: self,
            };
            fields
                .next_event(&mut collector.cursor, TypeWalkPolicy::base_variables())
                .await
        })
        .await?
        .await
    }

    async fn remember(
        &self,
        collector: &mut BaseTypeVarCollector<'db>,
        ty: Type<'db>,
    ) -> RunResult<bool> {
        let already_seen = self
            .local_quoted_with_fixed_transfers(
                const { walk_forward_quote::<RuntimeTypeWalk<'_, 'run, 'db, (), &Self>>() },
                || async {
                    let mut fields = RuntimeTypeWalk {
                        db: self.db(),
                        endpoint: self.access.endpoint(),
                        query: (),
                        unavailable: self,
                    };
                    fields
                        .remember_type(&mut collector.recursion_guard, ty)
                        .await
                },
            )
            .await?
            .await?;
        #[cfg(test)]
        if !already_seen {
            crate::types::infer::source_runtime::tests::class_generic_validation::base_typevars::collector_expanded(
                collector.class, ty,
            );
        }
        Ok(already_seen)
    }

    async fn record(
        &self,
        collector: &mut BaseTypeVarCollector<'db>,
        variable: BoundTypeVarInstance<'db>,
    ) -> RunResult<()> {
        let quote = self
            .local_quoted_with_fixed_transfers(const { storage::preparation_quote() }, || {
                storage::insert_quote(collector.typevars.len(), collector.typevars.capacity())
            })
            .await??;
        self.local_quoted_with_fixed_transfers(Ok(quote), || {
            collector.typevars.insert(variable);
            #[cfg(test)]
            crate::types::infer::source_runtime::tests::class_generic_validation::base_typevars::collector_retained(
                collector.class, &collector.typevars,
            );
        }).await
    }

    async fn expand(
        &self,
        collector: &mut BaseTypeVarCollector<'db>,
        kind: NonAtomicType<'db>,
    ) -> RunResult<()> {
        self.local_quoted_with_fixed_transfers(
            const { walk_forward_quote::<RuntimeTypeWalk<'_, 'run, 'db, (), &Self>>() },
            || async {
                let mut fields = RuntimeTypeWalk {
                    db: self.db(),
                    endpoint: self.access.endpoint(),
                    query: (),
                    unavailable: self,
                };
                fields
                    .expand_children(
                        &mut collector.cursor,
                        kind,
                        TypeWalkPolicy::base_variables(),
                    )
                    .await
            },
        )
        .await?
        .await
    }
}
