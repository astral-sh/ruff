//! Declaration-only storage follows prior-binding lookup and assignability checking.

#[cfg(test)]
use std::alloc::Layout;

use ruff_python_ast::AnyNodeRef;
use salsa::execution_probe::{RunError, RunResult};
use ty_python_core::BindingWithConstraintsIterator;
use ty_python_core::definition::Definition;
use ty_python_core::place::PlaceExprRef;
use ty_python_core::scope::FileScopeId;

use super::storage::{StorageQuote, definition_map_insert_quote};
use super::{SourceAccess, SourceEffects, SourceOperation};
use crate::place::implicit_symbol::module_type_implicit_global_symbol_with;
use crate::place::{
    LookupError, LookupResult, Place, PlaceAndQualifiers, RequiresExplicitReExport,
    place_from_bindings_with,
};
use crate::types::infer::TypeInferenceBuilder;
use crate::types::infer::builder::annotated_assignment::AnnotatedAssignmentOperation;
use crate::types::infer::builder::source_binding::SourceBindingEffects;
use crate::types::infer::builder::source_declaration::{
    AddDeclarationEffects, add_declaration_with,
};
use crate::types::relation::source::assignability_condition;
use crate::types::{Type, TypeAndQualifiers};

fn declaration_storage_quote(len: usize, capacity: usize) -> Option<StorageQuote> {
    definition_map_insert_quote::<TypeAndQualifiers<'_>>(len, capacity)
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> SourceEffects<'_, 'run, 'db, A> {
    pub(super) async fn add_source_declaration<'ast>(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        node: AnyNodeRef<'_>,
        declaration: Definition<'db>,
        declared: TypeAndQualifiers<'db>,
    ) -> RunResult<()> {
        self.allocate_future(|| add_declaration_with(builder, node, declaration, declared, self))
            .await?
            .await
    }
}

impl<'run, 'db: 'run, 'ast, A: SourceAccess<'run, 'db>> AddDeclarationEffects<'db, 'ast>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn local<T>(
        &self,
        work: usize,
        bytes: usize,
        action: impl FnOnce() -> T,
    ) -> RunResult<T> {
        SourceEffects::local(self, work, bytes, action).await
    }

    async fn prior_bindings(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        declaration: Definition<'db>,
    ) -> RunResult<Place<'db>> {
        let fields = self.access.endpoint().field_request_context();
        #[cfg(debug_assertions)]
        {
            let kind = self.field(declaration.read_fields(fields).kind()).await?;
            let in_stub = self.file_is_stub(builder.file()).await?;
            self.local(4, 0, || {
                debug_assert!(kind.category(in_stub, builder.module()).is_declaration())
            })
            .await?;
        }
        let scope = self
            .field(declaration.read_fields(fields).scope_id())
            .await?;
        let file_scope = self
            .field(scope.read_fields(fields).file_scope_id())
            .await?;
        let use_def = self
            .local(1, size_of::<&ty_python_core::UseDefMap<'db>>(), || {
                builder.index.use_def_map(file_scope)
            })
            .await?;
        // FrozenMap uses binary search over Definition handles. This bounds comparisons for
        // every representable length without scanning its entries to quote the lookup.
        let bindings = self
            .local(
                usize::BITS as usize * 2 + 4,
                size_of::<BindingWithConstraintsIterator<'_, 'db>>(),
                || use_def.bindings_at_definition(declaration),
            )
            .await?;
        let count = self
            .local(1, size_of::<usize>(), || bindings.traversal_len())
            .await?;
        self.local(
            Self::checked(count.checked_mul(4).and_then(|n| n.checked_add(4)))?,
            0,
            || (),
        )
        .await?;
        let cache = SourceBindingEffects::reachability_cache(self, builder).await?;
        let place = self
            .allocate_future(|| {
                place_from_bindings_with(
                    builder.program_environment(),
                    self,
                    bindings,
                    RequiresExplicitReExport::No,
                    Some(cache),
                )
            })
            .await?
            .await?;
        self.local(1, size_of::<Place<'db>>(), || place.place).await
    }

    async fn lookup(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        place: PlaceAndQualifiers<'db>,
    ) -> RunResult<LookupResult<'db>> {
        self.allocate_future(|| {
            place.into_lookup_result_with(self.db(), builder.program_environment(), self)
        })
        .await?
        .await
    }

    async fn fallback_place(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        declaration: Definition<'db>,
    ) -> RunResult<(FileScopeId, PlaceExprRef<'db>)> {
        #[cfg(test)]
        observations::fallback(declaration);
        let fields = self.access.endpoint().field_request_context();
        let scope = self
            .field(builder.scope().read_fields(fields).file_scope_id())
            .await?;
        let place = self
            .field(declaration.read_fields(fields).place_info())
            .await?;
        self.local(3, size_of::<(FileScopeId, PlaceExprRef<'db>)>(), || {
            (scope, builder.index.place_table(scope).place(place.place()))
        })
        .await
    }

    async fn implicit_global(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        name: &str,
    ) -> RunResult<PlaceAndQualifiers<'db>> {
        self.allocate_future(|| {
            module_type_implicit_global_symbol_with(self.db(), builder.program_file(), name, self)
        })
        .await?
        .await
    }

    async fn merge_fallback(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        error: LookupError<'db>,
        fallback: PlaceAndQualifiers<'db>,
    ) -> RunResult<LookupResult<'db>> {
        self.allocate_future(|| {
            error.or_fall_back_to_with(self.db(), builder.program_environment(), self, fallback)
        })
        .await?
        .await
    }

    async fn assignable(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        inferred: Type<'db>,
        declared: Type<'db>,
    ) -> RunResult<bool> {
        self.allocate_future(|| {
            assignability_condition(
                self.db(),
                builder.program_environment(),
                inferred,
                declared,
                self,
            )
        })
        .await?
        .await
    }

    async fn invalid_declaration(
        &self,
        _builder: &TypeInferenceBuilder<'db, 'ast>,
        _node: AnyNodeRef<'_>,
        _inferred: Type<'db>,
        _declared: Type<'db>,
    ) -> RunResult<()> {
        self.unavailable(SourceOperation::AnnotatedAssignment(
            AnnotatedAssignmentOperation::Diagnostic,
        ))
        .await
    }

    async fn store(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        declaration: Definition<'db>,
        ty: TypeAndQualifiers<'db>,
    ) -> RunResult<()> {
        let quote = self
            .local(
                5,
                size_of::<(usize, usize)>()
                    + 3 * size_of::<StorageQuote>()
                    + 2 * size_of::<Option<StorageQuote>>(),
                || {
                    declaration_storage_quote(
                        builder.declarations.len(),
                        builder.declarations.0.capacity(),
                    )
                },
            )
            .await?
            .ok_or(RunError::Contract("declaration storage quotation overflow"))?;
        #[cfg(test)]
        observations::before(declaration, quote);
        self.local(quote.work, quote.bytes, || {
            builder.declarations.insert(declaration, ty);
            #[cfg(test)]
            observations::after(self.db(), declaration);
        })
        .await
    }
}

