//! Display implementations for types.

use std::fmt::{Display, Formatter};

use crate::types::{IntersectionType, Type, UnionType};
use crate::Db;

impl Type {
    pub fn display<'a, 'db>(&'a self, db: &'db dyn Db) -> DisplayType<'a, 'db> {
        DisplayType { ty: self, db }
    }
}

#[derive(Copy, Clone)]
pub struct DisplayType<'a, 'db> {
    ty: &'a Type,
    db: &'db dyn Db,
}

impl Display for DisplayType<'_, '_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self.ty {
            Type::Any => f.write_str("Any"),
            Type::Never => f.write_str("Never"),
            Type::Unknown => f.write_str("Unknown"),
            Type::Unbound => f.write_str("Unbound"),
            Type::None => f.write_str("None"),
            Type::Module(module_id) => {
                write!(f, "<module '{:?}'>", module_id.file.path(self.db.upcast()))
            }
            // TODO functions and classes should display using a fully qualified name
            Type::Class(class_id) => {
                let class = class_id.lookup(self.db);

                f.write_str("Literal[")?;
                f.write_str(class.name())?;
                f.write_str("]")
            }
            Type::Instance(class_id) => {
                let class = class_id.lookup(self.db);
                f.write_str(class.name())
            }
            Type::Function(function_id) => {
                let function = function_id.lookup(self.db);
                f.write_str(function.name())
            }
            Type::Union(union_id) => {
                let union = union_id.lookup(self.db);

                union.display(self.db).fmt(f)
            }
            Type::Intersection(intersection_id) => {
                let intersection = intersection_id.lookup(self.db);

                intersection.display(self.db).fmt(f)
            }
            Type::IntLiteral(n) => write!(f, "Literal[{n}]"),
        }
    }
}

impl std::fmt::Debug for DisplayType<'_, '_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(self, f)
    }
}

impl UnionType {
    fn display<'a, 'db>(&'a self, db: &'db dyn Db) -> DisplayUnionType<'a, 'db> {
        DisplayUnionType { db, ty: self }
    }
}

struct DisplayUnionType<'a, 'db> {
    ty: &'a UnionType,
    db: &'db dyn Db,
}

impl Display for DisplayUnionType<'_, '_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let union = self.ty;

        let (int_literals, other_types): (Vec<Type>, Vec<Type>) = union
            .elements
            .iter()
            .copied()
            .partition(|ty| matches!(ty, Type::IntLiteral(_)));

        let mut first = true;
        if !int_literals.is_empty() {
            f.write_str("Literal[")?;
            let mut nums: Vec<_> = int_literals
                .into_iter()
                .filter_map(|ty| {
                    if let Type::IntLiteral(n) = ty {
                        Some(n)
                    } else {
                        None
                    }
                })
                .collect();
            nums.sort_unstable();
            for num in nums {
                if !first {
                    f.write_str(", ")?;
                }
                write!(f, "{num}")?;
                first = false;
            }
            f.write_str("]")?;
        }

        for ty in other_types {
            if !first {
                f.write_str(" | ")?;
            };
            first = false;
            write!(f, "{}", ty.display(self.db))?;
        }

        Ok(())
    }
}

impl std::fmt::Debug for DisplayUnionType<'_, '_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(self, f)
    }
}

impl IntersectionType {
    fn display<'a, 'db>(&'a self, db: &'db dyn Db) -> DisplayIntersectionType<'a, 'db> {
        DisplayIntersectionType { db, ty: self }
    }
}

struct DisplayIntersectionType<'a, 'db> {
    ty: &'a IntersectionType,
    db: &'db dyn Db,
}

impl Display for DisplayIntersectionType<'_, '_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let mut first = true;
        for (neg, ty) in self
            .ty
            .positive
            .iter()
            .map(|ty| (false, ty))
            .chain(self.ty.negative.iter().map(|ty| (true, ty)))
        {
            if !first {
                f.write_str(" & ")?;
            };
            first = false;
            if neg {
                f.write_str("~")?;
            };
            write!(f, "{}", ty.display(self.db))?;
        }
        Ok(())
    }
}

impl std::fmt::Debug for DisplayIntersectionType<'_, '_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(self, f)
    }
}
