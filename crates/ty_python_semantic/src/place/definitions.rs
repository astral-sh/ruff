use smallvec::SmallVec;
use ty_python_core::definition::{Definition, DefinitionState};
use ty_python_core::{
    BindingWithConstraintsIterator, ProgramFile, global_scope, place_table, use_def_map,
};

use crate::Db;
use crate::place::Place;
use crate::place_load::{
    ImplicitPlaceLoad, PlaceLoadResolver, PlaceLoadSourceKind, ResolvedPlaceLoadSource,
};
use crate::reachability::ReachabilityConstraintsExtension;

/// Returns the definitions that may supply a module global at the end of its scope.
pub(crate) fn definitions_for_module_global<'db>(
    db: &'db dyn Db,
    file: ProgramFile<'db>,
    name: &str,
) -> Option<DefinitionResolution<'db>> {
    let scope = global_scope(db, file);
    let symbol = place_table(db, scope).symbol_id(name)?;
    Some(DefinitionResolution::from_bindings(
        db,
        use_def_map(db, scope).end_of_scope_symbol_bindings(symbol),
    ))
}

/// The result of querying the definitions that may supply a name's value.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct DefinitionResolution<'db> {
    /// The definitions found for the name.
    definitions: SmallVec<[Definition<'db>; 2]>,
    /// Whether resolving this use of a name requires crossing a `global` or `nonlocal` declaration.
    crosses_scope_declaration: bool,
    /// Whether or not `definitions` identifies all bindings that can supply the value of the name.
    /// For instance, this is false when the name's value can come from an implicit value such as a
    /// builtin, or when `definitions` contains a synthetic definition like a synthetic loop header.
    identifies_all_bindings: bool,
}

impl<'db> DefinitionResolution<'db> {
    fn new() -> Self {
        Self {
            definitions: SmallVec::new(),
            crosses_scope_declaration: false,
            identifies_all_bindings: true,
        }
    }

    pub(crate) fn from_resolver(db: &'db dyn Db, resolver: PlaceLoadResolver<'db, '_>) -> Self {
        let mut resolution = Self::new();
        let resolved_load = resolver.resolve(|source| resolution.add_source(db, source));
        resolution.crosses_scope_declaration = resolved_load.crosses_scope_declaration;
        resolution
    }

    pub(crate) fn from_bindings(
        db: &'db dyn Db,
        mut bindings: BindingWithConstraintsIterator<'db, 'db>,
    ) -> Self {
        let mut resolution = Self::new();

        while let Some(binding) = bindings.next() {
            if let DefinitionState::Defined(definition) = binding.binding
                && !bindings
                    .reachability_constraints()
                    .evaluate(db, bindings.predicates(), binding.reachability_constraint)
                    .is_always_false()
            {
                resolution.push_definition(db, definition);
            }
        }

        resolution
    }

    fn from_place_load_source(
        db: &'db dyn Db,
        resolved_source: &ResolvedPlaceLoadSource<'_, 'db>,
    ) -> Self {
        match resolved_source.kind() {
            PlaceLoadSourceKind::Bindings(bindings) => Self::from_bindings(db, bindings.clone()),
            PlaceLoadSourceKind::DefinitionsFromOwningScope { scope, id } => {
                Self::from_bindings(db, use_def_map(db, *scope).reachable_bindings(*id))
            }
            PlaceLoadSourceKind::Implicit(ImplicitPlaceLoad::ExplicitGlobalSymbol {
                file,
                name,
            }) => {
                let scope = global_scope(db, *file);
                let Some(symbol) = place_table(db, scope).symbol_id(name) else {
                    return Self::new();
                };
                Self::from_bindings(db, use_def_map(db, scope).reachable_symbol_bindings(symbol))
            }
            PlaceLoadSourceKind::Implicit(ImplicitPlaceLoad::DunderClass(class_def)) => {
                let mut resolution = Self::new();
                resolution.push_definition(db, *class_def);

                // The statement `class C` binds the name `C`. Python fills the `__class__` cell
                // for its methods implicitly, so no statement binds the name `__class__` there.
                resolution.identifies_all_bindings = false;

                resolution
            }
            PlaceLoadSourceKind::Implicit(_) => {
                Self::from_place_without_definition(resolved_source.place().place)
            }
        }
    }

    fn from_place_without_definition(place: Place<'db>) -> Self {
        Self {
            // If the fallback is undefined, there is no missing binding to account for.
            identifies_all_bindings: place.is_undefined(),
            ..Self::new()
        }
    }

    fn add_source(&mut self, db: &'db dyn Db, resolved_source: &ResolvedPlaceLoadSource<'_, 'db>) {
        self.extend(db, Self::from_place_load_source(db, resolved_source));
    }

    fn push_definition(&mut self, db: &'db dyn Db, definition: Definition<'db>) {
        self.identifies_all_bindings &= definition.kind(db).is_user_visible();
        if !self.definitions.contains(&definition) {
            self.definitions.push(definition);
        }
    }

    fn extend(&mut self, db: &'db dyn Db, other: Self) {
        for definition in other.definitions {
            self.push_definition(db, definition);
        }
        self.identifies_all_bindings &= other.identifies_all_bindings;
    }

    /// Returns the definitions found for the name.
    pub fn definitions(&self) -> &[Definition<'db>] {
        &self.definitions
    }

    /// Returns whether this resolution required crossing a `global` or `nonlocal` declaration.
    pub fn crosses_scope_declaration(&self) -> bool {
        self.crosses_scope_declaration
    }

    /// Returns whether [`Self::definitions()`] identifies the Python code that binds the name
    /// on every path where the name can have a value.
    ///
    /// For example, if a module conditionally assigns to `len`, a later call to `len(items)`
    /// can use either the assignment or the builtin. Python supplies the builtin implicitly,
    /// so the returned definitions identify only the assignment and this method returns `false`.
    ///
    /// A synthetic definition also makes this method return `false`. A path on which the name
    /// is unbound has no binding to identify.
    pub fn identifies_all_bindings(&self) -> bool {
        self.identifies_all_bindings
    }
}
