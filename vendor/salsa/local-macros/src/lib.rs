//! Procedural macros for defining Salsa databases, ingredients, and queries.
//!
//! This crate is an implementation detail of [`salsa`](https://docs.rs/salsa/latest/salsa/). Its
//! macros are re-exported from that crate and should normally be invoked through the `salsa::`
//! path.
//!
//! See the [`salsa` crate documentation](https://docs.rs/salsa/latest/salsa/) for the concepts
//! behind each macro.

#![recursion_limit = "256"]

#[macro_use]
extern crate quote;

use proc_macro::TokenStream;

macro_rules! parse_quote {
    ($($inp:tt)*) => {
        {
            let tt = quote!{$($inp)*};
            syn::parse2(tt.clone()).unwrap_or_else(|err| {
                panic!("failed to parse `{}` at {}:{}:{}: {}", tt, file!(), line!(), column!(), err)
            })
        }
    }
}

/// Similar to `syn::parse_macro_input`, however, when a parse error is encountered, it will return
/// the input token stream in addition to the error. This will make it so that rust-analyzer can work
/// with incomplete code.
macro_rules! parse_macro_input {
    ($tokenstream:ident as $ty:ty) => {
        match syn::parse::<$ty>($tokenstream.clone()) {
            Ok(data) => data,
            Err(err) => {
                return $crate::token_stream_with_error($tokenstream, err);
            }
        }
    };
}

mod accumulator;
mod db;
mod db_lifetime;
mod debug;
mod fn_util;
mod hygiene;
mod input;
mod interned;
mod options;
mod salsa_struct;
mod salsa_value;
mod supertype;
mod tracked;
mod tracked_fn;
mod tracked_impl;
mod tracked_struct;
mod xform;

/// Defines a type whose values can be accumulated by tracked functions.
///
/// Accumulated values are auxiliary outputs, such as diagnostics, collected while a tracked query
/// runs. They are stored alongside the query's memoized result but do not contribute to that result
/// or its equality.
///
/// The macro implements [`salsa::Accumulator`] for the annotated struct.
///
/// See [accumulators in the `salsa` crate documentation] for their semantics and lifecycle.
///
/// This macro accepts no options. The annotated type must be a struct and implement
/// [`Send`] + [`Sync`] + [`UnwindSafe`] + `'static`.
///
/// # Example
///
/// ```ignore
/// #[salsa::accumulator]
/// struct Diagnostic(String);
///
/// #[salsa::tracked]
/// fn check(db: &dyn salsa::Database) {
///     salsa::Accumulator::accumulate(Diagnostic("something went wrong".into()), db);
/// }
/// ```
///
/// [`salsa::Accumulator`]: https://docs.rs/salsa/latest/salsa/trait.Accumulator.html
/// [`UnwindSafe`]: std::panic::UnwindSafe
/// [accumulators in the `salsa` crate documentation]: https://docs.rs/salsa/latest/salsa/#accumulators
#[proc_macro_attribute]
pub fn accumulator(args: TokenStream, input: TokenStream) -> TokenStream {
    accumulator::accumulator(args, input)
}

/// Defines a Salsa database struct or database trait.
///
/// A database is the state container passed to Salsa operations. Its storage holds inputs, tracked
/// and interned values, and memoized query results.
///
/// This macro accepts no options. Its effect depends on the annotated item:
///
/// - On a struct, it implements Salsa's storage plumbing. The struct must have named fields and
///   one of them must be named `storage`, conventionally with type [`salsa::Storage<Self>`].
/// - On a trait, it adds the hidden methods Salsa uses to view a database as that trait. Database
///   traits conventionally extend [`salsa::Database`].
/// - On a trait implementation, it implements those hidden view methods. Every implementation of
///   a trait annotated with `#[salsa::db]` must also carry `#[salsa::db]`.
///
/// # Example
///
/// ```ignore
/// #[salsa::db]
/// #[derive(Clone, Default)]
/// struct MyDatabase {
///     storage: salsa::Storage<Self>,
/// }
///
/// #[salsa::db]
/// trait MyDatabaseView: salsa::Database {}
///
/// #[salsa::db]
/// impl MyDatabaseView for MyDatabase {}
///
/// #[salsa::db]
/// impl salsa::Database for MyDatabase {}
/// ```
///
/// [`salsa::Database`]: https://docs.rs/salsa/latest/salsa/trait.Database.html
/// [`salsa::Storage<Self>`]: https://docs.rs/salsa/latest/salsa/struct.Storage.html
#[proc_macro_attribute]
pub fn db(args: TokenStream, input: TokenStream) -> TokenStream {
    db::db(args, input)
}

