//! A framework for writing incremental, on-demand computations.
//!
//! # Choosing a Salsa construct
//!
//! - [`input`] structs hold mutable state supplied from outside Salsa.
//! - [`tracked`] structs represent derived entities owned by one query invocation.
//! - [`interned`] structs canonicalize immutable values for structural equality and sharing.
//! - [`tracked`] functions are memoized computations. They record the queries and fields read by
//!   their body and act as incremental invalidation boundaries.
//! - [`accumulator`] values are auxiliary outputs, such as diagnostics, that do not participate in
//!   a tracked function's main result.
//! - [`Supertype`] enums let one tracked function accept several different Salsa struct types as
//!   its key.
//!
//! # Salsa structs
//!
//! Input, tracked, and interned structs are the three kinds of Salsa struct. All three are compact,
//! [`Copy`] handles whose fields live in the database, but they intentionally use different notions
//! of identity, equality, and ownership:
//!
//! | Kind | How identity is assigned | Lifecycle |
//! |------|--------------------------|-----------|
//! | [Input](#input-structs) | Each constructor call creates a distinct identity | Lives until the database is dropped |
//! | [Tracked](#tracked-structs) | Producing query, identity fields, and occurrence | Owned by the producing query |
//! | [Interned](#interned-structs) | All field values; equal fields share an identity | Shared; low-durability values may be reclaimed |
//!
//! The generated [`Eq`] and [`Hash`] implementations compare the compact ID for every kind of Salsa
//! struct: two handles are equal exactly when they have the same Salsa identity. What differs is how
//! Salsa assigns and preserves that identity.
//!
//! ## Input structs
//!
//! An [`input`] struct represents mutable state supplied from outside the incremental computation,
//! such as the contents of a file. Inputs are the roots from which tracked computations read.
//!
//! ### Identity
//!
//! Every constructor call creates a distinct input identity. Two inputs with equal field values are
//! therefore unequal, while updating a field preserves the input's identity.
//!
//! Dependencies are recorded per field. If a query reads only `file.text(db)`, changing another
//! field does not invalidate that query. A setter always records its field as changed; Salsa does
//! not compare the old and new values first. Each field also has a [`Durability`]. If a revision
//! changes only lower-durability inputs, Salsa can skip validating queries that depend exclusively
//! on higher-durability inputs.
//!
//! ### Lifecycle
//!
//! An input handle has no `'db` parameter and can be copied across revisions. It must still be used
//! with the database in which it was created. The input's data and memo entries keyed by it remain
//! in that database until it is dropped; dropping every copy of the handle does not delete the
//! input.
//!
//! References returned by field getters are tied to an immutable database borrow. They cannot
//! overlap the mutable borrow required to call a setter and begin a new revision.
//!
//! See [input structs in the Salsa book] for examples of declaring, reading, and updating inputs,
//! and the [durability reference] for choosing a durability.
//!
//! ## Tracked structs
//!
//! A [`tracked`] struct represents a derived entity created while a tracked function executes. It
//! is a good fit for intermediate values whose identity belongs to one computation rather than
//! being shared structurally across the entire database.
//!
//! ### Identity
//!
//! A tracked struct's identity consists of its producing query invocation, the values of every
//! field not marked `#[tracked]`, and its occurrence among structs with the same identity created
//! by that invocation. Equal-looking structs created by different queries, by different query
//! keys, or twice by one invocation are distinct.
//!
//! When the producing query re-executes, Salsa matches newly created structs with the previous
//! execution. Recreating the same identities in the same order preserves their IDs.
//!
//! A field marked `#[tracked]` is excluded from identity. When an entity is matched across
//! executions, Salsa compares the old and new field values with [`PartialEq`] and replaces the
//! stored value when they differ. Only queries that read a changed tracked field are invalidated;
//! reading an identity field depends on the entity as a whole.
//!
//! ### Lifecycle
//!
//! A tracked handle carries a `'db` lifetime tied to an immutable database borrow, so it cannot be
//! used across the mutable borrow that starts a new revision. The handle must be obtained again in
//! a later revision even when Salsa preserves the entity's identity.
//!
//! Tracked structs are outputs owned by their producing query. Validating that query also validates
//! its outputs. When it re-executes, any previous tracked struct that is not recreated becomes
//! stale; Salsa reclaims it and may reuse its storage.
//!
//! See [tracked structs in the Salsa book] for examples of tracked entities in an incremental IR.
//!
//! ## Interned structs
//!
//! An [`interned`] struct canonicalizes an immutable set of field values. Interning is useful when
//! every occurrence of equal field values should share one database-wide identity, regardless of
//! where or how often those values are created. Comparing the resulting handles is then a cheap ID
//! comparison.
//!
//! ### Identity
//!
//! The complete set of fields determines an interned struct's identity. Interning the same field
//! values again in the same revision returns the same handle and shares the stored data, regardless
//! of which query performs the interning. Once interned, comparing two values is a cheap ID
//! comparison.
//!
//! This database-wide sharing requires coordination through the interner. Prefer a tracked struct
//! when the value is a derived entity owned by one query and does not need structural sharing.
//!
//! ### Lifecycle
//!
//! By default, an interned handle carries a `'db` lifetime tied to an immutable database borrow and
//! cannot be used across a new revision. Create or retrieve the value again to obtain a handle for
//! the new revision.
//!
//! Salsa may reclaim a low-durability interned value after it has not been used for the number of
//! active revisions configured by the `revisions` option, which defaults to `3`. Reclaiming reuses
//! its slot with a new ID generation so dependencies on the old value are invalidated. Values with
//! higher durability are not reclaimed; `revisions = usize::MAX` disables reclamation for the
//! interned type.
//!
//! See [interned structs in the Salsa book] for examples of canonicalizing names and other values.
//!
//! # Return modes
//!
//! Salsa struct field getters and tracked functions return references by default. The
//! `#[returns(MODE)]` field attribute and `returns(MODE)` tracked-function option select another
//! mode:
//!
//! - `ref` returns a reference to the stored field or memoized result. This is the default.
//! - `clone` returns an owned clone.
//! - `copy` returns an owned copy.
//! - `deref` uses [`Deref`] and returns a reference to its `Target`.
//! - `as_ref` uses [`SalsaAsRef`].
//! - `as_deref` uses [`SalsaAsDeref`].
//!
//! Owned results can outlive the database borrow if their types permit it. Borrowed results are
//! tied to the immutable database borrow and cannot be used across a revision in which Salsa may
//! replace or reclaim the stored value.
//!
//! See [returning references in the Salsa book] for examples on fields and tracked functions.
//!
//! # Tracked functions and memoized values
//!
//! A [`tracked`] function is identified by the function and its non-database arguments. Those
//! arguments select a memoized query but are not themselves dependencies: dependencies arise only
//! when the body reads a Salsa field or calls another tracked function.
//!
//! A function without non-database arguments has one query key and one memoized result in each
//! database. A function with one non-database argument uses that Salsa struct's ID directly as its
//! query key. With multiple arguments, every call first interns the argument tuple to obtain a
//! synthetic Salsa ID. This extra interning step lets Salsa use the tuple as a query key; the
//! arguments' [`Eq`] and [`Hash`] implementations determine whether two calls resolve to the same
//! ID and memo.
//!
//! If a query's dependencies have not changed, Salsa reuses its memoized result. After
//! re-execution, Salsa compares the old and new results with [`PartialEq`]. If they are equal, Salsa
//! preserves the memo's previous "changed at" revision. This optimization is called [backdating];
//! it prevents invalidation from propagating to dependents when the result has not changed. The
//! `no_eq` option disables this comparison.
//!
//! A memo stores one current result, not a history. By default, each key that remains in the
//! database retains its result. Re-execution may update or replace the value; reclaiming the key or
//! dropping the database removes it. The `lru` option additionally evicts least-recently-used
//! results at the start of a new revision, but retains their memo entries so a later call can
//! recompute the value.
//!
//! The [return mode](#return-modes) controls whether callers receive an owned result or borrow it
//! from the memo.
//!
//! See [tracked functions in the Salsa book] for an introduction, the [red-green algorithm] for the
//! full validation model, and [cache tuning] for controlling memo retention.
//!
//! ## Specifying results
//!
//! The `specify` option supports queries with both an on-demand incremental implementation and a
//! batch implementation. The tracked function defines how to compute one result on demand. A
//! query that creates many tracked structs can instead compute their results together and call
//! `FUNCTION::specify(db, key, value)` for each one, avoiding the per-key implementation when those
//! results are later requested. It can also provide special results for built-in entities or model
//! a value initialized after a tracked struct is created.
//!
//! A specifiable function must take exactly one non-database argument, and that argument must be a
//! tracked struct, not an input or interned struct. `specify` must be called during the same tracked
//! query invocation that created the key. Salsa records the specified memo as an output of that
//! creating query, so validating or re-executing the creator also validates or replaces the
//! specified result. The `specify` and `lru` options cannot currently be combined.
//!
//! See [specifying query results in the Salsa book] for an example.
//!
//! # Accumulators
//!
//! An [`accumulator`] is a side channel for auxiliary outputs such as diagnostics. Accumulated
//! values are stored with a memoized query execution but do not participate in the query's return
//! value or result equality. Adding or removing one therefore does not by itself make the query's
//! main result change. Values can only be accumulated while a tracked function is executing;
//! attempting to accumulate outside one panics.
//!
//! Calling a tracked function's generated `accumulated` method first brings the query up to date,
//! then returns references to values emitted by that query and its transitive callees. The
//! references are tied to the database borrow. If the query re-executes, its new accumulated values
//! replace the previous set; if Salsa reuses the memo, the existing values remain available without
//! rerunning the function body.
//!
//! See [accumulators in the Salsa book] for a complete diagnostic-reporting example.
//!
//! # Supertypes
//!
//! A [`Supertype`] enum is a heterogeneous Salsa-struct key. It lets one tracked function operate
//! on several input, tracked, or interned struct types. Salsa uses the wrapped struct's ID directly
//! as the query key, while its concrete Salsa struct type determines the enum variant. Without a
//! supertype, the query must be duplicated for every concrete type or callers must convert every
//! value into some other common Salsa struct.
//!
//! The enum must be nonempty. Each variant must contain exactly one unnamed field wrapping a Salsa
//! struct or another `Supertype`; nesting supertypes can build larger groups from smaller ones. A
//! concrete Salsa struct must be reachable through only one variant, ensuring that Salsa can
//! determine the variant unambiguously from the wrapped ID.
//!
//! A supertype has no storage of its own, so its validity and lifecycle follow the wrapped value.
//!
//! The [Salsa book](https://salsa-rs.github.io/salsa) develops these constructs as part of a
//! complete incremental program. Its chapter on the [`'db` database lifetime] explains why tracked
//! and interned values cannot cross revisions.
//!
//! # Values retained across revisions
//!
//! Salsa retains tracked and interned fields and memoized query results after the database borrow
//! that produced them has ended. [`SalsaValue`] marks types whose database lifetime can safely be
//! replaced with `'static` for storage and restored when the value is accessed. The derive checks
//! this through the value's fields.
//!
//! A `'static` field type is accepted without a `SalsaValue` implementation. Custom field types
//! that carry the database lifetime should normally derive `SalsaValue`. See its safety
//! documentation before implementing it manually or exempting a field from the generated checks.
//!
//! [`Deref`]: std::ops::Deref
//! [`Hash`]: std::hash::Hash
//! [`'db` database lifetime]: https://salsa-rs.github.io/salsa/plumbing/db_lifetime.html
//! [accumulators in the Salsa book]: https://salsa-rs.github.io/salsa/tutorial/accumulators.html
//! [backdating]: https://salsa-rs.github.io/salsa/reference/algorithm.html#backdating-sometimes-we-can-be-smarter
//! [cache tuning]: https://salsa-rs.github.io/salsa/tuning.html#cache-eviction-lru
//! [durability reference]: https://salsa-rs.github.io/salsa/reference/durability.html
//! [input structs in the Salsa book]: https://salsa-rs.github.io/salsa/overview.html#inputs
//! [interned structs in the Salsa book]: https://salsa-rs.github.io/salsa/overview.html#interned-structs
//! [red-green algorithm]: https://salsa-rs.github.io/salsa/reference/algorithm.html
//! [returning references in the Salsa book]: https://salsa-rs.github.io/salsa/tutorial/parser.html#the-returnscopy-annotation
//! [specifying query results in the Salsa book]: https://salsa-rs.github.io/salsa/overview.html#specify-the-result-of-tracked-functions-for-particular-structs
//! [tracked functions in the Salsa book]: https://salsa-rs.github.io/salsa/overview.html#tracked-functions
//! [tracked structs in the Salsa book]: https://salsa-rs.github.io/salsa/overview.html#tracked-structs

