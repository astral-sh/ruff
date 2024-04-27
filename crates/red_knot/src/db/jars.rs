use crate::db::QueryResult;

/// Gives access to a specific jar in the database.
///
/// Nope, the terminology isn't borrowed from Java but from Salsa <https://salsa-rs.github.io/salsa/>,
/// which is an analogy to storing the salsa in different jars.
///
/// The basic idea is that each crate can define its own jar and the jars can be combined to a single
/// database in the top level crate. Each crate also defines its own `Database` trait. The combination of
/// `Database` trait and the jar allows to write queries in isolation without having to know how they get composed at the upper levels.
///
/// Salsa further defines a `HasIngredient` trait which slices the jar to a specific storage (e.g. a specific cache).
/// We don't need this just jet because we write our queries by hand. We may want a similar trait if we decide
/// to use a macro to generate the queries.
pub trait HasJar<T> {
    /// Gives a read-only reference to the jar.
    fn jar(&self) -> QueryResult<&T>;

    /// Gives a mutable reference to the jar.
    fn jar_mut(&mut self) -> &mut T;
}

pub trait HasJars {
    type Jars;

    fn jars(&self) -> &QueryResult<Self::Jars>;

    fn jars_unwrap(&self) -> &Self::Jars;

    fn jars_mut(&mut self) -> &mut Self::Jars;
}
