use std::fmt::Formatter;
use std::sync::Arc;

use arc_swap::ArcSwapOption;
use get_size2::GetSize;
use ruff_allocator::Allocator;
use ruff_python_ast::{
    AnyRootNodeRef, HasNodeIndex, ModExpression, ModModule, NodeIndex, NodeIndexError,
    StringLiteral, Suite, token::Tokens,
};
use ruff_python_parser::{
    ParseError, ParseErrorType, ParseOptions, Parsed, UnsupportedSyntaxError,
    parse_string_annotation,
};
use yoke::Yoke;

use crate::Db;
use crate::files::File;
use crate::source::source_text;

/// Returns the parsed AST of `file`, including its token stream.
///
/// The query uses Ruff's error-resilient parser. That means that the parser always succeeds to produce an
/// AST even if the file contains syntax errors. The parse errors
/// are then accessible through [`Parsed::errors`].
///
/// The query is only cached when the [`source_text()`] hasn't changed. This is because
/// comparing two ASTs is a non-trivial operation and every offset change is directly
/// reflected in the changed AST offsets.
/// The other reason is that Ruff's AST doesn't implement `Eq` which Salsa requires
/// for determining if a query result is unchanged.
///
/// The LRU capacity of 200 was picked without any empirical evidence that it's optimal,
/// instead it's a wild guess that it should be unlikely that incremental changes involve
/// more than 200 modules. Parsed ASTs within the same revision are never evicted by Salsa.
#[salsa::tracked(returns(ref), no_eq, heap_size=ruff_memory_usage::heap_size, lru=200)]
pub fn parsed_module(db: &dyn Db, file: File) -> ParsedModule {
    let _span = tracing::trace_span!("parsed_module", ?file).entered();

    ParsedModule::new(file, indexed_module(db, file))
}

fn indexed_module(db: &dyn Db, file: File) -> Arc<indexed::IndexedModule> {
    let source = source_text(db, file);
    let ty = file.source_type(db);

    let target_version = db.python_version();
    let options = ParseOptions::from(ty).with_target_version(target_version);
    indexed::IndexedModule::new(&source, options)
}

#[derive(yoke::Yokeable)]
struct ParsedExpressionData<'ast> {
    parsed: Parsed<ModExpression<'ast>>,
}

/// An owned parsed string-annotation expression.
pub struct ParsedExpression {
    inner: Yoke<ParsedExpressionData<'static>, Box<Allocator>>,
}

impl ParsedExpression {
    pub fn syntax(&self) -> &ModExpression<'_> {
        self.inner.get().parsed.syntax()
    }

    pub fn tokens(&self) -> &Tokens {
        self.inner.get().parsed.tokens()
    }

    pub fn expr(&self) -> &ruff_python_ast::Expr<'_> {
        self.inner.get().parsed.expr()
    }
}

pub fn parsed_string_annotation(
    source: &str,
    string: &StringLiteral,
) -> Result<ParsedExpression, ParseError> {
    let inner = Yoke::<ParsedExpressionData<'static>, Box<Allocator>>::try_attach_to_cart(
        Box::new(Allocator::new()),
        |allocator| {
            let expr = parse_string_annotation(source, string, allocator)?;

            // We need the sub-ast of the string annotation to be indexed.
            indexed::ensure_indexed(&expr, string.node_index().load()).map_err(|err| {
                let message = match err {
                    NodeIndexError::NoParent => {
                        "internal error: string annotation's parent had no NodeIndex".to_owned()
                    }
                    NodeIndexError::TooNested => "too many levels of nested string annotations; remove the redundant nested quotes".to_owned(),
                    NodeIndexError::OverflowedIndices => {
                        "file too long for string annotations; either break up the file or don't use string annotations".to_owned()
                    }
                    NodeIndexError::OverflowedSubIndices => {
                        "file too long for nested string annotations; remove the redundant nested quotes".to_owned()
                    }
                    NodeIndexError::ExhaustedSubIndices => {
                        "string annotation is too long; consider introducing type aliases to simplify".to_owned()
                    }
                    NodeIndexError::ExhaustedSubSubIndices => {
                        "nested string annotation is too long; remove the redundant nested quotes".to_owned()
                    }
                };

                ParseError {
                    error: ParseErrorType::OtherError(message),
                    location: string.range,
                }
            })?;

            Ok::<_, ParseError>(ParsedExpressionData { parsed: expr })
        },
    )?;

    Ok(ParsedExpression { inner })
}

