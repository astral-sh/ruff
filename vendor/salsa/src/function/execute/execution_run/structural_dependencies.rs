//! Canonical structural validation while retaining its semantic caller.

use std::fmt;

use super::callback;
use super::frame_free::EntryScope;
use super::registration::TaskEndpoint;
use super::{ExecutionWork, RunError, RunResult};
use crate::attempt_probe::{MemoReuse, QueryPolicy};
use crate::function::{ErasedMemo, FunctionIngredientRef, VerifyResult};
use crate::prepared_source_probe::{PreparationError, Stamp, try_with_preparation};
use crate::{Database, DatabaseKeyIndex, Revision};

/// An exact structural dependency that can be prepared while its registered run is parked.
/// Only the runtime can create a request, from an unregistered complete-only dependency.
#[derive(Clone, Copy)]
struct DependencyValidationRequest<'db> {
    db: &'db dyn Database,
    stamp: Stamp,
    key: DatabaseKeyIndex,
    changed_after: Revision,
}

impl fmt::Debug for DependencyValidationRequest<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DependencyValidationRequest")
            .field("key", &self.key)
            .field("changed_after", &self.changed_after)
            .field("stamp", &self.stamp)
            .finish_non_exhaustive()
    }
}

impl PartialEq for DependencyValidationRequest<'_> {
    fn eq(&self, other: &Self) -> bool {
        self.stamp == other.stamp
            && std::ptr::eq(self.db.zalsa_local(), other.db.zalsa_local())
            && self.key == other.key
            && self.changed_after == other.changed_after
    }
}

impl Eq for DependencyValidationRequest<'_> {}

impl<'db> DependencyValidationRequest<'db> {
    /// Runs canonical structural validation on an idle structural stack.
    ///
    /// This enters the same preparation boundary as source parsing and indexing. No attempt,
    /// query, or operation may be active on this stack; a registered driver can park its semantic
    /// state first. Native cancellation and panics keep their ordinary behavior. The returned
    /// certificate cannot execute or deliver the structural query's value.
    fn prepare(
        self,
        db: &'db dyn Database,
    ) -> Result<PreparedDependencyValidation<'db>, PreparationError> {
        let certificate = try_with_preparation(db, || {
            self.function(db)?;
            let ingredient = db.zalsa().lookup_ingredient(self.key.ingredient_index());
            // SAFETY: The opaque request retains the database that owns this ingredient, and
            // `function` checks both database identities before the canonical operation starts.
            let result = unsafe {
                ingredient.maybe_changed_after(
                    db.zalsa(),
                    db.into(),
                    self.key.key_index(),
                    self.changed_after,
                )
            };
            let unchanged = match result {
                VerifyResult::Changed => None,
                VerifyResult::Unchanged { .. } => Some(
                    self.current_memo(db)?
                        .ok_or(PreparationError::InvalidDependency)?,
                ),
            };
            Ok(PreparedDependencyValidation {
                request: self,
                unchanged,
            })
        })??;
        self.function(db)?;
        certificate
            .result(self)
            .map_err(|_| PreparationError::InvalidDependency)?;
        Ok(certificate)
    }

    fn function(
        self,
        db: &'db dyn Database,
    ) -> Result<FunctionIngredientRef<'db>, PreparationError> {
        if !std::ptr::eq(self.db.zalsa(), db.zalsa())
            || !std::ptr::eq(self.db.zalsa_local(), db.zalsa_local())
        {
            return Err(PreparationError::InvalidDependency);
        }
        if !self.stamp.belongs_to(db) {
            return Err(PreparationError::ChangedDatabaseStamp);
        }
        let function = db
            .zalsa()
            .lookup_ingredient(self.key.ingredient_index())
            .as_function()
            .ok_or(PreparationError::UnsupportedDependency)?;
        if function.attempt_policy() != QueryPolicy::CompleteOnly {
            return Err(PreparationError::UnsupportedDependency);
        }
        Ok(function)
    }

    fn current_memo(
        self,
        db: &'db dyn Database,
    ) -> Result<Option<ErasedMemo<'db>>, PreparationError> {
        let Some(memo) = self.function(db)?.memo(db.zalsa(), self.key.key_index()) else {
            return Ok(None);
        };
        // Validation consumes only the revision proof. Native verification has already updated
        // tracked outputs, and eviction of the value does not remove that proof.
        let header = memo.header();
        Ok((header.verified_at.load() == db.zalsa().current_revision()
            && !header.may_be_provisional()
            && header.attempt_reuse(db.zalsa()) == MemoReuse::Ordinary)
            .then_some(memo))
    }
}