/// Defines an interned struct.
///
/// All fields jointly determine the struct's identity. Within a revision, every occurrence of equal
/// field values maps to the same compact handle. Interned fields are immutable.
///
/// The annotated item must be a struct with named fields. It may declare one lifetime parameter,
/// which Salsa treats as the database lifetime, but no type or const parameters. The generated
/// struct is [`Copy`] and provides a constructor and field getters. Every field type must implement
/// [`Clone`] + [`Eq`] + [`Hash`] + [`Send`] + [`Sync`]. A field whose type is unconditionally
/// `'static` is accepted directly; any other field must implement [`salsa::SalsaValue`].
///
/// See [interned structs in the `salsa` crate documentation] for their identity and lifecycle.
///
/// # Options
///
/// Options are comma-separated inside the attribute:
///
/// - `constructor = IDENT` renames the generated constructor from `new` to `IDENT`.
/// - `debug` implements [`Debug`] using the field values when a database is attached to the current
///   thread. The generated `default_debug_fmt` method can also be called from a manual [`Debug`]
///   implementation.
/// - `revisions = EXPR` sets the minimum number of active revisions an unused value is retained
///   before its slot may be reused. The default is `3`. The value must be nonzero; `usize::MAX`
///   disables reuse.
/// - `heap_size = PATH` records heap use for Salsa's unstable memory-usage reporting. `PATH` must
///   accept a reference to the tuple of all fields and return its heap allocation size in bytes.
/// - `persist` enables persistent caching when Salsa's `persistence` feature is enabled. Fields
///   are serialized as a tuple with [`serde`] by default.
/// - `persist(serialize = PATH, deserialize = PATH)` enables persistence with custom tuple
///   serialization functions. Either path may be omitted to use the corresponding [`serde`]
///   implementation.
///
/// ## Legacy adapters
///
/// These options exist to adapt older code or external representations to Salsa. New code should
/// use the default lifetime-bearing struct and [`salsa::Id`], and its field types should implement
/// [`salsa::SalsaValue`].
///
/// - `id = PATH` uses `PATH` as a legacy ID adapter instead of [`salsa::Id`]. The custom type must
///   implement [`Copy`] + [`Clone`] + [`PartialEq`] + [`Eq`] + [`Hash`] as well as
///   `salsa::plumbing::AsId` and `salsa::plumbing::FromId`.
/// - **Unsafe: `unsafe(no_lifetime)` is strongly discouraged.** It adapts code that cannot carry
///   the database lifetime by generating a struct without one. This bypasses the compile-time
///   guarantee that an interned handle cannot outlive its database. It must be combined with
///   `revisions = usize::MAX` so that interned slots are never reclaimed or reused.
/// - **Unsafe: `unsafe(non_salsa_values)` is strongly discouraged.** It adapts field types that do
///   not implement [`salsa::SalsaValue`] by suppressing the generated checks. The caller becomes
///   responsible for ensuring retained values remain valid across revisions. Prefer adapting only
///   the affected field with `#[salsa_value(unsafe(prove_safe_to_retain_manually))]`.
///
/// # Field attributes
///
/// Every field generates a getter with the same name and visibility as the field. These helper
/// attributes configure that getter:
///
/// - `#[returns(MODE)]` selects how the getter returns the field. `ref` (the default) returns
///   `&FieldTy`; `clone` returns an owned `FieldTy` using [`Clone`]; `copy` returns an owned
///   `FieldTy` using [`Copy`]; and `deref` uses [`Deref`] to return
///   `&<FieldTy as Deref>::Target`. `as_ref` and `as_deref` use [`salsa::SalsaAsRef`] and
///   [`salsa::SalsaAsDeref`] to return borrowed forms such as `Option<&T>` and
///   `Option<&T::Target>`. Every borrowed result is tied to the database borrow.
/// - `#[get(IDENT)]` renames the generated getter.
/// - **Unsafe: `#[salsa_value(unsafe(prove(Predicate, ...)))]`** replaces the retention check with
///   predicates that must hold for every database lifetime. The compiler verifies the predicates,
///   but the caller must ensure they imply that Salsa can retain the field and expose it with a later
///   database lifetime.
/// - **Unsafe: `#[salsa_value(unsafe(prove_safe_to_retain_manually))]`** suppresses the retention
///   check for this field without adding predicates. The caller must ensure Salsa can retain the
///   field and expose it with a later database lifetime.
///
/// Other attributes, including documentation and lint attributes, are copied to the generated
/// getter.
///
/// # Example
///
/// ```ignore
/// #[salsa::interned(debug)]
/// struct Name<'db> {
///     #[returns(deref)]
///     text: String,
///     #[returns(copy)]
///     #[get(disambiguator)]
///     index: u32,
/// }
/// ```
///
/// [`Debug`]: std::fmt::Debug
/// [`Deref`]: std::ops::Deref
/// [`Hash`]: std::hash::Hash
/// [`salsa::Id`]: https://docs.rs/salsa/latest/salsa/struct.Id.html
/// [`salsa::SalsaAsDeref`]: https://docs.rs/salsa/latest/salsa/trait.SalsaAsDeref.html
/// [`salsa::SalsaAsRef`]: https://docs.rs/salsa/latest/salsa/trait.SalsaAsRef.html
/// [`salsa::SalsaValue`]: https://docs.rs/salsa/latest/salsa/trait.SalsaValue.html
/// [`serde`]: https://docs.rs/serde/latest/serde/
/// [interned structs in the `salsa` crate documentation]: https://docs.rs/salsa/latest/salsa/#interned-structs
/// [return mode]: https://docs.rs/salsa/latest/salsa/#return-modes
#[proc_macro_attribute]
pub fn interned(args: TokenStream, input: TokenStream) -> TokenStream {
    interned::interned(args, input)
}

