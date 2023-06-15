//! Definitions within a Python program. In this context, a "definition" is any named entity that
//! can be documented, such as a module, class, or function.

use std::fmt::Debug;
use std::ops::Deref;

use ruff_index::{newtype_index, IndexSlice, IndexVec};
use rustpython_parser::ast::{self, Stmt};

use crate::analyze::visibility::{
    class_visibility, function_visibility, method_visibility, ModuleSource, Visibility,
};

/// Id uniquely identifying a definition in a program.
#[newtype_index]
pub struct DefinitionId;

impl DefinitionId {
    /// Returns the ID for the module definition.
    #[inline]
    pub const fn module() -> Self {
        DefinitionId::from_u32(0)
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

impl<'a> Member<'a> {
    fn name(&self) -> &'a str {
        match &self.stmt {
            Stmt::FunctionDef(ast::StmtFunctionDef { name, .. }) => name,
            Stmt::AsyncFunctionDef(ast::StmtAsyncFunctionDef { name, .. }) => name,
            Stmt::ClassDef(ast::StmtClassDef { name, .. }) => name,
            _ => unreachable!("Unexpected member kind: {:?}", self.kind),
        }
    }
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
pub struct Definitions<'a>(IndexVec<DefinitionId, Definition<'a>>);

impl<'a> Definitions<'a> {
    pub fn for_module(definition: Module<'a>) -> Self {
        Self(IndexVec::from_raw(vec![Definition::Module(definition)]))
    }

    /// Pushes a new member definition and returns its unique id.
    ///
    /// Members are assumed to be pushed in traversal order, such that parents are pushed before
    /// their children.
    pub(crate) fn push_member(&mut self, member: Member<'a>) -> DefinitionId {
        self.0.push(Definition::Member(member))
    }

    /// Resolve the visibility of each definition in the collection.
    pub fn resolve(self, exports: Option<&[&str]>) -> ContextualizedDefinitions<'a> {
        let mut definitions: IndexVec<DefinitionId, ContextualizedDefinition<'a>> =
            IndexVec::with_capacity(self.len());

        for definition in self {
            // Determine the visibility of the next definition, taking into account its parent's
            // visibility.
            let visibility = {
                match &definition {
                    Definition::Module(module) => module.source.to_visibility(),
                    Definition::Member(member) => match member.kind {
                        MemberKind::Class => {
                            let parent = &definitions[member.parent];
                            if parent.visibility.is_private()
                                || exports
                                    .map_or(false, |exports| !exports.contains(&member.name()))
                            {
                                Visibility::Private
                            } else {
                                class_visibility(member.stmt)
                            }
                        }
                        MemberKind::NestedClass => {
                            let parent = &definitions[member.parent];
                            if parent.visibility.is_private()
                                || matches!(
                                    parent.definition,
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
                            let parent = &definitions[member.parent];
                            if parent.visibility.is_private()
                                || exports
                                    .map_or(false, |exports| !exports.contains(&member.name()))
                            {
                                Visibility::Private
                            } else {
                                function_visibility(member.stmt)
                            }
                        }
                        MemberKind::NestedFunction => Visibility::Private,
                        MemberKind::Method => {
                            let parent = &definitions[member.parent];
                            if parent.visibility.is_private() {
                                Visibility::Private
                            } else {
                                method_visibility(member.stmt)
                            }
                        }
                    },
                }
            };
            definitions.push(ContextualizedDefinition {
                definition,
                visibility,
            });
        }

        ContextualizedDefinitions(definitions.raw)
    }
}

impl<'a> Deref for Definitions<'a> {
    type Target = IndexSlice<DefinitionId, Definition<'a>>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<'a> IntoIterator for Definitions<'a> {
    type IntoIter = std::vec::IntoIter<Self::Item>;
    type Item = Definition<'a>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

/// A [`Definition`] in a Python program with its resolved [`Visibility`].
pub struct ContextualizedDefinition<'a> {
    pub definition: Definition<'a>,
    pub visibility: Visibility,
}

/// A collection of [`Definition`] structs in a Python program with resolved [`Visibility`].
pub struct ContextualizedDefinitions<'a>(Vec<ContextualizedDefinition<'a>>);

impl<'a> ContextualizedDefinitions<'a> {
    pub fn iter(&self) -> impl Iterator<Item = &ContextualizedDefinition<'a>> {
        self.0.iter()
    }
}
