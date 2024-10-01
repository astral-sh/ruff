//! Definitions within a Python program. In this context, a "definition" is any named entity that
//! can be documented, such as a module, class, or function.

use std::fmt::Debug;
use std::ops::Deref;
use std::path::Path;

use ruff_index::{newtype_index, IndexSlice, IndexVec};
use ruff_python_ast::name::QualifiedName;
use ruff_python_ast::{self as ast, Stmt, StmtFunctionDef};
use ruff_text_size::{Ranged, TextRange};

use crate::analyze::visibility::{
    class_visibility, function_visibility, is_property, method_visibility, module_visibility,
    Visibility,
};
use crate::model::all::DunderAllName;
use crate::SemanticModel;

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

/// A Python module can either be defined as a module path (i.e., the dot-separated path to the
/// module) or, if the module can't be resolved, as a file path (i.e., the path to the file defining
/// the module).
#[derive(Debug, Copy, Clone)]
pub enum ModuleSource<'a> {
    /// A module path is a dot-separated path to the module.
    Path(&'a [String]),
    /// A file path is the path to the file defining the module, often a script outside of a
    /// package.
    File(&'a Path),
}

#[derive(Debug, Copy, Clone, is_macro::Is)]
pub enum ModuleKind {
    /// A Python file that represents a module within a package.
    Module,
    /// A Python file that represents the root of a package (i.e., an `__init__.py` file).
    Package,
}

/// A Python module.
#[derive(Debug, Copy, Clone)]
pub struct Module<'a> {
    pub kind: ModuleKind,
    pub source: ModuleSource<'a>,
    pub python_ast: &'a [Stmt],
    pub name: Option<&'a str>,
}

impl<'a> Module<'a> {
    /// Return the fully-qualified path of the module.
    pub const fn qualified_name(&self) -> Option<&'a [String]> {
        if let ModuleSource::Path(path) = self.source {
            Some(path)
        } else {
            None
        }
    }

    /// Return the name of the module.
    pub const fn name(&self) -> Option<&'a str> {
        self.name
    }
}

#[derive(Debug, Copy, Clone, is_macro::Is)]
pub enum MemberKind<'a> {
    /// A class definition within a program.
    Class(&'a ast::StmtClassDef),
    /// A nested class definition within a program.
    NestedClass(&'a ast::StmtClassDef),
    /// A function definition within a program.
    Function(&'a ast::StmtFunctionDef),
    /// A nested function definition within a program.
    NestedFunction(&'a ast::StmtFunctionDef),
    /// A method definition within a program.
    Method(&'a ast::StmtFunctionDef),
}

/// A member of a Python module.
#[derive(Debug)]
pub struct Member<'a> {
    pub kind: MemberKind<'a>,
    pub parent: DefinitionId,
}

impl<'a> Member<'a> {
    /// Return the name of the member.
    pub fn name(&self) -> &'a str {
        match self.kind {
            MemberKind::Class(class) => &class.name,
            MemberKind::NestedClass(class) => &class.name,
            MemberKind::Function(function) => &function.name,
            MemberKind::NestedFunction(function) => &function.name,
            MemberKind::Method(method) => &method.name,
        }
    }

    /// Return the body of the member.
    pub fn body(&self) -> &'a [Stmt] {
        match self.kind {
            MemberKind::Class(class) => &class.body,
            MemberKind::NestedClass(class) => &class.body,
            MemberKind::Function(function) => &function.body,
            MemberKind::NestedFunction(function) => &function.body,
            MemberKind::Method(method) => &method.body,
        }
    }
}

impl Ranged for Member<'_> {
    /// Return the range of the member.
    fn range(&self) -> TextRange {
        match self.kind {
            MemberKind::Class(class) => class.range(),
            MemberKind::NestedClass(class) => class.range(),
            MemberKind::Function(function) => function.range(),
            MemberKind::NestedFunction(function) => function.range(),
            MemberKind::Method(method) => method.range(),
        }
    }
}

