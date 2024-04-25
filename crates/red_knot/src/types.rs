#![allow(dead_code)]
use crate::ast_ids::NodeKey;
use crate::module::Module;
use crate::symbols::SymbolId;
use crate::{FxDashMap, Name};
use ruff_index::{newtype_index, IndexVec};
use rustc_hash::{FxHashMap, FxHashSet};

/// unique ID for a type
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Type {
    /// the dynamic or gradual type: a statically-unknown set of values
    Any,
    /// the empty set of values
    Never,
    /// unknown type (no annotation)
    /// equivalent to Any, or to object in strict mode
    Unknown,
    /// name is not bound to any value
    Unbound,
    /// a specific function
    Function(FunctionTypeId),
    /// the set of Python objects with a given class in their __class__'s method resolution order
    Class(ClassTypeId),
    Union(UnionTypeId),
    Intersection(IntersectionTypeId),
    // TODO protocols, callable types, overloads, generics, type vars
}

impl Type {
    fn display<'a>(&'a self, store: &'a TypeStore) -> DisplayType<'a> {
        DisplayType { ty: self, store }
    }
}

#[derive(Debug, Default)]
pub(crate) struct TypeStore {
    modules: FxDashMap<Module, ModuleTypeStore>,
}

impl TypeStore {
    fn add_or_get_module(
        &mut self,
        module: Module,
    ) -> dashmap::mapref::one::RefMut<
        '_,
        Module,
        ModuleTypeStore,
        std::hash::BuildHasherDefault<rustc_hash::FxHasher>,
    > {
        self.modules
            .entry(module)
            .or_insert_with(|| ModuleTypeStore::new(module))
    }

    fn get_module(
        &self,
        module: Module,
    ) -> dashmap::mapref::one::Ref<
        '_,
        Module,
        ModuleTypeStore,
        std::hash::BuildHasherDefault<rustc_hash::FxHasher>,
    > {
        self.modules.get(&module).expect("module should exist")
    }

    fn add_function(&mut self, module: Module, name: &str) -> Type {
        self.add_or_get_module(module).add_function(name)
    }

    fn add_class(&mut self, module: Module, name: &str) -> Type {
        self.add_or_get_module(module).add_class(name)
    }

    fn add_union(&mut self, module: Module, elems: &[Type]) -> Type {
        self.add_or_get_module(module).add_union(elems)
    }

    fn add_intersection(&mut self, module: Module, positive: &[Type], negative: &[Type]) -> Type {
        self.add_or_get_module(module)
            .add_intersection(positive, negative)
    }
}

#[derive(Copy, Clone, Debug, Hash, Eq, PartialEq)]
pub(crate) struct FunctionTypeId {
    module: Module,
    func_id: ModuleFunctionTypeId,
}

#[derive(Copy, Clone, Debug, Hash, Eq, PartialEq)]
pub(crate) struct ClassTypeId {
    module: Module,
    class_id: ModuleClassTypeId,
}

#[derive(Copy, Clone, Debug, Hash, Eq, PartialEq)]
pub(crate) struct UnionTypeId {
    module: Module,
    union_id: ModuleUnionTypeId,
}

#[derive(Copy, Clone, Debug, Hash, Eq, PartialEq)]
pub(crate) struct IntersectionTypeId {
    module: Module,
    intersection_id: ModuleIntersectionTypeId,
}

#[newtype_index]
struct ModuleFunctionTypeId;

#[newtype_index]
struct ModuleClassTypeId;

#[newtype_index]
struct ModuleUnionTypeId;

#[newtype_index]
struct ModuleIntersectionTypeId;

// Using Arc in these maps allows us to access details of a type without locking the entire
// ModuleTypeStore to writes while we are doing it, but it also comes with the risk of high thread
// contention on the atomic reference counts of high-traffic types, even just for reading type
// details. This approach will only parallelize well if we are careful to limit our reads of type
// details. We can do this by caching (by Type id) all type judgments we make that require looking
// at type details, so we aren't having to peek into the details repeatedly.
#[derive(Debug)]
struct ModuleTypeStore {
    module: Module,
    /// arena of all function types defined in this module
    functions: IndexVec<ModuleFunctionTypeId, FunctionType>,
    /// arena of all class types defined in this module
    classes: IndexVec<ModuleClassTypeId, ClassType>,
    /// arenda of all union types created in this module
    unions: IndexVec<ModuleUnionTypeId, UnionType>,
    /// arena of all intersection types created in this module
    intersections: IndexVec<ModuleIntersectionTypeId, IntersectionType>,
    /// cached types of symbols in this module
    symbol_types: FxHashMap<SymbolId, Type>,
    /// cached types of AST nodes in this module
    node_types: FxHashMap<NodeKey, Type>,
}

impl ModuleTypeStore {
    fn new(module: Module) -> Self {
        Self {
            module,
            functions: IndexVec::default(),
            classes: IndexVec::default(),
            unions: IndexVec::default(),
            intersections: IndexVec::default(),
            symbol_types: FxHashMap::default(),
            node_types: FxHashMap::default(),
        }
    }

    fn add_function(&mut self, name: &str) -> Type {
        let func_id = self.functions.push(FunctionType {
            name: Name::new(name),
        });
        Type::Function(FunctionTypeId {
            module: self.module,
            func_id,
        })
    }

