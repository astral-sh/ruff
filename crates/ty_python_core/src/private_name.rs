//! Lexical class-private identifier spelling.

use ruff_python_ast::{self as ast, name::Name};
use std::borrow::Cow;

/// Lexical mangling context. A generic class header leaves ordinary private identifiers raw,
/// but uses the new class owner for spellings declared as private type parameters, including
/// matching attribute names.
///
/// ```python
/// class C[__T](ns.__T, __Base):  # ns._C__T, but raw __Base
///     pass
/// ```
#[derive(Clone, Debug, Default, Eq, PartialEq, Hash, get_size2::GetSize)]
pub(crate) struct PrivateNamePolicy {
    owner: Option<Name>,
    selective_owner: Option<Name>,
    selective_identifiers: Box<[Name]>,
}

impl PrivateNamePolicy {
    pub(crate) fn for_class(name: Name) -> Self {
        Self {
            owner: Some(name),
            ..Self::default()
        }
    }

    pub(crate) fn for_class_type_parameters(class: &ast::StmtClassDef) -> Self {
        let selective_identifiers =
            class
                .type_params
                .as_ref()
                .map_or_else(Box::default, |parameters| {
                    parameters
                        .type_params
                        .iter()
                        .map(|parameter| match parameter {
                            ast::TypeParam::TypeVar(parameter) => parameter.name.id.clone(),
                            ast::TypeParam::ParamSpec(parameter) => parameter.name.id.clone(),
                            ast::TypeParam::TypeVarTuple(parameter) => parameter.name.id.clone(),
                        })
                        .collect()
                });
        Self {
            selective_owner: Some(class.name.id.clone()),
            selective_identifiers,
            ..Self::default()
        }
    }

    pub(crate) fn canonical_identifier<'a>(&self, identifier: &'a str) -> Cow<'a, str> {
        let owner = if self.uses_selective_owner(identifier) {
            self.selective_owner.as_deref()
        } else {
            self.owner.as_deref()
        };
        canonical_identifier(owner, identifier)
    }

    pub(crate) fn uses_selective_owner(&self, identifier: &str) -> bool {
        self.selective_identifiers
            .iter()
            .any(|name| name == identifier)
    }

    pub(crate) fn owner(&self) -> Option<&str> {
        self.selective_owner.as_deref().or(self.owner.as_deref())
    }

    pub(crate) fn lexical_private_spelling<'a>(&self, runtime_key: &'a str) -> Option<&'a str> {
        [self.selective_owner.as_deref(), self.owner.as_deref()]
            .into_iter()
            .flatten()
            .find_map(|owner| {
                if !is_private_runtime_key(owner, runtime_key) {
                    return None;
                }
                let spelling = runtime_key
                    .strip_prefix('_')?
                    .strip_prefix(owner.trim_start_matches('_'))?;
                (self.canonical_identifier(spelling) == runtime_key).then_some(spelling)
            })
    }
}

/// Return the runtime spelling of an identifier in its lexical class context.
/// The parser supplies normalized Unicode identifiers. Mangling preserves their characters,
/// strips leading underscores from the class name, and leaves dunders, dotted module paths,
/// explicit runtime spellings, and all-underscore class owners unchanged.
///
/// ```python
/// class C:
///     __x = 1     # runtime key: _C__x
///     _C__x = 2   # same key
/// ```
pub fn canonical_identifier<'a>(class_name: Option<&str>, identifier: &'a str) -> Cow<'a, str> {
    if !identifier.starts_with("__") || identifier.ends_with("__") || identifier.contains('.') {
        return Cow::Borrowed(identifier);
    }
    let Some(owner) = class_name.map(|name| name.trim_start_matches('_')) else {
        return Cow::Borrowed(identifier);
    };
    if owner.is_empty() {
        Cow::Borrowed(identifier)
    } else {
        Cow::Owned(format!("_{owner}{identifier}"))
    }
}

/// Whether a runtime key can be produced by a private identifier in this class.
pub fn is_private_runtime_key(class_name: &str, runtime_key: &str) -> bool {
    let owner = class_name.trim_start_matches('_');
    !owner.is_empty()
        && runtime_key
            .strip_prefix('_')
            .and_then(|key| key.strip_prefix(owner))
            .is_some_and(|suffix| suffix.starts_with("__") && !suffix.ends_with("__"))
}