/// A wrapper around a parsed module.
///
/// This type manages instances of the module AST. A particular instance of the AST
/// is represented with the [`ParsedModuleRef`] type.
#[derive(Clone, get_size2::GetSize)]
pub struct ParsedModule {
    file: File,
    #[get_size(size_fn = arc_swap_size)]
    inner: Arc<ArcSwapOption<indexed::IndexedModule>>,
}

impl ParsedModule {
    fn new(file: File, parsed: Arc<indexed::IndexedModule>) -> Self {
        Self {
            file,
            inner: Arc::new(ArcSwapOption::new(Some(parsed))),
        }
    }
    /// Loads a reference to the parsed module.
    ///
    /// Note that holding on to the reference will prevent garbage collection
    /// of the AST. This method will reparse the module if it has been collected.
    pub fn load(&self, db: &dyn Db) -> ParsedModuleRef {
        let parsed = match self.inner.load_full() {
            Some(parsed) => parsed,
            None => {
                // Re-parse the file.
                let parsed = indexed_module(db, self.file);
                tracing::debug!(
                    "File `{}` was reparsed after being collected in the current Salsa revision",
                    self.file.path(db)
                );

                self.inner.store(Some(parsed.clone()));
                parsed
            }
        };

        ParsedModuleRef {
            module: self.clone(),
            indexed: parsed,
        }
    }

    /// Clear the parsed module, dropping the AST once all references to it are dropped.
    pub fn clear(&self) {
        self.inner.store(None);
    }

    /// Returns the file to which this module belongs.
    pub fn file(&self) -> File {
        self.file
    }
}

impl std::fmt::Debug for ParsedModule {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("ParsedModule").field(&self.inner).finish()
    }
}

impl PartialEq for ParsedModule {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.inner, &other.inner)
    }
}

impl Eq for ParsedModule {}

/// Cheap cloneable wrapper around an instance of a module AST.
#[derive(Clone)]
pub struct ParsedModuleRef {
    module: ParsedModule,
    indexed: Arc<indexed::IndexedModule>,
}

impl ParsedModuleRef {
    /// Returns a reference to the [`ParsedModule`] that this instance was loaded from.
    pub fn module(&self) -> &ParsedModule {
        &self.module
    }

    /// Returns a reference to the AST node at the given index.
    pub fn get_by_index<'ast>(&'ast self, index: NodeIndex) -> AnyRootNodeRef<'ast> {
        self.indexed.get_by_index(index)
    }

    pub fn syntax(&self) -> &ModModule<'_> {
        self.indexed.parsed().syntax()
    }

    pub fn suite(&self) -> &Suite<'_> {
        self.indexed.parsed().suite()
    }

    pub fn tokens(&self) -> &Tokens {
        self.indexed.parsed().tokens()
    }

    pub fn errors(&self) -> &[ParseError] {
        self.indexed.parsed().errors()
    }

    pub fn unsupported_syntax_errors(&self) -> &[UnsupportedSyntaxError] {
        self.indexed.parsed().unsupported_syntax_errors()
    }

    pub fn has_valid_syntax(&self) -> bool {
        self.indexed.parsed().has_valid_syntax()
    }

    pub fn has_invalid_syntax(&self) -> bool {
        self.indexed.parsed().has_invalid_syntax()
    }

    pub fn has_no_syntax_errors(&self) -> bool {
        self.indexed.parsed().has_no_syntax_errors()
    }

    pub fn has_syntax_errors(&self) -> bool {
        self.indexed.parsed().has_syntax_errors()
    }
}

/// Returns the heap-size of the currently stored `T` in the `ArcSwap`.
fn arc_swap_size<T>(arc_swap: &Arc<ArcSwapOption<T>>) -> usize
where
    T: GetSize,
{
    if let Some(value) = &*arc_swap.load() {
        T::get_heap_size(value)
    } else {
        0
    }
}

mod indexed {
    use std::sync::Arc;

    use get_size2::{GetSize, GetSizeTracker};
    use ruff_allocator::Allocator;
    use ruff_python_ast::visitor::source_order::*;
    use ruff_python_ast::*;
    use ruff_python_parser::{ParseOptions, Parsed, parse_unchecked};
    use yoke::Yoke;

