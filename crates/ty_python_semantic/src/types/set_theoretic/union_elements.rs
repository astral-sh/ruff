//! Ordered union members with shared prefixes.
//!
//! Small unions keep a single slice. Larger unions use a radix tree whose leaves and branches
//! contain at most sixteen entries. The tree shape depends only on the number of members, so
//! constructing the same ordered union in one batch or through successive updates interns the
//! same value. Updating a prefix copies only the nodes along one path.

use super::{RecursivelyDefined, UnionType};
use crate::Db;
use crate::types::Type;

const WIDTH: usize = 16;

#[derive(Clone, Debug, PartialEq, Eq, Hash, get_size2::GetSize, salsa::SalsaValue)]
pub enum UnionElements<'db> {
    Leaf(Box<[Type<'db>]>),
    Branch {
        len: usize,
        height: u32,
        children: Box<[UnionElementsNode<'db>]>,
    },
}

#[salsa::interned(debug, heap_size=ruff_memory_usage::heap_size)]
pub struct UnionElementsNode<'db> {
    #[returns(ref)]
    elements: UnionElements<'db>,
}

// The Salsa heap is tracked separately.
impl get_size2::GetSize for UnionElementsNode<'_> {}

impl<'db> UnionElements<'db> {
    pub(super) fn new(db: &'db dyn Db, elements: Box<[Type<'db>]>) -> Self {
        if elements.len() <= WIDTH {
            return Self::Leaf(elements);
        }
        let len = elements.len();
        let mut children = elements
            .chunks(WIDTH)
            .map(|chunk| UnionElementsNode::new(db, Self::Leaf(chunk.into())))
            .collect::<Vec<_>>();
        let mut height = 1;
        while children.len() > WIDTH {
            children = children
                .chunks(WIDTH)
                .map(|chunk| {
                    UnionElementsNode::new(
                        db,
                        Self::Branch {
                            len: chunk.iter().map(|node| node.elements(db).len()).sum(),
                            height,
                            children: chunk.into(),
                        },
                    )
                })
                .collect();
            height += 1;
        }
        Self::Branch {
            len,
            height,
            children: children.into_boxed_slice(),
        }
    }

    fn len(&self) -> usize {
        match self {
            Self::Leaf(elements) => elements.len(),
            Self::Branch { len, .. } => *len,
        }
    }

    /// Pad a new rightmost subtree to the same height as its siblings.
    fn singleton(db: &'db dyn Db, ty: Type<'db>, height: u32) -> Self {
        let mut elements = Self::Leaf(Box::new([ty]));
        for height in 1..=height {
            elements = Self::Branch {
                len: 1,
                height,
                children: Box::new([UnionElementsNode::new(db, elements)]),
            };
        }
        elements
    }

    pub(super) fn push(&self, db: &'db dyn Db, ty: Type<'db>) -> Self {
        match self {
            Self::Leaf(elements) if elements.len() < WIDTH => {
                let mut elements = elements.to_vec();
                elements.push(ty);
                Self::Leaf(elements.into_boxed_slice())
            }
            Self::Leaf(_) => Self::Branch {
                len: self.len() + 1,
                height: 1,
                children: Box::new([
                    UnionElementsNode::new(db, self.clone()),
                    UnionElementsNode::new(db, Self::singleton(db, ty, 0)),
                ]),
            },
            Self::Branch {
                len,
                height,
                children,
            } => {
                let child_capacity = WIDTH.pow(*height);
                if *len == child_capacity * WIDTH {
                    return Self::Branch {
                        len: len + 1,
                        height: height + 1,
                        children: Box::new([
                            UnionElementsNode::new(db, self.clone()),
                            UnionElementsNode::new(db, Self::singleton(db, ty, *height)),
                        ]),
                    };
                }
                let mut children = children.to_vec();
                if len.is_multiple_of(child_capacity) {
                    children.push(UnionElementsNode::new(
                        db,
                        Self::singleton(db, ty, height - 1),
                    ));
                } else if let Some(last) = children.last_mut() {
                    *last = UnionElementsNode::new(db, last.elements(db).push(db, ty));
                }
                Self::Branch {
                    len: len + 1,
                    height: *height,
                    children: children.into_boxed_slice(),
                }
            }
        }
    }

    pub(super) fn replace(&self, db: &'db dyn Db, index: usize, ty: Type<'db>) -> Self {
        match self {
            Self::Leaf(elements) => {
                let mut elements = elements.clone();
                elements[index] = ty;
                Self::Leaf(elements)
            }
            Self::Branch {
                len,
                height,
                children,
            } => {
                let child_capacity = WIDTH.pow(*height);
                let mut children = children.clone();
                let child = &mut children[index / child_capacity];
                *child = UnionElementsNode::new(
                    db,
                    child.elements(db).replace(db, index % child_capacity, ty),
                );
                Self::Branch {
                    len: *len,
                    height: *height,
                    children,
                }
            }
        }
    }

    /// Remove the last member without changing an internal node's height.
    fn pop_at_height(&self, db: &'db dyn Db) -> Self {
        match self {
            Self::Leaf(elements) => Self::Leaf(elements[..elements.len() - 1].into()),
            Self::Branch {
                len,
                height,
                children,
            } => {
                let mut children = children.to_vec();
                if let Some(last) = children.last_mut() {
                    if last.elements(db).len() == 1 {
                        children.pop();
                    } else {
                        *last = UnionElementsNode::new(db, last.elements(db).pop_at_height(db));
                    }
                }
                Self::Branch {
                    len: len - 1,
                    height: *height,
                    children: children.into_boxed_slice(),
                }
            }
        }
    }

    pub(super) fn pop(&self, db: &'db dyn Db) -> Self {
        let mut elements = self.pop_at_height(db);
        while let Self::Branch { children, .. } = &elements
            && let [child] = children.as_ref()
        {
            elements = child.elements(db).clone();
        }
        elements
    }

    pub(super) fn into_type(self, db: &'db dyn Db) -> Type<'db> {
        match &self {
            Self::Leaf(elements) if elements.is_empty() => Type::Never,
            Self::Leaf(elements) if elements.len() == 1 => elements[0],
            _ => Type::Union(UnionType::new_internal(db, self, RecursivelyDefined::No)),
        }
    }

    pub(super) fn flatten(&self, db: &'db dyn Db) -> Box<[Type<'db>]> {
        let mut result = Vec::with_capacity(self.len());
        let mut pending = vec![self];
        while let Some(elements) = pending.pop() {
            match elements {
                Self::Leaf(elements) => result.extend_from_slice(elements),
                Self::Branch { children, .. } => {
                    pending.extend(children.iter().rev().map(|child| child.elements(db)));
                }
            }
        }
        result.into_boxed_slice()
    }
}

#[cfg(test)]
mod tests {
    use super::{UnionElements, WIDTH};
    use crate::db::tests::setup_db;
    use crate::types::Type;

    #[test]
    fn updates_preserve_canonical_union_identity() -> anyhow::Result<()> {
        let db = setup_db();
        let mut shared = UnionElements::Leaf(Box::new([]));
        let mut flat = Vec::new();
        let mut prefixes = Vec::new();
        // Cross two branch boundaries, including a new root level.
        for i in 0..=i64::try_from(WIDTH * WIDTH + WIDTH)? {
            let ty = Type::int_literal(i);
            flat.push(ty);
            shared = shared.push(&db, ty);
            prefixes.push(shared.clone().into_type(&db));
            assert_eq!(shared.flatten(&db).as_ref(), flat.as_slice());
            assert_eq!(
                shared.clone().into_type(&db),
                UnionElements::new(&db, flat.clone().into_boxed_slice()).into_type(&db),
            );
        }
        for index in [0, WIDTH - 1, WIDTH, WIDTH * WIDTH, flat.len() - 1] {
            let ty = Type::int_literal(-1);
            flat[index] = ty;
            shared = shared.replace(&db, index, ty);
            assert_eq!(shared.flatten(&db).as_ref(), flat.as_slice());
            assert_eq!(
                shared.clone().into_type(&db),
                UnionElements::new(&db, flat.clone().into_boxed_slice()).into_type(&db),
            );
        }
        while !flat.is_empty() {
            flat.pop();
            shared = shared.pop(&db);
            assert_eq!(shared.flatten(&db).as_ref(), flat.as_slice());
            assert_eq!(
                shared.clone().into_type(&db),
                UnionElements::new(&db, flat.clone().into_boxed_slice()).into_type(&db),
            );
        }
        // Later replacements and removals do not change retained prefix types.
        for (len, prefix) in (1..).zip(prefixes) {
            let expected = (0..len).map(Type::int_literal).collect::<Box<[_]>>();
            assert_eq!(prefix, UnionElements::new(&db, expected).into_type(&db));
        }
        Ok(())
    }
}