/// Derives a heterogeneous query key from an enum of Salsa structs.
///
/// Use a supertype when one tracked function should accept several input, tracked, or interned
/// struct types. Salsa uses the wrapped struct's ID directly as the query key, while its concrete
/// Salsa struct type determines the enum variant. Every wrapped value is therefore memoized
/// independently.
///
/// Variants may also wrap another supertype, allowing supertypes to be nested. A concrete Salsa
/// struct type must be reachable through exactly one variant, including through nested supertypes,
/// so that Salsa can determine its enum variant unambiguously.
///
/// See [supertypes in the `salsa` crate documentation] for more details.
///
/// # Example
///
/// ```ignore
/// #[salsa::input]
/// struct File {
///     #[returns(deref)]
///     path: String,
/// }
///
/// #[salsa::interned]
/// struct Symbol<'db> {
///     #[returns(deref)]
///     name: String,
/// }
///
/// #[derive(Clone, Copy, PartialEq, Eq, Hash, salsa::Supertype)]
/// enum Source<'db> {
///     File(File),
///     Symbol(Symbol<'db>),
/// }
///
/// #[salsa::tracked(returns(deref))]
/// fn display_name<'db>(db: &'db dyn salsa::Database, source: Source<'db>) -> String {
///     let name = match source {
///         Source::File(file) => file.path(db),
///         Source::Symbol(symbol) => symbol.name(db),
///     };
///     name.to_owned()
/// }
/// ```
///
/// [supertypes in the `salsa` crate documentation]: https://docs.rs/salsa/latest/salsa/#supertypes
#[proc_macro_derive(Supertype)]
pub fn supertype(input: TokenStream) -> TokenStream {
    supertype::supertype(input)
}