/// A canonical validation result for one dependency request in one database state.
/// Unchanged results retain the exact final memo, including its accumulated-input metadata.
#[derive(Clone, Copy)]
struct PreparedDependencyValidation<'db> {
    request: DependencyValidationRequest<'db>,
    unchanged: Option<ErasedMemo<'db>>,
}

impl fmt::Debug for PreparedDependencyValidation<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PreparedDependencyValidation")
            .field("request", &self.request)
            .field("unchanged", &self.unchanged.is_some())
            .finish_non_exhaustive()
    }
}

impl<'db> PreparedDependencyValidation<'db> {
    fn result(self, request: DependencyValidationRequest<'db>) -> RunResult<VerifyResult> {
        if self.request != request {
            return Err(RunError::Contract(
                "prepared dependency validation belongs to another request",
            ));
        }
        request.function(request.db).map_err(preparation_error)?;
        let Some(expected) = self.unchanged else {
            return Ok(VerifyResult::Changed);
        };
        let current = request
            .current_memo(request.db)
            .map_err(preparation_error)?
            .ok_or(RunError::Contract(
                "prepared dependency memo is not current and final",
            ))?;
        if !std::ptr::eq(current.header(), expected.header()) {
            return Err(RunError::Contract("prepared dependency memo was replaced"));
        }
        Ok(current
            .header()
            .current_revision_result(request.changed_after))
    }
}

fn preparation_error(error: PreparationError) -> RunError {
    let message = match error {
        PreparationError::ChangedDatabaseStamp => "structural dependency database stamp changed",
        PreparationError::InvalidDependency => "structural dependency is invalid",
        PreparationError::UnsupportedDependency => "structural dependency is not complete-only",
        PreparationError::ActiveAttempt
        | PreparationError::ActiveQuery
        | PreparationError::ActiveOperation => "structural dependency preparation is not idle",
    };
    RunError::Contract(message)
}

