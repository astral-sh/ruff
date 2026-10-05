//! Export collection retains its prepared module and names while canonical children run.

use ruff_python_ast as ast;
use ruff_python_ast::name::Name;
use rustc_hash::FxHashSet;
use salsa::execution_probe::{RunError, RunResult};
use ty_module_resolver::Module;
use ty_python_core::{ProgramFile, Truthiness};

use super::storage::{StorageQuote, sequence_merge, slots, table_merge};
use super::{PreparedSource, SourceAccess, SourceEffects};
use crate::ProgramEnvironment;
use crate::dunder_all::collector::{
    Collector, DunderAllEffects, DunderAllFacts, Frame, collect_with,
};
use crate::types::infer::builder::string_literal::StringLiteralEffects;
#[cfg(test)]
use crate::types::infer::source_runtime::tests::dunder_all as observations;
use crate::types::module_member_effects::ModuleMemberEffects;
use crate::types::{ModuleLiteralType, Type, TypeContext};

fn checked(value: Option<usize>) -> RunResult<usize> {
    value.ok_or(RunError::Contract("export collection quotation overflow"))
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> SourceEffects<'_, 'run, 'db, A> {
    pub(super) async fn export_names_contains(
        &self,
        names: &FxHashSet<Name>,
        name: &str,
    ) -> RunResult<bool> {
        let (backing, length) = self
            .local(2, 0, || (slots(names.capacity()), name.len()))
            .await?;
        let work = checked(
            checked(backing)?
                .checked_add(1)
                .and_then(|n| n.checked_mul(length.checked_add(1)?)),
        )?;
        self.local(work, 0, || names.contains(name)).await
    }

    pub(in crate::types::infer) async fn infer_dunder_all(
        &self,
        file: ProgramFile<'db>,
    ) -> RunResult<Option<FxHashSet<Name>>> {
        self.check_file_program(file).await?;
        let prepared = self.access.prepare_existing(file).await?;
        self.check_file_program(prepared.file).await?;
        if prepared.file != file {
            return Err(RunError::Contract("prepared export file is foreign"));
        }
        #[cfg(test)]
        let _lifetime = observations::OwnerLifetime::new(file);
        let mut state = self.local(4, 0, Collector::default).await?;
        let effects = DunderAllSourceEffects {
            source: self,
            prepared: &prepared,
            env: ProgramEnvironment::from_file(file),
        };
        let body = self.local(1, 0, || prepared.module.suite()).await?;
        self.allocate_future(|| collect_with(&mut state, body, DunderAllFacts, &effects))
            .await?
            .await
    }
}

struct DunderAllSourceEffects<'effects, 'access, 'run, 'db: 'run, A> {
    source: &'effects SourceEffects<'access, 'run, 'db, A>,
    prepared: &'effects PreparedSource<'db>,
    env: ProgramEnvironment<'db>,
}

#[derive(Clone, Copy)]
enum IncomingName<'name> {
    Literal(&'name str),
    Imported(&'name Name),
}

impl<'name> IncomingName<'name> {
    fn text(self) -> &'name str {
        match self {
            Self::Literal(text) => text,
            Self::Imported(name) => name.as_str(),
        }
    }

    fn owned(self) -> Name {
        match self {
            Self::Literal(text) => Name::new(text),
            Self::Imported(name) => name.clone(),
        }
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> DunderAllSourceEffects<'_, '_, 'run, 'db, A> {
    async fn literal<'expr>(
        &self,
        literal: &'expr ast::ExprStringLiteral,
    ) -> RunResult<&'expr str> {
        let length = StringLiteralEffects::length(self.source, literal).await?;
        self.source.string_literal_text(literal, length).await
    }

    async fn text_work(&self, names: &FxHashSet<Name>, backing: usize) -> RunResult<usize> {
        let work = checked(
            backing
                .checked_add(names.len())
                .and_then(|n| n.checked_add(2)),
        )?;
        self.source
            .local(work, 0, || {
                names.iter().try_fold(0usize, |length, name| {
                    checked(length.checked_add(name.as_str().len()))
                })
            })
            .await?
    }

    async fn insert(&self, state: &mut Collector<'_>, name: IncomingName<'_>) -> RunResult<()> {
        let (length, capacity, retained, name_len) = self
            .source
            .local(4, 0, || {
                (
                    state.names.len(),
                    state.names.capacity(),
                    state.names_backing,
                    name.text().len(),
                )
            })
            .await?;
        let (mut quote, backing) = table_merge::<Name>(length, capacity, 1, retained)
            .ok_or(RunError::Contract("export set growth quotation overflow"))?;
        let old_backing = checked(slots(capacity))?.max(retained);
        let hashing = checked(
            old_backing
                .checked_add(1)
                .and_then(|n| n.checked_mul(name_len.checked_add(1)?)),
        )?;
        let rehashing = if quote.bytes != 0 {
            let text = self.text_work(&state.names, old_backing).await?;
            checked(
                length
                    .checked_mul(backing)
                    .and_then(|n| n.checked_add(text)),
            )?
        } else {
            0
        };
        // Each insertion prepays disposal of its name and the retained table. Removals and
        // clear preserve the backing estimate because either can leave allocated buckets.
        quote.work = checked(
            quote
                .work
                .checked_add(hashing)
                .and_then(|n| n.checked_add(rehashing))
                .and_then(|n| n.checked_add(backing))
                .and_then(|n| n.checked_add(name_len.checked_mul(2)?))
                .and_then(|n| n.checked_add(length))
                .and_then(|n| n.checked_add(8)),
        )?;
        if matches!(name, IncomingName::Literal(_)) {
            quote.bytes = checked(
                quote
                    .bytes
                    .checked_add(name_len)
                    .and_then(|n| n.checked_add(4 * size_of::<Name>())),
            )?;
        }
        #[cfg(test)]
        observations::before_mutation(self.source.db(), observations::Mutation::Insert);
        self.source
            .local(quote.work, quote.bytes, || {
                state.names.reserve(1);
                state.names.insert(name.owned());
                state.names_backing = slots(state.names.capacity())
                    .map_or(backing, |observed| retained.max(observed));
                #[cfg(test)]
                observations::after_mutation(self.source.db(), observations::Mutation::Insert);
            })
            .await
    }

    async fn names_for_module(
        &self,
        module: Module<'db>,
    ) -> RunResult<Option<&'db FxHashSet<Name>>> {
        let Some(file) = module.file_with(self.source.access.endpoint()).await? else {
            return Ok(None);
        };
        let prepared = self
            .source
            .access
            .prepare_file(file, self.source.program)
            .await?;
        self.source.check_file_program(prepared.file).await?;
        #[cfg(test)]
        observations::child_request(self.source.db(), prepared.file);
        let result = self.source.access.dunder_all_names(prepared.file).await?;
        self.source.local(1, 0, || result.as_ref()).await
    }
}

