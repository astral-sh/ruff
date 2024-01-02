use rustc_hash::FxHashSet;

use ruff_python_ast as ast;
use ruff_python_ast::call_path::CallPath;
use ruff_python_ast::helpers::map_subscript;

use crate::{BindingId, SemanticModel};

/// Return `true` if any base class matches a [`CallPath`] predicate.
pub fn any_call_path(
    class_def: &ast::StmtClassDef,
    semantic: &SemanticModel,
    func: &dyn Fn(CallPath) -> bool,
) -> bool {
    fn inner(
        class_def: &ast::StmtClassDef,
        semantic: &SemanticModel,
        func: &dyn Fn(CallPath) -> bool,
        seen: &mut FxHashSet<BindingId>,
    ) -> bool {
        class_def.bases().iter().any(|expr| {
            // If the base class itself matches the pattern, then this does too.
            // Ex) `class Foo(BaseModel): ...`
            if semantic
                .resolve_call_path(map_subscript(expr))
                .is_some_and(func)
            {
                return true;
            }

            // If the base class extends a class that matches the pattern, then this does too.
            // Ex) `class Bar(BaseModel): ...; class Foo(Bar): ...`
            if let Some(id) = semantic.lookup_attribute(map_subscript(expr)) {
                if seen.insert(id) {
                    let binding = semantic.binding(id);
                    if let Some(base_class) = binding
                        .kind
                        .as_class_definition()
                        .map(|id| &semantic.scopes[*id])
                        .and_then(|scope| scope.kind.as_class())
                    {
                        if inner(base_class, semantic, func, seen) {
                            return true;
                        }
                    }
                }
            }
            false
        })
    }

    if class_def.bases().is_empty() {
        return false;
    }

    inner(class_def, semantic, func, &mut FxHashSet::default())
}

/// Return `true` if any base class matches an [`ast::StmtClassDef`] predicate.
pub fn any_super_class(
    class_def: &ast::StmtClassDef,
    semantic: &SemanticModel,
    func: &dyn Fn(&ast::StmtClassDef) -> bool,
) -> bool {
    fn inner(
        class_def: &ast::StmtClassDef,
        semantic: &SemanticModel,
        func: &dyn Fn(&ast::StmtClassDef) -> bool,
        seen: &mut FxHashSet<BindingId>,
    ) -> bool {
        // If the function itself matches the pattern, then this does too.
        if func(class_def) {
            return true;
        }

        // Otherwise, check every base class.
        class_def.bases().iter().any(|expr| {
            // If the base class extends a class that matches the pattern, then this does too.
            if let Some(id) = semantic.lookup_attribute(map_subscript(expr)) {
                if seen.insert(id) {
                    let binding = semantic.binding(id);
                    if let Some(base_class) = binding
                        .kind
                        .as_class_definition()
                        .map(|id| &semantic.scopes[*id])
                        .and_then(|scope| scope.kind.as_class())
                    {
                        if inner(base_class, semantic, func, seen) {
                            return true;
                        }
                    }
                }
            }
            false
        })
    }

    inner(class_def, semantic, func, &mut FxHashSet::default())
}