pub(super) async fn validate<'run, 'db: 'run>(
    endpoint: TaskEndpoint<'run, 'db>,
    db: &'db dyn Database,
    key: DatabaseKeyIndex,
    changed_after: Revision,
) -> RunResult<VerifyResult> {
    let scope = match EntryScope::capture(&endpoint.inner.context) {
        Ok(scope) => scope,
        Err(error) => match callback::reject(&endpoint.inner, error, ()).await {},
    };
    let result = callback::complete(
        &endpoint.inner,
        &scope,
        callback::CallbackKind::Canonical,
        || async {
            endpoint.admit_work(16)?;
            let request = DependencyValidationRequest {
                db,
                stamp: Stamp::current(db),
                key,
                changed_after,
            };
            if let Some(memo) = request.current_memo(db).map_err(preparation_error)? {
                return Ok(memo.header().current_revision_result(changed_after));
            }
            endpoint.admit(ExecutionWork::Resource {
                requested_bytes: size_of::<DependencyValidationRequest<'db>>()
                    .saturating_add(size_of::<PreparedDependencyValidation<'db>>()),
            })?;
            endpoint.check_completion()?;
            let certificate = endpoint
                .prepare_structural(|| request.prepare(db).map_err(RunError::Preparation))
                .await;
            endpoint.check_completion()?;
            certificate.result(request)
        },
    )
    .await;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::panic::{AssertUnwindSafe, catch_unwind};

    use super::*;
    use crate::function::execute::participant::Consumer;
    use crate::function::{ClaimResult, Reentrancy};
    use crate::plumbing::AsId;
    use crate::zalsa::ZalsaDatabase;
    use crate::{DatabaseImpl, Durability, Id};

    #[crate::input]
    struct Input {
        value: u32,
    }

    #[crate::tracked(returns(copy), attempt = CompleteOnly)]
    fn source(db: &dyn Database, input: Input) -> u32 {
        *input.value(db)
    }

    thread_local! {
        static PANIC_SOURCE: Cell<bool> = const { Cell::new(false) };
    }

    #[crate::tracked(returns(copy), attempt = CompleteOnly, cycle_initial = initial)]
    fn fallible_source(db: &dyn Database, input: Input) -> u32 {
        assert!(!PANIC_SOURCE.get(), "structural source panic");
        *input.value(db)
    }

    fn initial(_db: &dyn Database, _id: Id, _input: Input) -> u32 {
        0
    }

    #[test]
    fn certificates_reject_different_keys_revisions_and_databases() {
        let mut db = DatabaseImpl::default();
        let first = Input::new(&db, 4);
        let second = Input::new(&db, 6);
        let previous = db.zalsa().current_revision();
        db.synthetic_write(Durability::LOW);
        let ingredient = source::fn_ingredient_(&db, db.zalsa());
        let request = DependencyValidationRequest {
            db: &db,
            stamp: Stamp::current(&db),
            key: ingredient.database_key_index(first.as_id()),
            changed_after: previous,
        };
        let foreign = DatabaseImpl::default();
        assert_eq!(
            request.prepare(&foreign).unwrap_err(),
            PreparationError::InvalidDependency
        );
        let cloned = db.clone();
        assert_eq!(
            request.prepare(&cloned).unwrap_err(),
            PreparationError::InvalidDependency
        );
        let certificate = request.prepare(&db).unwrap();
        let foreign_input = Input::new(&foreign, 4);
        let foreign_key = source::fn_ingredient_(&foreign, foreign.zalsa())
            .database_key_index(foreign_input.as_id());
        for other in [
            DependencyValidationRequest {
                key: ingredient.database_key_index(second.as_id()),
                ..request
            },
            DependencyValidationRequest {
                changed_after: db.zalsa().current_revision(),
                ..request
            },
            DependencyValidationRequest {
                db: &foreign,
                stamp: Stamp::current(&foreign),
                key: foreign_key,
                changed_after: foreign.zalsa().current_revision(),
            },
            DependencyValidationRequest {
                db: &cloned,
                ..request
            },
        ] {
            assert!(matches!(
                certificate.result(other),
                Err(RunError::Contract(
                    "prepared dependency validation belongs to another request"
                ))
            ));
        }
        for checked in [&db, &foreign, &cloned] {
            assert!(checked.zalsa_local().active_query().is_none());
        }
        assert_eq!(crate::attempt_probe::stack_depths(), (0, 0));
    }

    #[test]
    fn unchanged_certificate_rejects_an_equal_replacement_memo() {
        let db = DatabaseImpl::default();
        let input = Input::new(&db, 7);
        assert_eq!(source(&db, input), 7);
        let ingredient = source::fn_ingredient_(&db, db.zalsa());
        let request = DependencyValidationRequest {
            db: &db,
            stamp: Stamp::current(&db),
            key: ingredient.database_key_index(input.as_id()),
            changed_after: db.zalsa().current_revision(),
        };
        let certificate = request.prepare(&db).unwrap();
        assert!(certificate.result(request).unwrap().is_unchanged());
        let old = ingredient
            .get_memo_from_table_for(
                db.zalsa(),
                input.as_id(),
                ingredient.memo_ingredient_index(db.zalsa(), input.as_id()),
            )
            .expect("the source query retained its memo");

        // A fresh canonical execution can retain the same value and `changed_at` revision. The
        // certificate authorizes only the original allocation, even when both values are equal.
        try_with_preparation(&db, || {
            let claim = match ingredient.sync_table.try_claim(
                db.zalsa(),
                db.zalsa_local(),
                input.as_id(),
                Reentrancy::Deny,
            ) {
                ClaimResult::Claimed(claim) => claim,
                _ => panic!("the source query is idle"),
            };
            let replacement = ingredient
                .execute(&db, claim, Some(old), Consumer::Validation)
                .expect("the acyclic source query completes");
            assert!(!std::ptr::eq(old, replacement));
            assert_eq!(old.value(), replacement.value());
            assert_eq!(
                old.header.revisions.changed_at,
                replacement.header.revisions.changed_at
            );
        })
        .unwrap();
        assert!(matches!(
            certificate.result(request),
            Err(RunError::Contract("prepared dependency memo was replaced"))
        ));
        assert!(db.zalsa_local().active_query().is_none());
        assert_eq!(crate::attempt_probe::stack_depths(), (0, 0));
    }

    #[test]
    fn unchanged_certificate_rejects_a_provisional_memo_left_by_native_unwind() {
        let db = DatabaseImpl::default();
        let input = Input::new(&db, 7);
        assert_eq!(fallible_source(&db, input), 7);
        let ingredient = fallible_source::fn_ingredient_(&db, db.zalsa());
        let request = DependencyValidationRequest {
            db: &db,
            stamp: Stamp::current(&db),
            key: ingredient.database_key_index(input.as_id()),
            changed_after: db.zalsa().current_revision(),
        };
        let certificate = request.prepare(&db).unwrap();
        assert!(certificate.result(request).unwrap().is_unchanged());
        let old = ingredient
            .get_memo_from_table_for(
                db.zalsa(),
                input.as_id(),
                ingredient.memo_ingredient_index(db.zalsa(), input.as_id()),
            )
            .unwrap();

        // Canonical fixpoint execution replaces its memo with a provisional poison value on
        // unwind. The retained certificate must reject that real runtime state before reuse.
        PANIC_SOURCE.set(true);
        let panic = catch_unwind(AssertUnwindSafe(|| {
            try_with_preparation(&db, || {
                let claim = match ingredient.sync_table.try_claim(
                    db.zalsa(),
                    db.zalsa_local(),
                    input.as_id(),
                    Reentrancy::Deny,
                ) {
                    ClaimResult::Claimed(claim) => claim,
                    _ => panic!("the source query is idle"),
                };
                ingredient.execute(&db, claim, Some(old), Consumer::Validation)
            })
        }));
        PANIC_SOURCE.set(false);
        assert!(panic.is_err());
        let poisoned = ingredient
            .get_memo_from_table_for(
                db.zalsa(),
                input.as_id(),
                ingredient.memo_ingredient_index(db.zalsa(), input.as_id()),
            )
            .unwrap();
        assert!(poisoned.header.may_be_provisional());
        assert_eq!(
            poisoned.header.verified_at.load(),
            db.zalsa().current_revision()
        );
        assert!(poisoned.value().is_none());
        assert!(matches!(
            certificate.result(request),
            Err(RunError::Contract(
                "prepared dependency memo is not current and final"
            ))
        ));
        assert!(db.zalsa_local().active_query().is_none());
        assert_eq!(crate::attempt_probe::stack_depths(), (0, 0));

        // A value read preserves the native panic for the rest of this revision. Canonical
        // validation can report Changed, but cannot turn the poisoned memo into an unchanged proof.
        assert!(matches!(
            crate::Cancelled::catch(AssertUnwindSafe(|| fallible_source(&db, input))),
            Err(crate::Cancelled::PropagatedPanic)
        ));
        let prepared = request.prepare(&db).unwrap();
        assert!(matches!(
            prepared.result(request),
            Ok(VerifyResult::Changed)
        ));
        assert!(db.zalsa_local().active_query().is_none());
        assert_eq!(crate::attempt_probe::stack_depths(), (0, 0));
    }
}