    fn add_class(&mut self, name: &str) -> Type {
        let class_id = self.classes.push(ClassType {
            name: Name::new(name),
        });
        Type::Class(ClassTypeId {
            module: self.module,
            class_id,
        })
    }

    fn add_union(&mut self, elems: &[Type]) -> Type {
        let union_id = self.unions.push(UnionType {
            elements: FxHashSet::from_iter(elems.iter().copied()),
        });
        Type::Union(UnionTypeId {
            module: self.module,
            union_id,
        })
    }

    fn add_intersection(&mut self, positive: &[Type], negative: &[Type]) -> Type {
        let intersection_id = self.intersections.push(IntersectionType {
            positive: FxHashSet::from_iter(positive.iter().copied()),
            negative: FxHashSet::from_iter(negative.iter().copied()),
        });
        Type::Intersection(IntersectionTypeId {
            module: self.module,
            intersection_id,
        })
    }

    fn get_function(&self, func_id: ModuleFunctionTypeId) -> &FunctionType {
        &self.functions[func_id]
    }

    fn get_class(&self, class_id: ModuleClassTypeId) -> &ClassType {
        &self.classes[class_id]
    }

    fn get_union(&self, union_id: ModuleUnionTypeId) -> &UnionType {
        &self.unions[union_id]
    }

    fn get_intersection(&self, intersection_id: ModuleIntersectionTypeId) -> &IntersectionType {
        &self.intersections[intersection_id]
    }
}

#[derive(Copy, Clone, Debug)]
struct DisplayType<'a> {
    ty: &'a Type,
    store: &'a TypeStore,
}

impl std::fmt::Display for DisplayType<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.ty {
            Type::Any => f.write_str("Any"),
            Type::Never => f.write_str("Never"),
            Type::Unknown => f.write_str("Unknown"),
            Type::Unbound => f.write_str("Unbound"),
            Type::Class(class_id) => f.write_str(
                self.store
                    .get_module(class_id.module)
                    .get_class(class_id.class_id)
                    .name(),
            ),
            Type::Function(func_id) => f.write_str(
                self.store
                    .get_module(func_id.module)
                    .get_function(func_id.func_id)
                    .name(),
            ),
            Type::Union(union_id) => self
                .store
                .get_module(union_id.module)
                .get_union(union_id.union_id)
                .display(f, self.store),
            Type::Intersection(int_id) => self
                .store
                .get_module(int_id.module)
                .get_intersection(int_id.intersection_id)
                .display(f, self.store),
        }
    }
}

#[derive(Debug)]
pub(crate) struct ClassType {
    name: Name,
}

impl ClassType {
    fn name(&self) -> &str {
        self.name.as_str()
    }
}

#[derive(Debug)]
pub(crate) struct FunctionType {
    name: Name,
}

impl FunctionType {
    fn name(&self) -> &str {
        self.name.as_str()
    }
}

#[derive(Debug)]
pub(crate) struct UnionType {
    // the union type includes values in any of these types
    elements: FxHashSet<Type>,
}

impl UnionType {
    fn display(&self, f: &mut std::fmt::Formatter<'_>, store: &TypeStore) -> std::fmt::Result {
        f.write_str("(")?;
        let mut first = true;
        for ty in self.elements.iter() {
            if !first {
                f.write_str(" | ")?;
            };
            first = false;
            write!(f, "{}", ty.display(store))?;
        }
        f.write_str(")")
    }
}

// Negation types aren't expressible in annotations, and are most likely to arise from type
// narrowing along with intersections (e.g. `if not isinstance(...)`), so we represent them
// directly in intersections rather than as a separate type. This sacrifices some efficiency in the
// case where a Not appears outside an intersection (unclear when that could even happen, but we'd
// have to represent it as a single-element intersection if it did) in exchange for better
// efficiency in the not-within-intersection case.
#[derive(Debug)]
pub(crate) struct IntersectionType {
    // the intersection type includes only values in all of these types
    positive: FxHashSet<Type>,
    // negated elements of the intersection, e.g.
    negative: FxHashSet<Type>,
}

impl IntersectionType {
    fn display(&self, f: &mut std::fmt::Formatter<'_>, store: &TypeStore) -> std::fmt::Result {
        f.write_str("(")?;
        let mut first = true;
        for (neg, ty) in self
            .positive
            .iter()
            .map(|ty| (false, ty))
            .chain(self.negative.iter().map(|ty| (true, ty)))
        {
            if !first {
                f.write_str(" & ")?;
            };
            first = false;
            if neg {
                f.write_str("~")?;
            };
            write!(f, "{}", ty.display(store))?;
        }
        f.write_str(")")
    }
}

#[cfg(test)]
mod tests {
    use crate::module::test_module;
    use crate::types::TypeStore;

    #[test]
    fn add_class() {
        let mut store = TypeStore::default();
        let module = test_module(0);
        let class = store.add_class(module, "C");
        assert_eq!(format!("{}", class.display(&store)), "C");
    }

    #[test]
    fn add_function() {
        let mut store = TypeStore::default();
        let module = test_module(0);
        let func = store.add_function(module, "func");
        assert_eq!(format!("{}", func.display(&store)), "func");
    }
}
