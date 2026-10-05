//! Admitted traversal and replacement of the retained layout-constraint map.

use salsa::execution_probe::{RunError, RunResult};

use super::ClassCheckEffects;
use crate::analysis::ClassCheckOperation;
use crate::types::class::DisjointBase;
use crate::types::diagnostic::disjoint_bases::{
    DisjointBaseEffects, DisjointBaseFacts, next_disjoint_base_entry, prune_disjoint_bases_with,
};
use crate::types::diagnostic::{IncompatibleBaseInfo, IncompatibleBases};
use crate::types::infer::builder::source_definition::controlled::{SourceAccess, SourceOperation};

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> ClassCheckEffects<'_, '_, 'run, 'db, '_, A> {
    pub(super) async fn prune_disjoint_bases(
        &self,
        bases: &mut IncompatibleBases<'db>,
    ) -> RunResult<()> {
        prune_disjoint_bases_with(bases, DisjointBaseFacts, self).await
    }

    pub(super) async fn disjoint_base_count(
        &self,
        bases: &IncompatibleBases<'db>,
    ) -> RunResult<usize> {
        self.source.local(1, 0, || bases.len()).await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> DisjointBaseEffects<'db>
    for ClassCheckEffects<'_, '_, 'run, 'db, '_, A>
{
    type Error = RunError;

    async fn empty_retained_bases(&self) -> RunResult<IncompatibleBases<'db>> {
        // Insertion refuses before mutation, so this map stays allocation-free. Construction
        // prepays its disposal if a later comparison, insertion, or admission refuses.
        self.source.local(4, 0, IncompatibleBases::default).await
    }

    async fn next_disjoint_base(
        &self,
        bases: &IncompatibleBases<'db>,
        cursor: &mut usize,
    ) -> RunResult<Option<(DisjointBase<'db>, IncompatibleBaseInfo<'db>)>> {
        self.source
            .local(1, 0, || next_disjoint_base_entry(bases, cursor))
            .await
    }

    async fn is_layout_subtype(
        &self,
        _base: DisjointBase<'db>,
        _other: DisjointBase<'db>,
    ) -> RunResult<bool> {
        self.source
            .unavailable(SourceOperation::ClassCheck(ClassCheckOperation::Mro))
            .await
    }

    async fn retain_disjoint_base(
        &self,
        _retained: &mut IncompatibleBases<'db>,
        _base: DisjointBase<'db>,
        _info: IncompatibleBaseInfo<'db>,
    ) -> RunResult<()> {
        self.source
            .unavailable(SourceOperation::ClassCheck(ClassCheckOperation::Mro))
            .await
    }

    async fn replace_disjoint_bases(
        &self,
        bases: &mut IncompatibleBases<'db>,
        retained: &mut IncompatibleBases<'db>,
    ) -> RunResult<()> {
        // Both owners stay borrowed until admission succeeds. Their construction already
        // covers disposal, including cancellation before or after this swap.
        self.source
            .local(1, 0, || std::mem::swap(bases, retained))
            .await
    }
}
