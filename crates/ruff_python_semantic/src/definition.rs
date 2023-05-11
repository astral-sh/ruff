//! Definitions within a Python program. In this context, a "definition" is any named entity that
//! can be documented, such as a module, class, or function.

use std::fmt::Debug;
use std::iter::FusedIterator;
use std::num::TryFromIntError;
use std::ops::{Deref, Index};

use rustpython_parser::ast::Stmt;

use crate::analyze::visibility::{
    class_visibility, function_visibility, method_visibility, ModuleSource, Visibility,
};

/// Id uniquely identifying a definition in a program.
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub struct DefinitionId(u32);

impl DefinitionId {
    /// Returns the ID for the module definition.
    #[inline]
    pub const fn module() -> Self {
        DefinitionId(0)
    }
}

impl TryFrom<usize> for DefinitionId {
    type Error = TryFromIntError;

    fn try_from(value: usize) -> Result<Self, Self::Error> {
        Ok(Self(u32::try_from(value)?))
    }
}

impl From<DefinitionId> for usize {
    fn from(value: DefinitionId) -> Self {
        value.0 as usize
    }
}

#[derive(Debug)]
pub enum ModuleKind {
    /// A Python file that represents a module within a package.
    Module,
    /// A Python file that represents the root of a package (i.e., an `__init__.py` file).
    Package,
}

/// A Python module.
#[derive(Debug)]
pub struct Module<'a> {
    pub kind: ModuleKind,
    pub source: ModuleSource<'a>,
    pub python_ast: &'a [Stmt],
}

impl<'a> Module<'a> {
    pub fn path(&self) -> Option<&'a [String]> {
        if let ModuleSource::Path(path) = self.source {
            Some(path)
        } else {
            None
        }
    }
}

#[derive(Debug, Copy, Clone)]
pub enum MemberKind {
    /// A class definition within a program.
    Class,
    /// A nested class definition within a program.
    NestedClass,
    /// A function definition within a program.
    Function,
    /// A nested function definition within a program.
    NestedFunction,
    /// A method definition within a program.
    Method,
}

/// A member of a Python module.
#[derive(Debug)]
pub struct Member<'a> {
    pub kind: MemberKind,
    pub parent: DefinitionId,
    pub stmt: &'a Stmt,
}

/// A definition within a Python program.
#[derive(Debug)]
pub enum Definition<'a> {
    Module(Module<'a>),
    Member(Member<'a>),
}

impl Definition<'_> {
    /// Returns `true` if the [`Definition`] is a method definition.
    pub const fn is_method(&self) -> bool {
        matches!(
            self,
            Definition::Member(Member {
                kind: MemberKind::Method,
                ..
            })
        )
    }
}

/// The definitions within a Python program indexed by [`DefinitionId`].
#[derive(Debug, Default)]
pub struct Definitions<'a>(Vec<Definition<'a>>);

impl<'a> Definitions<'a> {
    pub fn for_module(definition: Module<'a>) -> Self {
        Self(vec![Definition::Module(definition)])
    }

    /// Pushes a new member definition and returns its unique id.
    ///
    /// Members are assumed to be pushed in traversal order, such that parents are pushed before
    /// their children.
    pub fn push_member(&mut self, member: Member<'a>) -> DefinitionId {
        let next_id = DefinitionId::try_from(self.0.len()).unwrap();
        self.0.push(Definition::Member(member));
        next_id
    }

    /// Iterate over all definitions in a program, with their visibilities.
    pub fn iter(&self) -> DefinitionsIter {
        DefinitionsIter {
            inner: self.0.iter(),
            definitions: Vec::with_capacity(self.0.len()),
            visibilities: Vec::with_capacity(self.0.len()),
        }
    }
}

impl<'a> Index<DefinitionId> for Definitions<'a> {
    type Output = Definition<'a>;

    fn index(&self, index: DefinitionId) -> &Self::Output {
        &self.0[usize::from(index)]
    }
}

impl<'a> Deref for Definitions<'a> {
    type Target = [Definition<'a>];
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

pub struct DefinitionsIter<'a> {
    inner: std::slice::Iter<'a, Definition<'a>>,
    definitions: Vec<&'a Definition<'a>>,
    visibilities: Vec<Visibility>,
}

impl<'a> Iterator for DefinitionsIter<'a> {
    type Item = (&'a Definition<'a>, Visibility);

    fn next(&mut self) -> Option<Self::Item> {
        let definition = self.inner.next()?;

        // Determine the visibility of the next definition, taking into account its parent's
        // visibility.
        let visibility = {
            match definition {
                Definition::Module(module) => module.source.to_visibility(),
                Definition::Member(member) => match member.kind {
                    MemberKind::Class => {
                        if self.visibilities[usize::from(member.parent)].is_private() {
                            Visibility::Private
                        } else {
                            class_visibility(member.stmt)
                        }
                    }
                    MemberKind::NestedClass => {
                        if self.visibilities[usize::from(member.parent)].is_private()
                            || matches!(
                                self.definitions[usize::from(member.parent)],
                                Definition::Member(Member {
                                    kind: MemberKind::Function
                                        | MemberKind::NestedFunction
                                        | MemberKind::Method,
                                    ..
                                })
                            )
                        {
                            Visibility::Private
                        } else {
                            class_visibility(member.stmt)
                        }
                    }
                    MemberKind::Function => {
                        if self.visibilities[usize::from(member.parent)].is_private() {
                            Visibility::Private
                        } else {
                            function_visibility(member.stmt)
                        }
                    }
                    MemberKind::NestedFunction => Visibility::Private,
                    MemberKind::Method => {
                        if self.visibilities[usize::from(member.parent)].is_private() {
                            Visibility::Private
                        } else {
                            method_visibility(member.stmt)
                        }
                    }
                },
            }
        };
        self.definitions.push(definition);
        self.visibilities.push(visibility);

        Some((definition, visibility))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.inner.size_hint()
    }
}

impl FusedIterator for DefinitionsIter<'_> {}
impl ExactSizeIterator for DefinitionsIter<'_> {}
