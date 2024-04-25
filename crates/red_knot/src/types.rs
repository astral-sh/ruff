#![allow(dead_code)]
use crate::Name;
use ruff_index::{newtype_index, IndexVec};
use rustc_hash::FxHashSet;

#[newtype_index]
pub(crate) struct TypeId;

impl TypeId {
    pub(crate) fn ty(self, env: &TypeEnvironment) -> &Type {
        env.type_for_id(self)
    }
}

/// Arena holding all known types
#[derive(Debug, Default)]
pub(crate) struct TypeEnvironment {
    types_by_id: IndexVec<TypeId, Type>,
}

impl TypeEnvironment {
    pub(crate) fn add_class(&mut self, name: &str) -> TypeId {
        let class = Type::Instance {
            instance_of: ClassType {
                name: Name::new(name),
            },
        };
        self.types_by_id.push(class)
    }

    pub(crate) fn add_function(&mut self, name: &str) -> TypeId {
        let function = Type::Function(FunctionType {
            name: Name::new(name),
        });
        self.types_by_id.push(function)
    }

    pub(crate) fn type_for_id(&self, type_id: TypeId) -> &Type {
        &self.types_by_id[type_id]
    }
}

#[derive(Debug)]
pub(crate) enum Type {
    /// a specific function
    Function(FunctionType),
    /// the set of Python objects with this class in their __class__'s method resolution order
    Instance {
        instance_of: ClassType,
    },
    Union(UnionType),
    Intersection(IntersectionType),
    /// the dynamic or gradual type: a statically-unknown set of values
    Any,
    /// the empty set of values
    Never,
    /// unknown type (no annotation)
    /// equivalent to Any, or to object in strict mode
    Unknown,
    /// name is not bound to any value
    Unbound,
    // TODO protocols, callable types, overloads, generics, type vars
}

impl Type {
    pub(crate) fn name(&self) -> Option<&str> {
        match self {
            Type::Instance { instance_of: class } => Some(class.name()),
            Type::Function(func) => Some(func.name()),
            Type::Union(_) => None,
            Type::Intersection(_) => None,
            Type::Any => Some("Any"),
            Type::Never => Some("Never"),
            Type::Unknown => Some("Unknown"),
            Type::Unbound => Some("Unbound"),
        }
    }

    fn display<'a>(&'a self, env: &'a TypeEnvironment) -> DisplayType<'a> {
        DisplayType { ty: self, env }
    }
}

#[derive(Copy, Clone, Debug)]
struct DisplayType<'a> {
    ty: &'a Type,
    env: &'a TypeEnvironment,
}

impl std::fmt::Display for DisplayType<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.ty {
            Type::Union(inner) => inner.display(f, self.env),
            Type::Intersection(inner) => inner.display(f, self.env),
            _ => {
                let name = self.ty.name().expect("type should have a name");
                f.write_str(name)
            }
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
    elements: FxHashSet<TypeId>,
}

impl UnionType {
    fn display(&self, f: &mut std::fmt::Formatter<'_>, env: &TypeEnvironment) -> std::fmt::Result {
        f.write_str("(")?;
        let mut first = true;
        for elem_id in self.elements.iter() {
            let ty = env.type_for_id(*elem_id);
            if !first {
                f.write_str(" | ")?;
            };
            first = false;
            write!(f, "{}", ty.display(env))?;
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
    positive: FxHashSet<TypeId>,
    // negated elements of the intersection, e.g.
    negative: FxHashSet<TypeId>,
}

impl IntersectionType {
    fn display(&self, f: &mut std::fmt::Formatter<'_>, env: &TypeEnvironment) -> std::fmt::Result {
        f.write_str("(")?;
        let mut first = true;
        for (neg, elem_id) in self
            .positive
            .iter()
            .map(|elem_id| (false, elem_id))
            .chain(self.negative.iter().map(|elem_id| (true, elem_id)))
        {
            let ty = env.type_for_id(*elem_id);
            if !first {
                f.write_str(" & ")?;
            };
            first = false;
            if neg {
                f.write_str("~")?;
            };
            write!(f, "{}", ty.display(env))?;
        }
        f.write_str(")")
    }
}

#[cfg(test)]
mod tests {
    use crate::types::TypeEnvironment;

    #[test]
    fn add_class() {
        let mut env = TypeEnvironment::default();
        let cid = env.add_class("C");
        assert_eq!(cid.ty(&env).name(), Some("C"));
    }

    #[test]
    fn add_function() {
        let mut env = TypeEnvironment::default();
        let fid = env.add_function("func");
        assert_eq!(fid.ty(&env).name(), Some("func"));
    }
}
