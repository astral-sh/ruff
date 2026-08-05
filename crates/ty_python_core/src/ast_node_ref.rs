use std::fmt::Debug;
use std::marker::PhantomData;

#[cfg(debug_assertions)]
use ruff_db::files::File;
use ruff_db::parsed::ParsedModuleRef;
#[cfg(debug_assertions)]
use ruff_python_ast::PythonVersion;
use ruff_python_ast::{AnyNodeRef, NodeIndex};
use ruff_python_ast::{AnyRootNodeRef, HasNodeIndex};
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
    /// The index of the node in the AST.
    index: NodeIndex,

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

    _node: PhantomData<T>,
}

impl<T> AstNodeRef<T> {
    pub fn index(&self) -> NodeIndex {
        self.index
    }
}

impl<T> AstNodeRef<T>
where
    T: HasNodeIndex + Ranged + PartialEq + Debug,
    for<'ast> AnyNodeRef<'ast>: From<&'ast T>,
    for<'ast> &'ast T: TryFrom<AnyRootNodeRef<'ast>>,
{
    /// Creates a new `AstNodeRef` that references `node`.
    ///
    /// This method may panic or produce unspecified results if the provided module is from a
    /// different file, Python version, or Salsa revision than the module to which the node belongs.
    pub(super) fn new(module_ref: &ParsedModuleRef, node: &T) -> Self {
        let index = node.node_index().load();
        debug_assert_eq!(module_ref.get_by_index(index).try_into().ok(), Some(node));

        Self {
            index,
            #[cfg(debug_assertions)]
            file: module_ref.module().file(),
            #[cfg(debug_assertions)]
            python_version: module_ref.module().python_version(),
            #[cfg(debug_assertions)]
            kind: AnyNodeRef::from(node).kind(),
            #[cfg(debug_assertions)]
            range: node.range(),
            _node: PhantomData,
        }
    }

    /// Returns a reference to the wrapped node.
    ///
    /// This method may panic or produce unspecified results if the provided module is from a
    /// different file, Python version, or Salsa revision than the module to which the node belongs.
    #[track_caller]
    pub fn node<'ast>(&self, module_ref: &'ast ParsedModuleRef) -> &'ast T {
        #[cfg(debug_assertions)]
        assert_eq!(
            (
                module_ref.module().file(),
                module_ref.module().python_version()
            ),
            (self.file, self.python_version),
            "an `AstNodeRef` cannot be used with a module parsed for a different file or Python version"
        );
        // The user guarantees that the module is from the same file, Python version, and Salsa
        // revision, so the file contents cannot have changed.
        module_ref
            .get_by_index(self.index)
            .try_into()
            .ok()
            .expect("AST indices should never change within the same revision")
    }
}

impl<T> get_size2::GetSize for AstNodeRef<T> {}

#[expect(clippy::missing_fields_in_debug)]
impl<T> Debug for AstNodeRef<T>
where
    T: Debug,
    for<'ast> &'ast T: TryFrom<AnyRootNodeRef<'ast>>,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        cfg_select! {
            debug_assertions => {
                f.debug_struct("AstNodeRef")
                    .field("kind", &self.kind)
                    .field("range", &self.range)
                    .finish()
            },
            _ => {
                // Unfortunately we have no access to the AST here.
                f.debug_tuple("AstNodeRef").finish_non_exhaustive()
            },
        }
    }
}

#[cfg(all(test, debug_assertions))]
mod tests {
    use ruff_db::PythonFile;
    use ruff_db::files::system_path_to_file;
    use ruff_db::parsed::parsed_module;
    use ruff_python_ast::PythonVersion;

    use crate::ast_node_ref::AstNodeRef;
    use crate::db::tests::TestDbBuilder;

    #[test]
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
        let assignment = parsed_py311.syntax().body[0].as_assign_stmt().unwrap();

        let node = AstNodeRef::new(&parsed_py311, assignment);
        node.node(&parsed_py312);
    }
}
