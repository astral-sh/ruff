#![allow(dead_code)]
use crate::Name;
use ruff_index::{newtype_index, IndexVec};

#[newtype_index]
pub(crate) struct TypeId;

impl TypeId {
    pub(crate) fn typ(self, env: &TypeEnvironment) -> &Type {
        env.type_for_id(self)
    }
}

/// Arena holding all known types
#[derive(Default)]
pub(crate) struct TypeEnvironment {
    types_by_id: IndexVec<TypeId, Type>,
}

impl TypeEnvironment {
    pub(crate) fn add_class(&mut self, name: &str) -> TypeId {
        let class = Type::Class(TypeClass {
            name: Name::new(name),
        });
        self.types_by_id.push(class)
    }

    pub(crate) fn add_function(&mut self, name: &str) -> TypeId {
        let function = Type::Function(TypeFunction {
            name: Name::new(name),
        });
        self.types_by_id.push(function)
    }

    pub(crate) fn type_for_id(&self, type_id: TypeId) -> &Type {
        &self.types_by_id[type_id]
    }
}

pub(crate) enum Type {
    Class(TypeClass),
    Function(TypeFunction),
}

impl Type {
    pub(crate) fn name(&self) -> &str {
        match self {
            Type::Class(inner) => inner.name(),
            Type::Function(inner) => inner.name(),
        }
    }
}

pub(crate) struct TypeClass {
    name: Name,
}

impl TypeClass {
    pub(crate) fn name(&self) -> &str {
        self.name.as_str()
    }
}

pub(crate) struct TypeFunction {
    name: Name,
}

impl TypeFunction {
    pub(crate) fn name(&self) -> &str {
        self.name.as_str()
    }
}

#[cfg(test)]
mod tests {
    use crate::types::TypeEnvironment;

    #[test]
    fn add_class() {
        let mut env = TypeEnvironment::default();
        let cid = env.add_class("C");
        assert_eq!(cid.typ(&env).name(), "C");
    }

    #[test]
    fn add_function() {
        let mut env = TypeEnvironment::default();
        let fid = env.add_function("func");
        assert_eq!(fid.typ(&env).name(), "func");
    }
}
