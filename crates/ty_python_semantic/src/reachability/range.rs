//! Diagnostic reachability checks each containing range in the current scope and its ancestors.

use std::convert::Infallible;

use ruff_text_size::TextRange;
use ty_mapping_probe_macros::shared_semantic_family;
use ty_python_core::SemanticIndex;
use ty_python_core::reachability_constraints::ScopedReachabilityConstraintId;
use ty_python_core::scope::FileScopeId;
use ty_python_core::UseDefMap;

use crate::Db;

/// A cursor over one scope's range constraints, borrowing the use-def map's storage.
///
/// Only `scope_ranges` constructs this cursor. Its iterator maps a slice without filtering,
/// so advancing it examines at most one stored range.
#[derive(Debug)]
pub(crate) struct RangeCursor<I> {
    entries: I,
}

impl<I: Iterator<Item = (TextRange, ScopedReachabilityConstraintId)>> RangeCursor<I> {
    /// Advances to the next stored range and its constraint.
    pub(crate) fn next(&mut self) -> Option<(TextRange, ScopedReachabilityConstraintId)> {
        self.entries.next()
    }
}

/// Borrows the range constraints for one scope without allocating or scanning them.
pub(crate) fn scope_ranges<'map>(
    use_def: &'map UseDefMap<'_>,
) -> RangeCursor<impl Iterator<Item = (TextRange, ScopedReachabilityConstraintId)> + 'map> {
    RangeCursor {
        entries: use_def.range_reachability(),
    }
}

/// Advances from a scope to its parent, returning the current scope's use-def map.
pub(crate) fn next_ancestor<'index, 'db>(
    index: &'index SemanticIndex<'db>,
    cursor: &mut Option<FileScopeId>,
) -> Option<&'index UseDefMap<'db>> {
    let scope = (*cursor)?;
    *cursor = index.scope(scope).parent();
    Some(index.use_def_map(scope))
}

shared_semantic_family! {
    /// Supplies bounded cursor steps and predicate evaluation for diagnostic reachability.
    #[synchronous(SynchronousRangeReachabilityEffects)]
    pub(crate) trait RangeReachabilityEffects<'db> {
        type Error;

        #[operation(local)]
        async fn ancestor_cursor(&self, scope: FileScopeId) -> Result<Option<FileScopeId>, Self::Error>;
        #[operation(local)]
        #[progress]
        async fn next_ancestor<'index>(
            &self,
            index: &'index SemanticIndex<'db>,
            cursor: &mut Option<FileScopeId>,
        ) -> Result<Option<&'index UseDefMap<'db>>, Self::Error>;
        #[operation(source)]
        async fn scope_reachable(&self, use_def: &UseDefMap<'db>, range: TextRange) -> Result<bool, Self::Error>;
        #[operation(local)]
        #[progress]
        async fn next_range<I>(
            &self,
            cursor: &mut RangeCursor<I>,
        ) -> Result<Option<(TextRange, ScopedReachabilityConstraintId)>, Self::Error>
        where
            I: Iterator<Item = (TextRange, ScopedReachabilityConstraintId)>;
        #[operation(local)]
        async fn contains_range(&self, entry_range: TextRange, range: TextRange) -> Result<bool, Self::Error>;
        #[operation(source)]
        async fn constraint_reachable(
            &self,
            use_def: &UseDefMap<'db>,
            constraint: ScopedReachabilityConstraintId,
        ) -> Result<bool, Self::Error>;
    }

    /// Checks containing range constraints, starting in the given scope and then its ancestors.
    #[synchronous(is_range_reachable_sync)]
    #[capabilities(effects = RangeReachabilityEffects)]
    #[passive_values()]
    pub(crate) async fn is_range_reachable_with<'db, E: RangeReachabilityEffects<'db>>(
        index: &SemanticIndex<'db>,
        scope: FileScopeId,
        range: TextRange,
        effects: &E,
    ) -> Result<bool, E::Error> {
        let mut cursor = effects.ancestor_cursor(scope).await?;
        #[cursor_loop]
        while let Some(use_def) = effects.next_ancestor(index, &mut cursor).await? {
            if !effects.scope_reachable(use_def, range).await? {
                return Ok(false);
            }
        }
        Ok(true)
    }

    /// Checks whether a range may be reached according to one scope's containing entries.
    #[synchronous(scope_ranges_reachable_sync)]
    #[capabilities(effects = RangeReachabilityEffects)]
    #[passive_values()]
    pub(crate) async fn scope_ranges_reachable_with<'db, I, E: RangeReachabilityEffects<'db>>(
        use_def: &UseDefMap<'db>,
        range: TextRange,
        cursor: &mut RangeCursor<I>,
        effects: &E,
    ) -> Result<bool, E::Error>
    where
        I: Iterator<Item = (TextRange, ScopedReachabilityConstraintId)>,
    {
        #[cursor_loop]
        while let Some(entry) = effects.next_range(cursor).await? {
            let (entry_range, constraint) = entry;
            if effects.contains_range(entry_range, range).await?
                && !effects.constraint_reachable(use_def, constraint).await?
            {
                return Ok(false);
            }
        }
        Ok(true)
    }
}

/// Evaluates diagnostic reachability through the ordinary inference queries.
pub(crate) struct OrdinaryRangeReachabilityEffects<'db> {
    db: &'db dyn Db,
}

impl<'db> OrdinaryRangeReachabilityEffects<'db> {
    pub(crate) const fn new(db: &'db dyn Db) -> Self {
        Self { db }
    }
}

impl<'db> SynchronousRangeReachabilityEffects<'db> for OrdinaryRangeReachabilityEffects<'db> {
    type Error = Infallible;

    fn ancestor_cursor(&self, scope: FileScopeId) -> Result<Option<FileScopeId>, Self::Error> {
        Ok(Some(scope))
    }

    fn next_ancestor<'index>(
        &self,
        index: &'index SemanticIndex<'db>,
        cursor: &mut Option<FileScopeId>,
    ) -> Result<Option<&'index UseDefMap<'db>>, Self::Error> {
        Ok(next_ancestor(index, cursor))
    }

    fn scope_reachable(&self, use_def: &UseDefMap<'db>, range: TextRange) -> Result<bool, Self::Error> {
        scope_ranges_reachable_sync(use_def, range, &mut scope_ranges(use_def), self)
    }

    fn next_range<I>(
        &self,
        cursor: &mut RangeCursor<I>,
    ) -> Result<Option<(TextRange, ScopedReachabilityConstraintId)>, Self::Error>
    where
        I: Iterator<Item = (TextRange, ScopedReachabilityConstraintId)>,
    {
        Ok(cursor.next())
    }

    fn contains_range(&self, entry_range: TextRange, range: TextRange) -> Result<bool, Self::Error> {
        Ok(entry_range.contains_range(range))
    }

    fn constraint_reachable(
        &self,
        use_def: &UseDefMap<'db>,
        constraint: ScopedReachabilityConstraintId,
    ) -> Result<bool, Self::Error> {
        Ok(super::is_reachable(self.db, use_def, constraint))
    }
}