#![deny(clippy::undocumented_unsafe_blocks)]
#![forbid(unsafe_op_in_unsafe_fn)]

#[cfg(test)]
extern crate self as salsa;

#[cfg(feature = "accumulator")]
mod accumulator;
mod active_query;
mod attach;
mod cancelled;
mod cycle;
mod database;
mod database_impl;
mod durability;
mod event;
mod field_reads;
mod function;
mod hash;
mod id;
mod ingredient;
mod ingredient_cache;
mod input;
mod interned;
mod key;
mod memo_ingredient_indices;
mod quote;
mod return_mode;
mod revision;
mod runtime;
mod salsa_struct;
mod salsa_value;
mod storage;
mod sync;
mod table;
mod tracing;
mod tracked_struct;
mod views;
mod zalsa;
mod zalsa_local;

pub mod attempt_probe;
pub mod prepared_source_probe;

pub use function::execute::execution_run::registration as execution_probe;

#[cfg(not(feature = "inventory"))]
mod nonce;

#[cfg(feature = "macros")]
pub use salsa_macros::{SalsaValue, Supertype, accumulator, db, input, interned, tracked};

#[cfg(feature = "salsa_unstable")]
pub use self::database::{IngredientInfo, PageInfo};

#[cfg(feature = "accumulator")]
pub use self::accumulator::Accumulator;
pub use self::active_query::Backtrace;
pub use self::cancelled::Cancelled;

