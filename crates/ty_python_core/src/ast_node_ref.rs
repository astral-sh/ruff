use std::fmt::Debug;

#[cfg(debug_assertions)]
use ruff_db::files::File;
use ruff_db::parsed::ParsedModuleRef;
#[cfg(debug_assertions)]
use ruff_python_ast::PythonVersion;
use ruff_python_ast::{NodeIndex, flat};
#[cfg(debug_assertions)]
use ruff_text_size::Ranged;

/// Reference to an AST node.
///
/// This type acts as a reference to an AST node within a given module that remains
/// stable regardless of whether the AST is garbage collected. As such, accessing a
/// node through the [`AstNodeRef`] requires a reference to the current [`ParsedModuleRef`]
/// for the module containing the node.
///
/// ## Usage in salsa tracked structs
/// It's important that [`AstNodeRef`] fields in salsa tracked structs are tracked fields
/// (attributed with `#[tracked`]). It prevents that the tracked struct gets a new ID
/// every time the AST changes, which in turn, invalidates the result of any query
/// that takes said tracked struct as a query argument or returns the tracked struct as part of its result.
///
/// For example, marking the [`AstNodeRef`] as tracked on `Expression`
/// has the effect that salsa will consider the expression as "unchanged" for as long as it:
///
/// * belongs to the same file
/// * belongs to the same scope
/// * has the same kind
/// * was created in the same order
///
/// This means that changes to expressions in other scopes don't invalidate the expression's id, giving
/// us some form of scope-stable identity for expressions. Only queries accessing the node field
/// run on every AST change. All other queries only run when the expression's identity changes.
#[derive(Clone)]
pub struct AstNodeRef<T> {
    handle: flat::Handle<T>,

    /// Debug information.
    #[cfg(debug_assertions)]
    kind: ruff_python_ast::NodeKind,
    #[cfg(debug_assertions)]
    range: ruff_text_size::TextRange,
    // Note that because the module address is not stored in release builds, `AstNodeRef`
    // cannot implement `Eq`, as indices are only unique within a given instance of the
    // AST.
    #[cfg(debug_assertions)]
    file: File,
    #[cfg(debug_assertions)]
    python_version: PythonVersion,
}

impl<T> AstNodeRef<T> {
    #[cfg(debug_assertions)]
    #[inline]
    fn check_module(&self, module_ref: &ParsedModuleRef) {
        assert_eq!(
            (
                module_ref.module().file(),
                module_ref.module().python_version()
            ),
            (self.file, self.python_version),
            "an `AstNodeRef` cannot be used with a module parsed for a different file or Python version"
        );
    }

    /// Resolves the row in the current parse of the same module.
    #[track_caller]
    pub fn node<'ast>(&self, module_ref: &'ast ParsedModuleRef) -> flat::View<'ast, T>
    where
        T: flat::Node,
    {
        #[cfg(debug_assertions)]
        self.check_module(module_ref);
        module_ref
            .ast()
            .view(self.handle)
            .expect("AST indices should never change within the same revision")
    }

    pub fn from_view(module_ref: &ParsedModuleRef, node: flat::View<'_, T>) -> Self {
        debug_assert!(std::ptr::eq(module_ref.ast(), node.ast()));
        Self {
            handle: node.handle(),
            #[cfg(debug_assertions)]
            file: module_ref.module().file(),
            #[cfg(debug_assertions)]
            python_version: module_ref.module().python_version(),
            #[cfg(debug_assertions)]
            kind: node.erase().node_kind(),
            #[cfg(debug_assertions)]
            range: node.range(),
        }
    }

    pub fn index(&self) -> NodeIndex {
        self.handle.module_index()
    }
}

impl<T> get_size2::GetSize for AstNodeRef<T> {}

#[expect(clippy::missing_fields_in_debug)]
impl<T> Debug for AstNodeRef<T>
where
    T: Debug,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        cfg_select! {
            debug_assertions => f
                .debug_struct("AstNodeRef")
                .field("kind", &self.kind)
                .field("range", &self.range)
                .finish(),
            _ => {
                // Unfortunately we have no access to the AST here.
                f.debug_tuple("AstNodeRef").finish_non_exhaustive()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use ruff_db::PythonFile;
    use ruff_db::files::system_path_to_file;
    use ruff_db::parsed::parsed_module;
    use ruff_python_ast::PythonVersion;
    use ruff_text_size::Ranged;

    use crate::ast_node_ref::AstNodeRef;
    use crate::db::tests::TestDbBuilder;

    #[test]
    fn handles_survive_cache_eviction() {
        let db = TestDbBuilder::new()
            .with_file("test.py", "def f(x): return x.child(1)[0]\n")
            .build()
            .unwrap();
        let file = system_path_to_file(&db, "test.py").unwrap();
        let parsed = parsed_module(&db, PythonFile::new(&db, file, PythonVersion::PY312));
        let references: Vec<_> = {
            let first = parsed.load(&db);
            first
                .root()
                .descendants()
                .map(|node| {
                    (
                        AstNodeRef::from_view(&first, node),
                        node.node_kind(),
                        node.range(),
                    )
                })
                .collect()
        };
        parsed.clear();

        let fresh = parsed.load(&db);
        for (reference, kind, range) in references {
            let node = reference.node(&fresh);
            assert_eq!((node.node_kind(), node.range()), (kind, range));
        }
    }

    #[test]
    #[cfg(debug_assertions)]
    #[should_panic(
        expected = "an `AstNodeRef` cannot be used with a module parsed for a different file or Python version"
    )]
    fn rejects_module_parsed_for_different_python_version() {
        let db = TestDbBuilder::new()
            .with_file("test.py", "x = 1")
            .build()
            .unwrap();
        let file = system_path_to_file(&db, "test.py").unwrap();

        let parsed_py311 =
            parsed_module(&db, PythonFile::new(&db, file, PythonVersion::PY311)).load(&db);
        let parsed_py312 =
            parsed_module(&db, PythonFile::new(&db, file, PythonVersion::PY312)).load(&db);
        let assignment = parsed_py311.suite().at(0).as_assign_stmt().unwrap();

        let node = AstNodeRef::from_view(&parsed_py311, assignment);
        node.node(&parsed_py312);
    }
}