/// Defines a mutable input to a Salsa database.
///
/// Each constructed input has a distinct identity that remains stable when its fields are changed.
/// Reading a field records a dependency on that field; setting it invalidates queries that read it.
///
/// The macro replaces a named-field struct with a compact, [`Copy`] Salsa ID and generates a
/// constructor, a builder, and getter and setter methods for every field.
///
/// See [input structs in the `salsa` crate documentation] for their identity, field-level
/// dependencies, and lifecycle.
///
/// The annotated item must be a struct with named fields and no generic parameters.
///
/// # Options
///
/// Options are comma-separated inside the attribute:
///
/// - `constructor = IDENT` renames the generated constructor from `new` to `IDENT`.
/// - `debug` implements [`Debug`] using the field values when a database is attached to the current
///   thread. The generated `default_debug_fmt` method can also be called from a manual [`Debug`]
///   implementation.
/// - `singleton` permits only one instance of this input type in a database and generates
///   `try_get(db)` and `get(db)` methods for retrieving it.
/// - `heap_size = PATH` records heap use for Salsa's unstable memory-usage reporting. `PATH` must
///   accept a reference to the tuple of all fields and return its heap allocation size in bytes.
/// - `persist` enables persistent caching when Salsa's `persistence` feature is enabled. Fields
///   are serialized as a tuple with [`serde`] by default.
/// - `persist(serialize = PATH, deserialize = PATH)` enables persistence with custom tuple
///   serialization functions. Either path may be omitted to use the corresponding [`serde`]
///   implementation.
///
/// # Field attributes
///
/// Every field generates getter and setter methods with the same name and visibility as the field.
/// These helper attributes configure those methods:
///
/// - `#[returns(MODE)]` selects how the getter returns the field. `ref` (the default) returns
///   `&FieldTy`; `clone` returns an owned `FieldTy` using [`Clone`]; `copy` returns an owned
///   `FieldTy` using [`Copy`]; and `deref` uses [`Deref`] to return
///   `&<FieldTy as Deref>::Target`. `as_ref` and `as_deref` use [`salsa::SalsaAsRef`] and
///   [`salsa::SalsaAsDeref`] to return borrowed forms such as `Option<&T>` and
///   `Option<&T::Target>`. Every borrowed result is tied to the database borrow.
/// - `#[get(IDENT)]` renames the generated getter.
/// - `#[set(IDENT)]` renames the generated setter.
/// - `#[default]` initializes the field with [`Default::default`], omits it from the constructor's
///   arguments, and adds a builder method for overriding the default.
///
/// Input fields do not support `#[salsa_value(...)]`. Input field types are stored without database
/// lifetime rebinding, so neither conditional nor unconditional retention proofs apply.
///
/// Other attributes, including documentation and lint attributes, are copied to the generated
/// getter.
///
/// [`Debug`]: std::fmt::Debug
/// [`Deref`]: std::ops::Deref
/// [`salsa::SalsaAsDeref`]: https://docs.rs/salsa/latest/salsa/trait.SalsaAsDeref.html
/// [`salsa::SalsaAsRef`]: https://docs.rs/salsa/latest/salsa/trait.SalsaAsRef.html
/// [`serde`]: https://docs.rs/serde/latest/serde/
/// [input structs in the `salsa` crate documentation]: https://docs.rs/salsa/latest/salsa/#input-structs
/// [return mode]: https://docs.rs/salsa/latest/salsa/#return-modes
#[proc_macro_attribute]
pub fn input(args: TokenStream, input: TokenStream) -> TokenStream {
    input::input(args, input)
}