pub use self::cycle::{Cycle, CycleHeadCandidate, CycleHeadCandidates};
pub use self::database::Database;
pub use self::database_impl::DatabaseImpl;
pub use self::durability::Durability;
pub use self::event::{Event, EventKind};
pub use self::field_reads::FieldReads;
pub use self::id::Id;
pub use self::input::setter::Setter;
pub use self::key::DatabaseKeyIndex;
pub use self::return_mode::SalsaAsDeref;
pub use self::return_mode::SalsaAsRef;
pub use self::revision::Revision;
pub use self::runtime::Runtime;
pub use self::salsa_value::SalsaValue;
pub use self::storage::{Storage, StorageHandle};
pub use self::zalsa::IngredientIndex;
pub use self::zalsa_local::CancellationToken;
pub use crate::attach::{attach, attach_allow_change, with_attached_database};
pub use crate::interned::{HashEqLike, Lookup};

pub mod prelude {
    #[cfg(feature = "accumulator")]
    pub use crate::accumulator::Accumulator;
    pub use crate::{Database, Setter};
}

/// Internal names used by salsa macros.
///
/// # WARNING
///
/// The contents of this module are NOT subject to semver.
#[doc(hidden)]
pub mod plumbing {
    pub use crate::quote::{QuoteError, QuoteFuel};
    pub use std::any::TypeId;
    pub use std::option::Option::{self, None, Some};
    pub use typeid::ConstTypeId;