    #[derive(yoke::Yokeable)]
    struct OwnedModule<'ast> {
        parsed: Parsed<ModModule<'ast>>,
    }

    /// Owns the arena backing a fully constructed parsed module.
    struct FrozenAllocator(Allocator);

    // SAFETY: `FrozenAllocator` is private and is only borrowed mutably-through-shared-reference
    // inside `IndexedModule::new`, before the resulting module can be shared. After construction,
    // the cart is retained solely to keep immutable AST references alive and is never exposed or
    // used for further allocations.
    unsafe impl Sync for FrozenAllocator {}

    type ParsedModule = Yoke<OwnedModule<'static>, Box<FrozenAllocator>>;

    #[derive(yoke::Yokeable)]
    struct Index<'ast> {
        nodes: Box<[AnyRootNodeRef<'ast>]>,
    }

    /// A wrapper around an owned arena-allocated AST that allows access to AST nodes by index.
    pub struct IndexedModule {
        index: Yoke<Index<'static>, Arc<ParsedModule>>,
    }

    /// Ensure the following sub-AST is indexed, using the parent node's index
    /// as a basis for unambiguous AST node indices.
    pub fn ensure_indexed<'ast>(
        parsed: &Parsed<ModExpression<'ast>>,
        parent_node_index: NodeIndex,
    ) -> Result<(), NodeIndexError> {
        let parent_index = parent_node_index.as_u32().ok_or(NodeIndexError::NoParent)?;
        let (index, max_index) = sub_indices(parent_index)?;
        let mut visitor = Visitor {
            overflowed: false,
            nodes: None,
            index,
            max_index,
        };

        AnyNodeRef::from(parsed.syntax()).visit_source_order(&mut visitor);

        if visitor.overflowed {
            let level = sub_ast_level(parent_index);
            if level == 0 {
                return Err(NodeIndexError::ExhaustedSubIndices);
            } else {
                return Err(NodeIndexError::ExhaustedSubSubIndices);
            }
        }

        Ok(())
    }

    impl IndexedModule {
        /// Create a new [`IndexedModule`] by parsing into its owned arena.
        pub fn new(source: &str, options: ParseOptions) -> Arc<Self> {
            let parsed = Arc::new(
                Yoke::<OwnedModule<'static>, Box<FrozenAllocator>>::attach_to_cart(
                    Box::new(FrozenAllocator(Allocator::new())),
                    |allocator| OwnedModule {
                        parsed: parse_unchecked(source, options, &allocator.0)
                            .try_into_module()
                            .expect("PySourceType always parses into a module"),
                    },
                ),
            );

            let index =
                Yoke::<Index<'static>, Arc<ParsedModule>>::attach_to_cart(parsed, |parsed| {
                    let mut visitor = Visitor {
                        nodes: Some(Vec::new()),
                        index: 0,
                        max_index: MAX_REAL_INDEX,
                        overflowed: false,
                    };

                    AnyNodeRef::from(parsed.get().parsed.syntax()).visit_source_order(&mut visitor);

                    Index {
                        nodes: visitor.nodes.unwrap().into_boxed_slice(),
                    }
                });

            Arc::new(Self { index })
        }

        pub fn parsed(&self) -> &Parsed<ModModule<'_>> {
            &self.index.backing_cart().get().parsed
        }

        /// Returns the node at the given index.
        pub fn get_by_index<'ast>(&'ast self, index: NodeIndex) -> AnyRootNodeRef<'ast> {
            let index = index
                .as_u32()
                .expect("attempted to access uninitialized `NodeIndex`");

            self.index.get().nodes[index as usize]
        }
    }

    impl std::fmt::Debug for IndexedModule {
        fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter
                .debug_struct("IndexedModule")
                .field("parsed", self.parsed())
                .field("index", &self.index.get().nodes)
                .finish()
        }
    }

    impl GetSize for IndexedModule {
        fn get_heap_size_with_tracker<Tracker: GetSizeTracker>(
            &self,
            tracker: Tracker,
        ) -> (usize, Tracker) {
            let (parsed_size, tracker) = self.parsed().get_heap_size_with_tracker(tracker);
            let (index_size, tracker) = self.index.get().nodes.get_heap_size_with_tracker(tracker);
            (parsed_size + index_size, tracker)
        }
    }

    /// A visitor that collects nodes in source order.
    pub struct Visitor<'a> {
        pub index: u32,
        pub max_index: u32,
        pub nodes: Option<Vec<AnyRootNodeRef<'a>>>,
        pub overflowed: bool,
    }

    impl<'a> Visitor<'a> {
        fn visit_node<T>(&mut self, node: &'a T)
        where
            T: HasNodeIndex + std::fmt::Debug,
            AnyRootNodeRef<'a>: From<&'a T>,
        {
            // Only check on write (the maximum is orders of magnitude less than u32::MAX)
            if self.index > self.max_index {
                self.overflowed = true;
            } else {
                node.node_index().set(NodeIndex::from(self.index));
            }

            if let Some(nodes) = &mut self.nodes {
                nodes.push(AnyRootNodeRef::from(node));
            }
            self.index += 1;
        }
    }

    impl<'a> SourceOrderVisitor<'a> for Visitor<'a> {
        #[inline]
        fn visit_mod(&mut self, module: &'a Mod<'a>) {
            self.visit_node(module);
            walk_module(self, module);
        }

        #[inline]
        fn visit_stmt(&mut self, stmt: &'a Stmt<'a>) {
            self.visit_node(stmt);
            walk_stmt(self, stmt);
        }

        #[inline]
        fn visit_annotation(&mut self, expr: &'a Expr<'a>) {
            self.visit_node(expr);
            walk_annotation(self, expr);
        }

        #[inline]
        fn visit_expr(&mut self, expr: &'a Expr<'a>) {
            self.visit_node(expr);
            walk_expr(self, expr);
        }

        #[inline]
        fn visit_decorator(&mut self, decorator: &'a Decorator<'a>) {
            self.visit_node(decorator);
            walk_decorator(self, decorator);
        }

        #[inline]
        fn visit_comprehension(&mut self, comprehension: &'a Comprehension<'a>) {
            self.visit_node(comprehension);
            walk_comprehension(self, comprehension);
        }

        #[inline]
        fn visit_except_handler(&mut self, except_handler: &'a ExceptHandler<'a>) {
            self.visit_node(except_handler);
            walk_except_handler(self, except_handler);
        }

        #[inline]
        fn visit_arguments(&mut self, arguments: &'a Arguments<'a>) {
            self.visit_node(arguments);
            walk_arguments(self, arguments);
        }

        #[inline]
        fn visit_parameters(&mut self, parameters: &'a Parameters<'a>) {
            self.visit_node(parameters);
            walk_parameters(self, parameters);
        }

        #[inline]
        fn visit_parameter(&mut self, arg: &'a Parameter<'a>) {
            self.visit_node(arg);
            walk_parameter(self, arg);
        }

        fn visit_parameter_with_default(
            &mut self,
            parameter_with_default: &'a ParameterWithDefault<'a>,
        ) {
            self.visit_node(parameter_with_default);
            walk_parameter_with_default(self, parameter_with_default);
        }

        #[inline]
        fn visit_keyword(&mut self, keyword: &'a Keyword<'a>) {
            self.visit_node(keyword);
            walk_keyword(self, keyword);
        }

        #[inline]
        fn visit_alias(&mut self, alias: &'a Alias) {
            self.visit_node(alias);
            walk_alias(self, alias);
        }

        #[inline]
        fn visit_with_item(&mut self, with_item: &'a WithItem<'a>) {
            self.visit_node(with_item);
            walk_with_item(self, with_item);
        }

        #[inline]
        fn visit_type_params(&mut self, type_params: &'a TypeParams<'a>) {
            self.visit_node(type_params);
            walk_type_params(self, type_params);
        }

        #[inline]
        fn visit_type_param(&mut self, type_param: &'a TypeParam<'a>) {
            self.visit_node(type_param);
            walk_type_param(self, type_param);
        }

        #[inline]
        fn visit_match_case(&mut self, match_case: &'a MatchCase<'a>) {
            self.visit_node(match_case);
            walk_match_case(self, match_case);
        }

        #[inline]
        fn visit_pattern(&mut self, pattern: &'a Pattern<'a>) {
            self.visit_node(pattern);
            walk_pattern(self, pattern);
        }

        #[inline]
        fn visit_pattern_arguments(&mut self, pattern_arguments: &'a PatternArguments<'a>) {
            self.visit_node(pattern_arguments);
            walk_pattern_arguments(self, pattern_arguments);
        }

        #[inline]
        fn visit_pattern_keyword(&mut self, pattern_keyword: &'a PatternKeyword<'a>) {
            self.visit_node(pattern_keyword);
            walk_pattern_keyword(self, pattern_keyword);
        }

        #[inline]
        fn visit_elif_else_clause(&mut self, elif_else_clause: &'a ElifElseClause<'a>) {
            self.visit_node(elif_else_clause);
            walk_elif_else_clause(self, elif_else_clause);
        }

        #[inline]
        fn visit_f_string(&mut self, f_string: &'a FString<'a>) {
            self.visit_node(f_string);
            walk_f_string(self, f_string);
        }

        #[inline]
        fn visit_interpolated_string_element(
            &mut self,
            interpolated_string_element: &'a InterpolatedStringElement<'a>,
        ) {
            self.visit_node(interpolated_string_element);
            walk_interpolated_string_element(self, interpolated_string_element);
        }

        #[inline]
        fn visit_t_string(&mut self, t_string: &'a TString<'a>) {
            self.visit_node(t_string);
            walk_t_string(self, t_string);
        }

        #[inline]
        fn visit_string_literal(&mut self, string_literal: &'a StringLiteral) {
            self.visit_node(string_literal);
            walk_string_literal(self, string_literal);
        }

        #[inline]
        fn visit_bytes_literal(&mut self, bytes_literal: &'a BytesLiteral) {
            self.visit_node(bytes_literal);
            walk_bytes_literal(self, bytes_literal);
        }

        #[inline]
        fn visit_identifier(&mut self, identifier: &'a Identifier) {
            self.visit_node(identifier);
            walk_identifier(self, identifier);
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::Db;
    use crate::files::{system_path_to_file, vendored_path_to_file};
    use crate::parsed::parsed_module;
    use crate::system::{
        DbWithTestSystem, DbWithWritableSystem as _, SystemPath, SystemVirtualPath,
    };
    use crate::tests::TestDb;
    use crate::vendored::{VendoredFileSystemBuilder, VendoredPath};
    use zip::CompressionMethod;

    #[test]
    fn python_file() -> crate::system::Result<()> {
        let mut db = TestDb::new();
        let path = "test.py";

        db.write_file(path, "x = 10")?;

        let file = system_path_to_file(&db, path).unwrap();

        let parsed = parsed_module(&db, file).load(&db);

        assert!(parsed.has_valid_syntax());

        Ok(())
    }

    #[test]
    fn python_ipynb_file() -> crate::system::Result<()> {
        let mut db = TestDb::new();
        let path = SystemPath::new("test.ipynb");

        db.write_file(path, "%timeit a = b")?;

        let file = system_path_to_file(&db, path).unwrap();

        let parsed = parsed_module(&db, file).load(&db);

        assert!(parsed.has_valid_syntax());

        Ok(())
    }

    #[test]
    fn virtual_python_file() -> crate::system::Result<()> {
        let mut db = TestDb::new();
        let path = SystemVirtualPath::new("untitled:Untitled-1");

        db.write_virtual_file(path, "x = 10");

        let virtual_file = db.files().virtual_file(&db, path);

        let parsed = parsed_module(&db, virtual_file.file()).load(&db);

        assert!(parsed.has_valid_syntax());

        Ok(())
    }

    #[test]
    fn virtual_ipynb_file() -> crate::system::Result<()> {
        let mut db = TestDb::new();
        let path = SystemVirtualPath::new("untitled:Untitled-1.ipynb");

        db.write_virtual_file(path, "%timeit a = b");

        let virtual_file = db.files().virtual_file(&db, path);

        let parsed = parsed_module(&db, virtual_file.file()).load(&db);

        assert!(parsed.has_valid_syntax());

        Ok(())
    }

    #[test]
    fn vendored_file() {
        let mut db = TestDb::new();

        let mut vendored_builder = VendoredFileSystemBuilder::new(CompressionMethod::Stored);
        vendored_builder
            .add_file(
                "path.pyi",
                r#"
import sys

if sys.platform == "win32":
    from ntpath import *
    from ntpath import __all__ as __all__
else:
    from posixpath import *
    from posixpath import __all__ as __all__"#,
            )
            .unwrap();
        let vendored = vendored_builder.finish().unwrap();
        db.with_vendored(vendored);

        let file = vendored_path_to_file(&db, VendoredPath::new("path.pyi")).unwrap();

        let parsed = parsed_module(&db, file).load(&db);

        assert!(parsed.has_valid_syntax());
    }
}