/// A definition within a Python program.
#[derive(Debug, is_macro::Is)]
pub enum Definition<'a> {
    Module(Module<'a>),
    Member(Member<'a>),
}

impl<'a> Definition<'a> {
    /// Returns `true` if the [`Definition`] is a method definition.
    pub const fn is_method(&self) -> bool {
        matches!(
            self,
            Definition::Member(Member {
                kind: MemberKind::Method(_),
                ..
            })
        )
    }

    pub fn is_property<P, I>(&self, extra_properties: P, semantic: &SemanticModel) -> bool
    where
        P: IntoIterator<IntoIter = I>,
        I: Iterator<Item = QualifiedName<'a>> + Clone,
    {
        self.as_function_def()
            .is_some_and(|StmtFunctionDef { decorator_list, .. }| {
                is_property(decorator_list, extra_properties, semantic)
            })
    }

    /// Return the name of the definition.
    pub fn name(&self) -> Option<&'a str> {
        match self {
            Definition::Module(module) => module.name(),
            Definition::Member(member) => Some(member.name()),
        }
    }

    /// Return the [`ast::StmtFunctionDef`] of the definition, if it's a function definition.
    pub fn as_function_def(&self) -> Option<&'a ast::StmtFunctionDef> {
        match self {
            Definition::Member(Member {
                kind:
                    MemberKind::Function(function)
                    | MemberKind::NestedFunction(function)
                    | MemberKind::Method(function),
                ..
            }) => Some(function),
            _ => None,
        }
    }

    /// Return the [`ast::StmtClassDef`] of the definition, if it's a class definition.
    pub fn as_class_def(&self) -> Option<&'a ast::StmtClassDef> {
        match self {
            Definition::Member(Member {
                kind: MemberKind::Class(class) | MemberKind::NestedClass(class),
                ..
            }) => Some(class),
            _ => None,
        }
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
    pub fn resolve(self, exports: Option<&[DunderAllName]>) -> ContextualizedDefinitions<'a> {
        let mut definitions: IndexVec<DefinitionId, ContextualizedDefinition<'a>> =
            IndexVec::with_capacity(self.len());

        for definition in self {
            // Determine the visibility of the next definition, taking into account its parent's
            // visibility.
            let visibility = {
                match &definition {
                    Definition::Module(module) => module_visibility(module),
                    Definition::Member(member) => match member.kind {
                        MemberKind::Class(class) => {
                            let parent = &definitions[member.parent];
                            if parent.visibility.is_private()
                                || exports.is_some_and(|exports| {
                                    !exports.iter().any(|export| export.name() == member.name())
                                })
                            {
                                Visibility::Private
                            } else {
                                class_visibility(class)
                            }
                        }
                        MemberKind::NestedClass(class) => {
                            let parent = &definitions[member.parent];
                            if parent.visibility.is_private()
                                || parent.definition.as_function_def().is_some()
                            {
                                Visibility::Private
                            } else {
                                class_visibility(class)
                            }
                        }
                        MemberKind::Function(function) => {
                            let parent = &definitions[member.parent];
                            if parent.visibility.is_private()
                                || exports.is_some_and(|exports| {
                                    !exports.iter().any(|export| export.name() == member.name())
                                })
                            {
                                Visibility::Private
                            } else {
                                function_visibility(function)
                            }
                        }
                        MemberKind::NestedFunction(_) => Visibility::Private,
                        MemberKind::Method(function) => {
                            let parent = &definitions[member.parent];
                            if parent.visibility.is_private() {
                                Visibility::Private
                            } else {
                                method_visibility(function)
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

    /// Returns a reference to the Python AST.
    pub fn python_ast(&self) -> Option<&'a [Stmt]> {
        let module = self[DefinitionId::module()].as_module()?;
        Some(module.python_ast)
    }
}

impl<'a> Deref for Definitions<'a> {
    type Target = IndexSlice<DefinitionId, Definition<'a>>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<'a> IntoIterator for Definitions<'a> {
    type Item = Definition<'a>;
    type IntoIter = std::vec::IntoIter<Self::Item>;

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