/// Defines a tracked struct or function, or enables tracked methods in an `impl` block.
///
/// The accepted syntax and generated API depend on the annotated item. See the sections below for
/// the options and field attributes accepted by each form.
///
/// # Tracked structs
///
/// A tracked struct represents a derived entity created during tracked-function execution. Its
/// identity belongs to the producing query, which can recreate and update the entity in a later
/// revision.
///
/// The annotated item must have named fields and exactly one lifetime parameter, conventionally
/// `'db`; type and const parameters are not supported. A field whose type is unconditionally
/// `'static` is accepted directly; any other field must implement [`salsa::SalsaValue`].
///
/// See [tracked structs in the `salsa` crate documentation] for their identity, change tracking,
/// and lifecycle.
///
/// ## Struct options
///
/// - `constructor = IDENT` renames the generated constructor from `new` to `IDENT`.
/// - `debug` implements [`Debug`] using the field values when a database is attached to the current
///   thread. The generated `default_debug_fmt` method can also be called from a manual [`Debug`]
///   implementation.
/// - `heap_size = PATH` records heap use for Salsa's unstable memory-usage reporting. `PATH` must
///   accept a reference to the tuple of all fields and return its heap allocation size in bytes.
/// - `persist` enables persistent caching when Salsa's `persistence` feature is enabled. Fields
///   are serialized as a tuple with [`serde`] by default.
/// - `persist(serialize = PATH, deserialize = PATH)` enables persistence with custom tuple
///   serialization functions. Either path may be omitted to use the corresponding [`serde`]
///   implementation.
///
/// ## Struct field attributes
///
/// - `#[tracked]` excludes the field from the struct's identity. When the producing query recreates
///   the same entity with a new value for this field, Salsa updates the existing entity instead of
///   creating a new one. Reads of the field are tracked separately, so changing it invalidates only
///   queries that read that field. Use this for properties that may change while the conceptual
///   entity remains the same.
/// - `#[returns(MODE)]` selects how the getter returns the field. `ref` (the default) returns
///   `&FieldTy`; `clone` returns an owned `FieldTy` using [`Clone`]; `copy` returns an owned
///   `FieldTy` using [`Copy`]; and `deref` uses [`Deref`] to return
///   `&<FieldTy as Deref>::Target`. `as_ref` and `as_deref` use [`salsa::SalsaAsRef`] and
///   [`salsa::SalsaAsDeref`] to return borrowed forms such as `Option<&T>` and
///   `Option<&T::Target>`. Every borrowed result is tied to the database borrow.
/// - `#[get(IDENT)]` renames the generated getter.
/// - `#[no_eq]` replaces the stored value and treats the field as changed whenever the struct is
///   recreated, avoiding the [`PartialEq`] requirement. It is most useful together with
///   `#[tracked]`: because the field does not contribute to identity, the struct can retain its
///   identity when recreated, while readers of the field are always invalidated.
/// - **Unsafe: `#[salsa_value(unsafe(prove(Predicate, ...)))]`** replaces the retention check with
///   predicates that must hold for every database lifetime. The compiler verifies the predicates,
///   but the caller must ensure they imply that Salsa can retain the field and expose it with a later
///   database lifetime.
/// - **Unsafe: `#[salsa_value(unsafe(prove_safe_to_retain_manually))]`** suppresses the retention
///   check for this field without adding predicates. The caller must ensure Salsa can retain the
///   field and expose it with a later database lifetime.
///
/// Other attributes, including documentation and lint attributes, are copied to the generated
/// getter.
///
/// # Tracked functions
///
/// A tracked function memoizes its result and records the Salsa values read by its body. Salsa
/// reuses the memoized result while those dependencies remain unchanged.
///
/// The first parameter must be an immutable `&dyn DatabaseTrait`; the remaining parameters form
/// the query key. The function may declare one database lifetime but no type or const parameters.
/// Every key parameter and the output must implement [`Send`] + [`Sync`]. With no key parameters,
/// the function has one memoized query per database. A single key parameter must be a Salsa struct
/// and uses its ID directly. With multiple key parameters, Salsa first interns their tuple to
/// obtain an ID, adding an interning step to every call. Each key parameter must additionally
/// implement [`Clone`] + [`Eq`] + [`Hash`]. Equality and hashing determine whether calls use the
/// same memo, and Salsa always clones the stored tuple when materializing the function arguments.
/// Interned key parameters and outputs whose types are not unconditionally `'static` must implement
/// [`salsa::SalsaValue`].
///
/// See [tracked functions in the `salsa` crate documentation] for query identity, dependency
/// tracking, result equality, and memo lifecycle.
///
/// ## Function options
///
/// - `returns(MODE)` selects how callers receive the memoized result. `ref` (the default) returns
///   `&Output`; `clone` returns an owned `Output` using [`Clone`]; `copy` returns an owned `Output`
///   using [`Copy`]; and `deref` uses [`Deref`] to return `&<Output as Deref>::Target`.
///   `as_ref` and `as_deref` use [`salsa::SalsaAsRef`] and [`salsa::SalsaAsDeref`] to return
///   borrowed forms such as `Option<&T>` and `Option<&T::Target>`. Every borrowed result is tied to
///   the database borrow and remains stored in the query's memo.
/// - `no_eq` treats every newly computed result as changed and removes the output's equality
///   requirement. It cannot be combined with `cycle_fn`.
/// - `specify` generates `FUNCTION::specify(db, key, value)`. It supports queries that have both a
///   per-key incremental implementation and a batch implementation that computes many results at
///   once. The function must take exactly one key argument, and it must be a tracked struct, not an
///   input or interned struct. `specify` must be called during the same tracked query invocation
///   that created the key. It cannot be combined with `lru`. See [specifying query results in the
///   Salsa book] for an example.
/// - `lru = INTEGER` bounds the number of memoized values retained by the function and sets the
///   initial capacity used by `FUNCTION::set_lru_capacity`.
/// - `cycle_initial = EXPR` enables fixed-point cycle recovery and computes the initial value. The
///   expression is called as `(db, cycle_head_id, query_arguments...)`.
/// - `cycle_fn = EXPR` combines successive fixed-point values. It must be accompanied by
///   `cycle_initial` and is called as
///   `(db, cycle, previous_value, new_value, query_arguments...)`. See [fixed-point cycle recovery
///   in the Salsa book] for the convergence requirements and a complete example.
/// - `cycle_result = EXPR` supplies an immediate fallback for cycles instead of fixed-point
///   iteration. It is called with the same arguments as `cycle_initial` and cannot be combined
///   with `cycle_initial` or `cycle_fn`.
/// - `heap_size = PATH` records heap use for Salsa's unstable memory-usage reporting. `PATH` must
///   accept a reference to the output and return its heap allocation size in bytes.
/// - `persist` enables persistent caching when Salsa's `persistence` feature is enabled. The query
///   inputs and output must implement [`serde::Serialize`] and [`serde::Deserialize`].
/// - `self_ty = TYPE` prefixes the query's debug name with `TYPE`. The impl-block form supplies
///   this automatically for methods and associated functions.
///
/// ## Legacy function adapter
///
/// - **Unsafe: `unsafe(non_salsa_values)` is strongly discouraged.** It adapts output or internally
///   interned input types that do not implement [`salsa::SalsaValue`] by suppressing the generated
///   checks. The caller becomes responsible for ensuring retained values remain valid across
///   revisions. Prefer deriving or implementing [`salsa::SalsaValue`] for those types.
///
/// # Tracked impl blocks
///
/// Applying `#[salsa::tracked]` to an inherent or trait `impl` allows individual methods and
/// associated functions in it to also use `#[salsa::tracked(...)]`. The outer attribute accepts no
/// options; inner attributes accept all tracked-function options.
///
/// A tracked method takes `self` by value followed by the database parameter. A tracked associated
/// function takes the database parameter first. Other methods and associated items are left
/// unchanged.
///
/// # Examples
///
/// ```ignore
/// #[salsa::input]
/// struct File {
///     #[returns(deref)]
///     text: String,
/// }
///
/// #[salsa::tracked(returns(copy))]
/// fn word_count(db: &dyn salsa::Database, file: File) -> usize {
///     file.text(db).split_whitespace().count()
/// }
///
/// #[salsa::tracked]
/// impl File {
///     #[salsa::tracked(returns(copy))]
///     fn line_count(self, db: &dyn salsa::Database) -> usize {
///         self.text(db).lines().count()
///     }
/// }
/// ```
///
/// [`Debug`]: std::fmt::Debug
/// [`Deref`]: std::ops::Deref
/// [`Eq`]: std::cmp::Eq
/// [`Hash`]: std::hash::Hash
/// [`salsa::SalsaAsDeref`]: https://docs.rs/salsa/latest/salsa/trait.SalsaAsDeref.html
/// [`salsa::SalsaAsRef`]: https://docs.rs/salsa/latest/salsa/trait.SalsaAsRef.html
/// [`salsa::SalsaValue`]: https://docs.rs/salsa/latest/salsa/trait.SalsaValue.html
/// [`serde`]: https://docs.rs/serde/latest/serde/
/// [`serde::Deserialize`]: https://docs.rs/serde/latest/serde/trait.Deserialize.html
/// [`serde::Serialize`]: https://docs.rs/serde/latest/serde/trait.Serialize.html
/// [fixed-point cycle recovery in the Salsa book]: https://salsa-rs.github.io/salsa/cycles.html#fixed-point-iteration
/// [return mode]: https://docs.rs/salsa/latest/salsa/#return-modes
/// [specifying query results in the Salsa book]: https://salsa-rs.github.io/salsa/overview.html#specify-the-result-of-tracked-functions-for-particular-structs
/// [tracked functions in the `salsa` crate documentation]: https://docs.rs/salsa/latest/salsa/#tracked-functions-and-memoized-values
/// [tracked structs in the `salsa` crate documentation]: https://docs.rs/salsa/latest/salsa/#tracked-structs
#[proc_macro_attribute]
pub fn tracked(args: TokenStream, input: TokenStream) -> TokenStream {
    tracked::tracked(args, input)
}

