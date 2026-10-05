use crate::accumulator::accumulated_map::{AccumulatedMap, InputAccumulatedValues};
use crate::accumulator::{self};
use crate::function::execute::execution_run::explicit_reads::assert_ordinary_execution_allowed;
use crate::function::{Configuration, IngredientImpl};
use crate::hash::FxHashSet;
use crate::zalsa::ZalsaDatabase;
use crate::zalsa_local::QueryOriginRef;
use crate::{DatabaseKeyIndex, Id};

impl<C> IngredientImpl<C>
where
    C: Configuration,
{
    /// Helper used by `accumulate` functions. Computes the results accumulated by `database_key_index`
    /// and its inputs.
    pub fn accumulated_by<'db, A>(&self, db: &'db C::DbView, key: Id) -> Vec<&'db A>
    where
        A: accumulator::Accumulator,
    {
        assert_ordinary_execution_allowed(db.zalsas().into(), "accumulated query values");
        let (zalsa, zalsa_local) = db.zalsas();

        // NOTE: We don't have a precise way to track accumulated values at present,
        // so we report any read of them as an untracked read.
        //
        // Like tracked struct fields, accumulated values are essentially a "side channel output"
        // from a tracked function, hence we can't report this as a read of the tracked function(s)
        // whose accumulated values we are probing, since the accumulated values may have changed
        // even when the main return value of the function has not changed.
        //
        // Unlike tracked struct fields, we don't have a distinct id or ingredient to represent
        // "the values of type A accumulated by tracked function X". Typically accumulated values
        // are read from outside of salsa anyway so this is not a big deal.
        zalsa_local.report_untracked_read(zalsa.current_revision());

        let Some(accumulator) = <accumulator::IngredientImpl<A>>::from_zalsa(zalsa) else {
            return vec![];
        };
        let mut output = vec![];

        // First ensure the result is up to date
        self.fetch(db, zalsa, zalsa_local, key);

        let db_key = self.database_key_index(key);
        let mut visited: FxHashSet<DatabaseKeyIndex> = FxHashSet::default();
        let mut stack: Vec<DatabaseKeyIndex> = vec![db_key];

        // Do a depth-first search across the dependencies of `key`, reading the values accumulated by
        // each dependency.
        while let Some(k) = stack.pop() {
            // Already visited `k`?
            if !visited.insert(k) {
                continue;
            }

            let ingredient = zalsa.lookup_ingredient(k.ingredient_index());
            // Extend `output` with any values accumulated by `k`.
            // SAFETY: `db` owns the `ingredient`
            let (accumulated_map, input) =
                unsafe { ingredient.accumulated(db.into(), k.key_index()) };
            if let Some(accumulated_map) = accumulated_map {
                accumulated_map.extend_with_accumulated(accumulator.index(), &mut output);
            }
            // Skip over the inputs because we know that the entire sub-graph has no accumulated values
            if input.is_empty() {
                continue;
            }

            // Find the inputs of `k` and push them onto the stack.
            //
            // Careful: to ensure the user gets a consistent ordering in their
            // output vector, we want to push in execution order, so reverse order to
            // ensure the first child that was executed will be the first child popped
            // from the stack.
            let Some(origin) = ingredient
                .as_function()
                .and_then(|function| function.memo(zalsa, k.key_index()))
                .map(|memo| memo.header().origin())
            else {
                continue;
            };

            if let QueryOriginRef::Derived(edges) | QueryOriginRef::DerivedUntracked(edges) = origin
            {
                stack.reserve(edges.len());
            }

            stack.extend(origin.inputs().rev());

            visited.reserve(stack.len());
        }

        output
    }

    pub(super) fn accumulated_map<'db>(
        &'db self,
        db: &'db C::DbView,
        key: Id,
    ) -> (Option<&'db AccumulatedMap>, InputAccumulatedValues) {
        let (zalsa, zalsa_local) = db.zalsas();
        // NEXT STEP: stash and refactor `fetch` to return an `&Memo` so we can make this work
        let memo = self.refresh_memo(db, zalsa, zalsa_local, key);
        (
            memo.header.revisions.accumulated(),
            memo.header.revisions.accumulated_inputs.load(),
        )
    }
}