    #[cfg(feature = "accumulator")]
    pub use salsa_macro_rules::setup_accumulator_impl;
    pub use salsa_macro_rules::{
        gate_accumulated, macro_if, maybe_default, maybe_default_tt, return_mode_expression,
        return_mode_field_mode, return_mode_ty, setup_input_struct, setup_interned_struct,
        setup_tracked_assoc_fn_body, setup_tracked_fn, setup_tracked_method_body,
        setup_tracked_struct, unexpected_cycle_initial, unexpected_cycle_recovery,
    };

    pub use crate::SalsaValue;
    #[cfg(feature = "accumulator")]
    pub use crate::accumulator::Accumulator;
    pub use crate::attach::{attach, with_attached_database};
    pub use crate::cycle::CycleRecoveryStrategy;
    pub use crate::database::{Database, current_revision};
    pub use crate::durability::Durability;
    pub use crate::execution_probe::{
        FieldRequest, FieldRequestContext, FieldReturnMode, InputFieldRequest,
        InternedFieldRequest, TrackedFieldRequest,
    };
    pub use crate::field_reads::interned_fields;
    pub use crate::function::execute::execution_run::explicit_reads::{
        OrdinaryAccess, assert_ordinary_execution_allowed,
    };
    pub use crate::id::{AsId, FromId, FromIdWithDb, Id};
    pub use crate::ingredient::{Ingredient, Jar, Location};
    pub use crate::ingredient_cache::IngredientCache;
    pub use crate::interned::{HashEqLike, Lookup};
    pub use crate::key::DatabaseKeyIndex;
    pub use crate::memo_ingredient_indices::{
        IngredientIndices, MemoIngredientIndices, MemoIngredientMap, MemoIngredientSingletonIndex,
        NewMemoIngredientIndices,
    };
    pub use crate::revision::{AtomicRevision, Revision};
    pub use crate::runtime::{Runtime, Stamp, stamp};
    pub use crate::salsa_struct::{SalsaStructInDb, assert_supertype_no_overlap};
    pub use crate::salsa_value::helper::{
        Dispatch as SalsaValueDispatch, Fallback as SalsaValueFallback, assert_salsa_value,
    };
    pub use crate::storage::{HasStorage, Storage};
    pub use crate::table::memo::MemoTableWithTypes;
    pub use crate::tracked_struct::{TrackedStructInDb, update_field};
    pub use crate::views::DatabaseDownCaster;
    pub use crate::zalsa::{
        ErasedJar, HasJar, IngredientIndex, JarKind, Zalsa, ZalsaDatabase, register_jar,
        transmute_data_ptr, views,
    };
    pub use crate::zalsa_local::ZalsaLocal;