#[cfg(test)]
pub(in crate::types::infer) mod observations {
    use super::StorageQuote;
    use crate::Db;
    use salsa::plumbing::AsId;
    use std::cell::Cell;
    use ty_python_core::definition::Definition;

    #[derive(Clone, Copy, Default, Debug)]
    pub(in crate::types::infer) struct State {
        pub(in crate::types::infer) before: bool,
        pub(in crate::types::infer) fallbacks: usize,
        pub(in crate::types::infer) stored: bool,
        pub(in crate::types::infer) work: usize,
        pub(in crate::types::infer) bytes: usize,
    }

    thread_local! {
        static TARGET: Cell<Option<salsa::Id>> = const { Cell::new(None) };
        static CANCEL: Cell<bool> = const { Cell::new(false) };
        static STATE: Cell<State> = const { Cell::new(State { before: false, fallbacks: 0, stored: false, work: 0, bytes: 0 }) };
    }

    pub(in crate::types::infer) fn reset(target: Definition<'_>, cancel: bool) {
        TARGET.set(Some(target.as_id()));
        CANCEL.set(cancel);
        STATE.set(State::default());
    }

    pub(in crate::types::infer) fn state() -> State {
        STATE.get()
    }

    pub(super) fn fallback(target: Definition<'_>) {
        if TARGET.get() == Some(target.as_id()) {
            STATE.set(State {
                fallbacks: STATE.get().fallbacks + 1,
                ..STATE.get()
            });
        }
    }

    pub(in crate::types::infer::builder::source_definition::controlled) fn before(target: Definition<'_>, quote: StorageQuote) {
        if TARGET.get() == Some(target.as_id()) {
            STATE.set(State {
                before: true,
                stored: false,
                work: quote.work,
                bytes: quote.bytes,
                ..STATE.get()
            });
        }
    }

    pub(in crate::types::infer::builder::source_definition::controlled) fn after(db: &dyn Db, target: Definition<'_>) {
        if TARGET.get() == Some(target.as_id()) {
            STATE.set(State {
                stored: true,
                ..STATE.get()
            });
            if CANCEL.replace(false) {
                db.cancellation_token().cancel();
                db.unwind_if_revision_cancelled();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Storage quotation rejects lengths whose arithmetic or replacement layout cannot be represented.
    #[test]
    fn declaration_storage_rejects_overflow_and_invalid_layout() {
        type Entry<'db> = (Definition<'db>, TypeAndQualifiers<'db>);
        assert!(declaration_storage_quote(usize::MAX, usize::MAX).is_none());
        let capacity = (isize::MAX as usize / size_of::<Entry<'_>>()) / 2 + 1;
        assert!(Layout::array::<Entry<'_>>(capacity * 2).is_err());
        assert!(declaration_storage_quote(capacity, capacity).is_none());
    }
}