/// Derives the unsafe [`salsa::SalsaValue`] trait for a struct or enum.
///
/// A field whose type is unconditionally `'static` is accepted directly; any other field must
/// implement [`salsa::SalsaValue`].
///
/// The type may declare at most one lifetime parameter. Type and const parameters are supported;
/// unions are not. Named fields, tuple fields, unit structs, and enum variants are supported.
///
/// # Field attributes
///
/// These field attributes are supported by `derive(SalsaValue)` and by Salsa's `tracked` and
/// `interned` struct macros. On a derived type, conditional predicates become bounds on the
/// generated `SalsaValue` implementation. On tracked and interned structs, they must hold for every
/// database lifetime. The `input` macro supports neither form.
///
/// A field accepts at most one `#[salsa_value(...)]` attribute:
///
/// - **Unsafe: `#[salsa_value(unsafe(prove(Predicate, ...)))]`** suppresses the generated retention
///   check for this field and adds the predicates to the generated `SalsaValue` implementation. The
///   compiler verifies the predicates, but the author must ensure they imply that Salsa can replace
///   the field's database lifetime with `'static` for storage and safely restore it later.
/// - **Unsafe: `#[salsa_value(unsafe(prove_safe_to_retain_manually))]`** suppresses the generated
///   retention check without adding predicates. The author must ensure the field is safe for every
///   generic instantiation accepted by the enclosing type.
///
/// # Safety
///
/// Its field checks establish the structural requirements, but cannot inspect
/// invariants maintained by unsafe code in the derived type's methods. By
/// deriving `SalsaValue`, the author asserts that any such invariants remain
/// valid when Salsa retains the value across revisions and rebinds its database
/// lifetime.
///
/// # Examples
///
/// ```ignore
/// #[derive(salsa::SalsaValue)]
/// struct QueryValue<'db> {
///     item: MyInterned<'db>,
/// }
/// ```
///
/// A conditional proof narrows the generated implementation when an unmodifiable field type does
/// not implement `SalsaValue`:
///
/// ```ignore
/// #[derive(salsa::SalsaValue)]
/// struct QueryValue<T> {
///     #[salsa_value(unsafe(prove(T: salsa::SalsaValue)))]
///     value: ForeignContainer<T>,
/// }
/// ```
///
/// [`salsa::SalsaValue`]: https://docs.rs/salsa/latest/salsa/trait.SalsaValue.html
#[proc_macro_derive(SalsaValue, attributes(salsa_value))]
pub fn salsa_value(input: TokenStream) -> TokenStream {
    let item = parse_macro_input!(input as syn::DeriveInput);
    match salsa_value::salsa_value_derive(item) {
        Ok(tokens) => tokens.into(),
        Err(error) => error.into_compile_error().into(),
    }
}

pub(crate) fn token_stream_with_error(mut tokens: TokenStream, error: syn::Error) -> TokenStream {
    tokens.extend(TokenStream::from(error.into_compile_error()));
    tokens
}

mod kw {
    syn::custom_keyword!(prove);
    syn::custom_keyword!(prove_safe_to_retain_manually);
}