    #[cfg(feature = "persistence")]
    pub use serde;

    /// Names a tracked function's existing configuration for exact ingredient types.
    /// Generated for function markers opting into the `configuration` attribute option.
    pub trait TrackedFunctionConfiguration {
        type Configuration: function::Configuration;
    }

    // A stub for `serde` used when persistence is disabled.
    //
    // We provide dummy types to avoid detecting features during macro expansion.
    #[cfg(not(feature = "persistence"))]
    pub mod serde {
        pub trait Serializer {
            type Ok;
            type Error;
        }

        pub trait Deserializer<'de> {
            type Ok;
            type Error;
        }
    }

    #[cfg(feature = "accumulator")]
    pub mod accumulator {
        pub use crate::accumulator::{IngredientImpl, JarImpl};
    }

    pub mod input {
        pub use crate::input::input_field::FieldIngredientImpl;
        pub use crate::input::setter::SetterImpl;
        pub use crate::input::singleton::{NotSingleton, Singleton};
        pub use crate::input::{Configuration, HasBuilder, IngredientImpl, JarImpl, Value};
    }

    pub mod interned {
        pub use crate::interned::{
            Configuration, FiniteInternedConfiguration, IngredientImpl, JarImpl, Value,
        };
    }

    pub mod function {
        pub use crate::function::{
            Configuration, FixedQueryFields, IngredientImpl, InternedQueryConfiguration, Memo,
            RetainedInput,
        };
        pub use crate::function::{EvictionPolicy, HasCapacity, Lru, NoopEviction};
        pub use crate::table::memo::MemoEntryType;
    }

    pub mod tracked_struct {
        pub use crate::tracked_struct::tracked_field::FieldIngredientImpl;
        pub use crate::tracked_struct::{Configuration, IngredientImpl, JarImpl, Value};
    }
}