impl<'run, 'db: 'run, 'ast, A: SourceAccess<'run, 'db>> DunderAllEffects<'db, 'ast>
    for DunderAllSourceEffects<'_, '_, 'run, 'db, A>
{
    type Error = RunError;

    async fn next(&self, state: &mut Collector<'ast>) -> RunResult<Option<Frame<'ast>>> {
        self.source.local(2, 0, || state.frames.pop()).await
    }

    async fn push(&self, state: &mut Collector<'ast>, frame: Frame<'ast>) -> RunResult<()> {
        let (length, capacity) = self
            .source
            .local(2, 0, || (state.frames.len(), state.frames.capacity()))
            .await?;
        let mut quote = sequence_merge::<Frame<'ast>>(length, capacity, 1).ok_or(
            RunError::Contract("export traversal growth quotation overflow"),
        )?;
        let backing = capacity.max(quote.bytes / size_of::<Frame<'ast>>());
        quote.work = checked(
            quote
                .work
                .checked_add(backing)
                .and_then(|n| n.checked_add(length))
                .and_then(|n| n.checked_add(4)),
        )?;
        let mut frame = Some(frame);
        self.source
            .local(quote.work, quote.bytes, || {
                state.frames.reserve(1);
                state.frames.extend(frame.take());
                #[cfg(test)]
                observations::frame_depth(self.source.db(), state.frames.len());
            })
            .await
    }

    async fn clear_names(&self, state: &mut Collector<'ast>) -> RunResult<()> {
        let (length, capacity, retained) = self
            .source
            .local(3, 0, || {
                (
                    state.names.len(),
                    state.names.capacity(),
                    state.names_backing,
                )
            })
            .await?;
        let backing = checked(slots(capacity))?.max(retained);
        let work = checked(backing.checked_add(length).and_then(|n| n.checked_add(4)))?;
        #[cfg(test)]
        observations::before_mutation(self.source.db(), observations::Mutation::Clear);
        self.source
            .local(work, 0, || {
                state.names.clear();
                state.names_backing = backing;
                #[cfg(test)]
                observations::after_mutation(self.source.db(), observations::Mutation::Clear);
            })
            .await
    }

    async fn add_name(
        &self,
        state: &mut Collector<'ast>,
        literal: &ast::ExprStringLiteral,
    ) -> RunResult<()> {
        let name = self.literal(literal).await?;
        self.insert(state, IncomingName::Literal(name)).await
    }

    async fn remove_name(
        &self,
        state: &mut Collector<'ast>,
        literal: &ast::ExprStringLiteral,
    ) -> RunResult<()> {
        let name = self.literal(literal).await?;
        let backing = self
            .source
            .local(2, 0, || {
                checked(slots(state.names.capacity())).map(|slots| slots.max(state.names_backing))
            })
            .await??;
        let work = checked(
            backing
                .checked_add(1)
                .and_then(|n| n.checked_mul(name.len().checked_add(1)?))
                .and_then(|n| n.checked_add(4)),
        )?;
        #[cfg(test)]
        observations::before_mutation(self.source.db(), observations::Mutation::Remove);
        self.source
            .local(work, 0, || {
                state.names.remove(name);
                state.names_backing = backing;
                #[cfg(test)]
                observations::after_mutation(self.source.db(), observations::Mutation::Remove);
            })
            .await
    }

    async fn extend_names(
        &self,
        state: &mut Collector<'ast>,
        names: &FxHashSet<Name>,
    ) -> RunResult<()> {
        let backing = self
            .source
            .local(1, 0, || checked(slots(names.capacity())))
            .await??;
        let mut names = self.source.local(backing, 0, || names.iter()).await?;
        while let Some(name) = self.source.local(2, 0, || names.next()).await? {
            self.insert(state, IncomingName::Imported(name)).await?;
        }
        Ok(())
    }

    async fn contains_dunder_all(&self, names: &FxHashSet<Name>) -> RunResult<bool> {
        self.source.export_names_contains(names, "__all__").await
    }

    async fn finish(&self, state: &mut Collector<'ast>) -> RunResult<FxHashSet<Name>> {
        let (length, capacity, retained) = self
            .source
            .local(3, 0, || {
                (
                    state.names.len(),
                    state.names.capacity(),
                    state.names_backing,
                )
            })
            .await?;
        let old_backing = checked(slots(capacity))?.max(retained);
        // Retained history includes buckets left behind by removals. If more than half the
        // allocation's full capacity is occupied, shrinking cannot choose a smaller table.
        let no_shrink = length
            .checked_mul(2)
            .and_then(|length| length.checked_sub(1))
            .and_then(slots)
            .is_some_and(|bound| old_backing <= bound);
        let text = self.text_work(&state.names, old_backing).await?;
        let (mut quote, new_backing) = if no_shrink {
            (StorageQuote::default(), old_backing)
        } else {
            table_merge::<Name>(0, 0, length, 0).ok_or(RunError::Contract(
                "export set finalization quotation overflow",
            ))?
        };
        let rehashing = if no_shrink {
            0
        } else {
            checked(length.checked_mul(new_backing))?
        };
        quote.work = checked(
            quote
                .work
                .checked_add(old_backing)
                .and_then(|n| n.checked_add(new_backing))
                .and_then(|n| n.checked_add(rehashing))
                .and_then(|n| n.checked_add(text.checked_mul(2)?))
                .and_then(|n| n.checked_add(length.checked_mul(2)?))
                .and_then(|n| n.checked_add(4)),
        )?;
        #[cfg(test)]
        observations::before_mutation(self.source.db(), observations::Mutation::Finish);
        self.source
            .local(quote.work, quote.bytes, || {
                state.names.shrink_to_fit();
                let names = std::mem::take(&mut state.names);
                state.names_backing = 0;
                #[cfg(test)]
                observations::after_mutation(self.source.db(), observations::Mutation::Finish);
                names
            })
            .await
    }

    async fn discard(&self, state: &mut Collector<'ast>) -> RunResult<()> {
        let work = self
            .source
            .local(4, 0, || {
                checked(
                    slots(state.names.capacity())
                        .map(|slots| slots.max(state.names_backing))
                        .and_then(|n| n.checked_add(state.names.len()))
                        .and_then(|n| n.checked_add(state.frames.capacity()))
                        .and_then(|n| n.checked_add(state.frames.len()))
                        .and_then(|n| n.checked_add(4)),
                )
            })
            .await??;
        #[cfg(test)]
        observations::before_mutation(self.source.db(), observations::Mutation::Discard);
        self.source
            .local(work, 0, || {
                drop(std::mem::take(&mut state.names));
                drop(std::mem::take(&mut state.frames));
                state.names_backing = 0;
                #[cfg(test)]
                observations::after_mutation(self.source.db(), observations::Mutation::Discard);
            })
            .await
    }

    async fn imported_names(
        &self,
        import: &ast::StmtImportFrom,
    ) -> RunResult<Option<&'db FxHashSet<Name>>> {
        let Ok(name) = self
            .source
            .import_module_name(self.prepared.file, import)
            .await?
        else {
            return Ok(None);
        };
        let file = self.source.physical_file(self.prepared.file).await?;
        let Some(module) = self
            .source
            .access
            .resolve_module(self.source.program, &name, Some(file))
            .await?
        else {
            return Ok(None);
        };
        self.names_for_module(module).await
    }

    async fn module_names(
        &self,
        module: ModuleLiteralType<'db>,
    ) -> RunResult<Option<&'db FxHashSet<Name>>> {
        let module = ModuleMemberEffects::module(self.source, self.source.db(), module).await?;
        self.names_for_module(module).await
    }

    async fn expression_type(&self, expr: &'ast ast::Expr) -> RunResult<Type<'db>> {
        let lookup = self
            .source
            .local(1, 0, || self.prepared.index.expression_lookup_work())
            .await?;
        let expression = self
            .source
            .local(checked(lookup)?, 0, || self.prepared.index.expression(expr))
            .await?;
        let inference = self
            .source
            .canonical_expression(expression, TypeContext::default())
            .await?;
        let work = checked(inference.expressions.iter().len().checked_add(3))?;
        self.source
            .local(work, 0, || inference.expression_type(expr))
            .await
    }

    async fn truthiness(&self, ty: Type<'db>) -> RunResult<Option<Truthiness>> {
        let result = self.source.try_type_truthiness(&self.env, ty).await?;
        self.source.local(1, 0, || result.ok()).await
    }
}
