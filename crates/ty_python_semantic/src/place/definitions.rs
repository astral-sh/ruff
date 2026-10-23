use rustc_hash::FxHashSet;
use smallvec::SmallVec;
use ty_python_core::definition::{Definition, DefinitionState};
use ty_python_core::{BindingWithConstraintsIterator, global_scope, place_table, use_def_map};

use crate::Db;
use crate::place::{Place, loop_header_reachability};
use crate::place_load::{
    ImplicitPlaceLoad, PlaceLoadResolver, PlaceLoadSourceKind, ResolvedPlaceLoadSource,
};
use crate::reachability::ReachabilityConstraintsExtension;

/// The result of querying the definitions that may supply a name's value at a particular use.
#[derive(Debug)]
pub struct DefinitionResolution<'db> {
    /// The definitions found for this use.
    definitions: SmallVec<[Definition<'db>; 2]>,
    /// Whether resolving this use of a name requires crossing a `global` or `nonlocal` declaration.
    crosses_scope_declaration: bool,
    /// Whether or not `definitions` identifies all bindings that can supply the value of the name.
    /// For instance, this is false when the name's value can come from an implicit value such as a
    /// builtin, or when `definitions` contains a synthetic definition.
    identifies_all_bindings: bool,
}

#[allow(dead_code, reason = "awaiting IDE integration")]
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
        bindings: BindingWithConstraintsIterator<'db, 'db>,
    ) -> Self {
        let mut resolution = Self::new();
        resolution.add_bindings(db, bindings);

        resolution
    }

    fn add_source(&mut self, db: &'db dyn Db, resolved_source: &ResolvedPlaceLoadSource<'_, 'db>) {
        let has_definitions = match resolved_source.kind() {
            PlaceLoadSourceKind::Bindings(bindings) => self.add_bindings(db, bindings.clone()),
            PlaceLoadSourceKind::DefinitionsFromOwningScope { scope, id } => {
                self.add_bindings(db, use_def_map(db, *scope).reachable_bindings(*id))
            }
            PlaceLoadSourceKind::Implicit(ImplicitPlaceLoad::ExplicitGlobalSymbol {
                file,
                name,
            }) => {
                let scope = global_scope(db, *file);
                let Some(symbol) = place_table(db, scope).symbol_id(name) else {
                    return;
                };
                self.add_bindings(db, use_def_map(db, scope).reachable_symbol_bindings(symbol))
            }
            PlaceLoadSourceKind::Implicit(ImplicitPlaceLoad::DunderClass(class_def)) => {
                self.push_definition(db, *class_def);

                // The statement `class C` binds the name `C`. Python fills the `__class__` cell
                // for its methods implicitly, so no statement binds the name `__class__` there.
                self.identifies_all_bindings = false;
                return;
            }
            PlaceLoadSourceKind::Implicit(_) => {
                // If the fallback is undefined, there is no missing binding to account for.
                self.identifies_all_bindings &= resolved_source.place().place.is_undefined();
                return;
            }
        };

        if !has_definitions
            && let Place::Defined(defined) = resolved_source.place().place
            && defined.origin.is_declared()
        {
            // An annotation such as `widget: int` declares a type without binding a value.
            self.identifies_all_bindings = false;
        }
    }

    fn add_bindings(
        &mut self,
        db: &'db dyn Db,
        mut bindings: BindingWithConstraintsIterator<'db, 'db>,
    ) -> bool {
        let mut has_definitions = false;
        while let Some(binding) = bindings.next() {
            if let DefinitionState::Defined(definition) = binding.binding
                && !bindings
                    .reachability_constraints()
                    .evaluate(db, bindings.predicates(), binding.reachability_constraint)
                    .is_always_false()
            {
                has_definitions |= self.push_definition(db, definition);
            }
        }

        has_definitions
    }

    fn push_definition(&mut self, db: &'db dyn Db, definition: Definition<'db>) -> bool {
        let mut has_definitions = false;
        let mut pending = SmallVec::<[_; 2]>::new();
        pending.push(definition);
        let mut expanded_headers = FxHashSet::default();

        while let Some(definition) = pending.pop() {
            let kind = definition.kind(db);
            if kind.is_loop_header() {
                // Expand each header only once because it can appear in multiple binding lists.
                if expanded_headers.insert(definition) {
                    pending.extend(
                        loop_header_reachability(db, definition)
                            .reachable_bindings
                            .iter()
                            .rev()
                            .map(|binding| binding.definition),
                    );
                }
                continue;
            }

            has_definitions = true;
            self.identifies_all_bindings &= kind.is_user_visible();
            if !self.definitions.contains(&definition) {
                self.definitions.push(definition);
            }
        }

        has_definitions
    }

    /// Returns the definitions found for this use of the name.
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
