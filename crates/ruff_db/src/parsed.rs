use std::fmt::Formatter;
use std::sync::Arc;

use arc_swap::ArcSwapOption;
use get_size2::GetSize;
use ruff_python_ast::{
    AnyRootNodeRef, HasNodeIndex, ModExpression, ModModule, NodeIndex, NodeIndexError,
    StringLiteral,
};
use ruff_python_parser::{
    ParseError, ParseErrorType, ParseOptions, Parsed, parse_string_annotation, parse_unchecked,
};

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

    let parsed = parsed_module_impl(db, file);

    ParsedModule::new(file, parsed)
}

pub(super) fn disable_lru(db: &mut dyn Db) {
    parsed_module::set_lru_capacity(db, 0);
}

pub fn parsed_module_impl(db: &dyn Db, file: File) -> Parsed<ModModule> {
    let source = source_text(db, file);
    let ty = file.source_type(db);

    let target_version = db.python_version();
    let options = ParseOptions::from(ty).with_target_version(target_version);
    parse_unchecked(&source, options)
        .try_into_module()
        .expect("PySourceType always parses into a module")
}

pub fn parsed_string_annotation(
    source: &str,
    string: &StringLiteral,
) -> Result<Parsed<ModExpression>, ParseError> {
    let expr = parse_string_annotation(source, string)?;

    // We need the sub-ast of the string annotation to be indexed
    indexed::ensure_indexed(&expr, string.node_index().load()).map_err(|err| {
        let message = match err {
            NodeIndexError::NoParent => {
                "Internal error: string annotation's parent had no NodeIndex"
            }
            NodeIndexError::TooNested => {
                "Too many levels of nested string annotations; \
                remove the redundant nested quotes"
            }
            NodeIndexError::OverflowedIndices => {
                "File too long for string annotations; either break up the file \
                or don't use string annotations"
            }
            NodeIndexError::OverflowedSubIndices => {
                "File too long for nested string annotations; remove the redundant nested quotes"
            }
            NodeIndexError::ExhaustedSubIndices => {
                "String annotation is too long; consider introducing type aliases to simplify"
            }
            NodeIndexError::ExhaustedSubSubIndices => {
                "Nested string annotation is too long; remove the redundant nested quotes"
            }
        };

        ParseError {
            error: ParseErrorType::StringAnnotationError(message),
            location: string.range,
        }
    })?;

    Ok(expr)
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
    pub fn new(file: File, parsed: Parsed<ModModule>) -> Self {
        Self {
            file,
            inner: Arc::new(ArcSwapOption::new(Some(indexed::IndexedModule::new(
                parsed,
            )))),
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
                let parsed = indexed::IndexedModule::new(parsed_module_impl(db, self.file));
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
}

impl std::ops::Deref for ParsedModuleRef {
    type Target = Parsed<ModModule>;

    fn deref(&self) -> &Self::Target {
        &self.indexed.parsed
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

    use ruff_python_ast::visitor::source_order::*;
    use ruff_python_ast::*;
    use ruff_python_parser::Parsed;

    /// A wrapper around the AST that allows access to AST nodes by index.
    #[derive(Debug, get_size2::GetSize)]
    pub struct IndexedModule {
        index: IndexedNodes,
        pub parsed: Parsed<ModModule>,
    }

    /// Compact storage for the address and [`RootNodeKind`] of every indexed AST node.
    ///
    /// This stores the information needed to reconstruct an [`AnyRootNodeRef`] without retaining
    /// a fat pointer per node. Entries are divided into fixed-size chunks so that unrelated AST
    /// allocations do not force every node into a wider representation. Each chunk starts on a
    /// word boundary in `words`.
    #[derive(Debug, Default, get_size2::GetSize)]
    struct IndexedNodes {
        chunks: Box<[IndexChunk]>,
        words: Box<[u64]>,
    }

    /// Describes the entries for one consecutive group of node indices.
    #[derive(Debug, get_size2::GetSize)]
    struct IndexChunk {
        /// Minimum node address in a relative chunk; unused for a wide chunk.
        base: usize,
        /// Index of this chunk's first word in [`IndexedNodes::words`].
        word_start: u32,
        /// Number of bits per packed entry.
        entry_bits: u8,
        /// Number of entries, which is only less than [`IndexedNodes::CHUNK_LEN`] in the last
        /// chunk.
        entry_count: u8,
        layout: IndexChunkLayout,
    }

    #[derive(Copy, Clone, Debug, get_size2::GetSize)]
    #[repr(u8)]
    enum IndexChunkLayout {
        /// Packs each scaled address offset together with its root-node kind:
        ///
        /// ```text
        /// | address offset | root-node kind |
        ///   entry_bits - 5       5 bits
        /// ```
        ///
        /// The original address is `base + address_offset * ALIGNMENT`. Entries may cross `u64`
        /// boundaries and the unused bits at the end of the chunk are padding.
        Relative,
        /// Stores full addresses followed by a packed stream of root-node kinds:
        ///
        /// ```text
        /// | address 0 | ... | address n - 1 | kind 0 | ... | kind n - 1 |
        ///     64 bits           64 bits        5 bits          5 bits
        /// ```
        Wide,
    }

    impl IndexedNodes {
        const ALIGNMENT: usize = std::mem::align_of::<AtomicNodeIndex>();
        const CHUNK_LEN: usize = 64;
        const KIND_BITS: u8 = 5;
        const KIND_MASK: u64 = (1 << Self::KIND_BITS) - 1;

        fn from_collected(nodes: CollectedNodes) -> Self {
            let CollectedNodes { addresses, kinds } = nodes;
            debug_assert_eq!(addresses.len(), kinds.len());

            let mut chunks = Vec::with_capacity(addresses.len().div_ceil(Self::CHUNK_LEN));
            let mut words = Vec::new();

            for (address_chunk, kind_chunk) in addresses
                .chunks(Self::CHUNK_LEN)
                .zip(kinds.chunks(Self::CHUNK_LEN))
            {
                let base = address_chunk.iter().copied().min().unwrap_or_default();
                let max = address_chunk.iter().copied().max().unwrap_or_default();
                let aligned = address_chunk
                    .iter()
                    .all(|address| address.is_multiple_of(Self::ALIGNMENT));
                let offset_bits = usize::BITS - ((max - base) / Self::ALIGNMENT).leading_zeros();
                let relative_bits = u8::try_from(offset_bits)
                    .expect("an address offset cannot require more than u8::MAX bits")
                    + Self::KIND_BITS;
                let word_start = u32::try_from(words.len())
                    .expect("indexed AST bitstream should fit in u32 words");

                if aligned && relative_bits <= 64 {
                    let entry_count = u8::try_from(address_chunk.len())
                        .expect("an index chunk contains at most 64 entries");
                    chunks.push(IndexChunk {
                        base,
                        word_start,
                        entry_bits: relative_bits,
                        entry_count,
                        layout: IndexChunkLayout::Relative,
                    });
                    for (entry, (&address, &kind)) in
                        address_chunk.iter().zip(kind_chunk).enumerate()
                    {
                        let offset = (address - base) / Self::ALIGNMENT;
                        let offset = u64::try_from(offset)
                            .expect("relative address offset was checked to fit in 64 bits");
                        Self::write_bits(
                            &mut words,
                            word_start as usize * 64 + entry * usize::from(relative_bits),
                            (offset << Self::KIND_BITS) | u64::from(kind as u8),
                            relative_bits,
                        );
                    }
                } else {
                    // Wide chunks store one address word per entry followed by packed node kinds.
                    let entry_count = u8::try_from(address_chunk.len())
                        .expect("an index chunk contains at most 64 entries");
                    chunks.push(IndexChunk {
                        base: 0,
                        word_start,
                        entry_bits: Self::KIND_BITS,
                        entry_count,
                        layout: IndexChunkLayout::Wide,
                    });
                    words.extend(address_chunk.iter().map(|address| {
                        u64::try_from(*address)
                            .expect("AST node addresses should fit in a bitstream word")
                    }));
                    for (entry, &kind) in kind_chunk.iter().enumerate() {
                        Self::write_bits(
                            &mut words,
                            (word_start as usize + address_chunk.len()) * 64
                                + entry * usize::from(Self::KIND_BITS),
                            u64::from(kind as u8),
                            Self::KIND_BITS,
                        );
                    }
                }
            }

            Self {
                chunks: chunks.into_boxed_slice(),
                words: words.into_boxed_slice(),
            }
        }

        fn write_bits(words: &mut Vec<u64>, bit: usize, value: u64, bits: u8) {
            debug_assert!((1..=64).contains(&bits));
            let word = bit / 64;
            let shift = bit % 64;
            let end = bit + usize::from(bits);
            words.resize(words.len().max(end.div_ceil(64)), 0);
            words[word] |= value << shift;
            if end > (word + 1) * 64 {
                words[word + 1] |= value >> (64 - shift);
            }
        }

        fn read_bits(words: &[u64], bit: usize, bits: u8) -> u64 {
            debug_assert!((1..=64).contains(&bits));
            let word = bit / 64;
            let shift = bit % 64;
            let low = words[word] >> shift;
            let value = if shift + usize::from(bits) <= 64 {
                low
            } else {
                low | (words[word + 1] << (64 - shift))
            };
            if bits == 64 {
                value
            } else {
                value & ((1 << bits) - 1)
            }
        }

        #[cfg(test)]
        fn len(&self) -> usize {
            self.chunks
                .iter()
                .map(|chunk| usize::from(chunk.entry_count))
                .sum()
        }

        fn get(&self, index: usize) -> (usize, RootNodeKind) {
            let chunk_index = index / Self::CHUNK_LEN;
            let entry_index = index % Self::CHUNK_LEN;
            let chunk = &self.chunks[chunk_index];
            let words = &self.words[chunk.word_start as usize..];

            match chunk.layout {
                IndexChunkLayout::Relative => {
                    let entry = Self::read_bits(
                        words,
                        entry_index * usize::from(chunk.entry_bits),
                        chunk.entry_bits,
                    );
                    let offset = (entry >> Self::KIND_BITS) as usize;
                    let kind = RootNodeKind::from_u8((entry & Self::KIND_MASK) as u8)
                        .expect("packed node kind should be valid");
                    (chunk.base + offset * Self::ALIGNMENT, kind)
                }
                IndexChunkLayout::Wide => {
                    let address = usize::try_from(words[entry_index])
                        .expect("stored AST node address should fit in usize");
                    let kind_bit = usize::from(chunk.entry_count) * 64
                        + entry_index * usize::from(Self::KIND_BITS);
                    let kind =
                        RootNodeKind::from_u8(
                            Self::read_bits(words, kind_bit, Self::KIND_BITS) as u8
                        )
                        .expect("packed node kind should be valid");
                    (address, kind)
                }
            }
        }
    }

    const _: () = assert!(RootNodeKind::ALL.len() <= 1 << IndexedNodes::KIND_BITS);

    /// Ensure the following sub-AST is indexed, using the parent node's index
    /// as a basis for unambiguous AST node indices.
    pub fn ensure_indexed(
        parsed: &Parsed<ModExpression>,
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
        /// Create a new [`IndexedModule`] from the given AST.
        pub fn new(parsed: Parsed<ModModule>) -> Arc<Self> {
            let mut visitor = Visitor {
                nodes: Some(CollectedNodes::default()),
                index: 0,
                max_index: MAX_REAL_INDEX,
                overflowed: false,
            };

            let mut inner = Arc::new(IndexedModule {
                parsed,
                index: IndexedNodes::default(),
            });

            AnyNodeRef::from(inner.parsed.syntax()).visit_source_order(&mut visitor);

            let nodes = visitor
                .nodes
                .expect("top-level AST visitor should collect indexed nodes");
            Arc::get_mut(&mut inner)
                .expect("newly created indexed module should have a unique Arc")
                .index = IndexedNodes::from_collected(nodes);

            inner
        }

        /// Returns the node at the given index.
        pub fn get_by_index<'ast>(&'ast self, index: NodeIndex) -> AnyRootNodeRef<'ast> {
            let index = index
                .as_u32()
                .expect("attempted to access uninitialized `NodeIndex`");

            let index = index as usize;
            let (address, kind) = self.index.get(index);

            // SAFETY: `kind` and `address` were recorded from the same node, and the node is owned
            // by `self.parsed`, so it lives for as long as this borrow of `self`.
            unsafe {
                AnyRootNodeRef::from_raw_parts(
                    kind,
                    std::ptr::NonNull::with_exposed_provenance(
                        std::num::NonZeroUsize::new(address)
                            .expect("recorded AST node address should be non-null"),
                    ),
                )
            }
        }
    }

    #[derive(Default)]
    struct CollectedNodes {
        addresses: Vec<usize>,
        kinds: Vec<RootNodeKind>,
    }

    /// A visitor that collects nodes in source order.
    struct Visitor {
        index: u32,
        max_index: u32,
        nodes: Option<CollectedNodes>,
        overflowed: bool,
    }

    impl Visitor {
        fn visit_node<'a, T>(&mut self, node: &'a T)
        where
            T: HasNodeIndex,
            AnyRootNodeRef<'a>: From<&'a T>,
        {
            // Only check on write (the maximum is orders of magnitude less than u32::MAX)
            if self.index > self.max_index {
                self.overflowed = true;
            } else {
                node.node_index().set(NodeIndex::from(self.index));
            }

            if let Some(nodes) = &mut self.nodes {
                let (kind, pointer) = AnyRootNodeRef::from(node).into_raw_parts();
                nodes.addresses.push(pointer.as_ptr().expose_provenance());
                nodes.kinds.push(kind);
            }
            self.index += 1;
        }
    }

    impl<'a> SourceOrderVisitor<'a> for Visitor {
        #[inline]
        fn visit_stmt(&mut self, stmt: &'a Stmt) {
            self.visit_node(stmt);
            walk_stmt(self, stmt);
        }

        #[inline]
        fn visit_annotation(&mut self, expr: &'a Expr) {
            // `walk_annotation` delegates to `visit_expr`, which indexes the expression once.
            walk_annotation(self, expr);
        }

        #[inline]
        fn visit_expr(&mut self, expr: &'a Expr) {
            self.visit_node(expr);
            walk_expr(self, expr);
        }

        #[inline]
        fn visit_decorator(&mut self, decorator: &'a Decorator) {
            self.visit_node(decorator);
            walk_decorator(self, decorator);
        }

        #[inline]
        fn visit_comprehension(&mut self, comprehension: &'a Comprehension) {
            self.visit_node(comprehension);
            walk_comprehension(self, comprehension);
        }

        #[inline]
        fn visit_except_handler(&mut self, except_handler: &'a ExceptHandler) {
            self.visit_node(except_handler);
            walk_except_handler(self, except_handler);
        }

        #[inline]
        fn visit_arguments(&mut self, arguments: &'a Arguments) {
            self.visit_node(arguments);
            walk_arguments(self, arguments);
        }

        #[inline]
        fn visit_parameters(&mut self, parameters: &'a Parameters) {
            self.visit_node(parameters);
            walk_parameters(self, parameters);
        }

        #[inline]
        fn visit_parameter(&mut self, arg: &'a Parameter) {
            self.visit_node(arg);
            walk_parameter(self, arg);
        }

        fn visit_parameter_with_default(
            &mut self,
            parameter_with_default: &'a ParameterWithDefault,
        ) {
            self.visit_node(parameter_with_default);
            walk_parameter_with_default(self, parameter_with_default);
        }

        #[inline]
        fn visit_keyword(&mut self, keyword: &'a Keyword) {
            self.visit_node(keyword);
            walk_keyword(self, keyword);
        }

        #[inline]
        fn visit_alias(&mut self, alias: &'a Alias) {
            self.visit_node(alias);
            walk_alias(self, alias);
        }

        #[inline]
        fn visit_with_item(&mut self, with_item: &'a WithItem) {
            self.visit_node(with_item);
            walk_with_item(self, with_item);
        }

        #[inline]
        fn visit_type_params(&mut self, type_params: &'a TypeParams) {
            self.visit_node(type_params);
            walk_type_params(self, type_params);
        }

        #[inline]
        fn visit_type_param(&mut self, type_param: &'a TypeParam) {
            self.visit_node(type_param);
            walk_type_param(self, type_param);
        }

        #[inline]
        fn visit_match_case(&mut self, match_case: &'a MatchCase) {
            self.visit_node(match_case);
            walk_match_case(self, match_case);
        }

        #[inline]
        fn visit_pattern(&mut self, pattern: &'a Pattern) {
            self.visit_node(pattern);
            walk_pattern(self, pattern);
        }

        #[inline]
        fn visit_pattern_arguments(&mut self, pattern_arguments: &'a PatternArguments) {
            self.visit_node(pattern_arguments);
            walk_pattern_arguments(self, pattern_arguments);
        }

        #[inline]
        fn visit_pattern_keyword(&mut self, pattern_keyword: &'a PatternKeyword) {
            self.visit_node(pattern_keyword);
            walk_pattern_keyword(self, pattern_keyword);
        }

        #[inline]
        fn visit_elif_else_clause(&mut self, elif_else_clause: &'a ElifElseClause) {
            self.visit_node(elif_else_clause);
            walk_elif_else_clause(self, elif_else_clause);
        }

        #[inline]
        fn visit_f_string(&mut self, f_string: &'a FString) {
            self.visit_node(f_string);
            walk_f_string(self, f_string);
        }

        #[inline]
        fn visit_interpolated_string_element(
            &mut self,
            interpolated_string_element: &'a InterpolatedStringElement,
        ) {
            self.visit_node(interpolated_string_element);
            walk_interpolated_string_element(self, interpolated_string_element);
        }

        #[inline]
        fn visit_t_string(&mut self, t_string: &'a TString) {
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

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn indexed_nodes_round_trip() {
            let parsed = ruff_python_parser::parse_module(
                r#"
import os as imported_os

@decorator
class C[T](Base, metaclass=Meta):
    def method(self, value: int = 1, *args, keyword=2, **kwargs):
        try:
            with context() as items:
                return [item for item in items if item]
        except Error as error:
            match error:
                case Error(code=code):
                    if code:
                        return f"{code!r}"
                    elif code is None:
                        return t"{code}"
                    else:
                        return "string"
                case _:
                    return b"bytes"
"#,
            )
            .expect("test source should parse");
            let indexed = IndexedModule::new(parsed);

            let node_count = u32::try_from(indexed.index.len())
                .expect("number of indexed nodes should fit in u32");
            let mut seen_kinds = [false; 1 << IndexedNodes::KIND_BITS];

            for raw_index in 0..node_count {
                let index = NodeIndex::from(raw_index);
                let (_, kind) = indexed.index.get(raw_index as usize);
                seen_kinds[usize::from(kind as u8)] = true;
                assert_eq!(indexed.get_by_index(index).node_index().load(), index);
            }

            for kind in RootNodeKind::ALL {
                let is_indexed = !matches!(
                    kind,
                    RootNodeKind::Mod | RootNodeKind::InterpolatedStringFormatSpec
                );
                assert_eq!(seen_kinds[usize::from(*kind as u8)], is_indexed);
            }
        }

        #[test]
        fn indexed_node_storage_layouts() {
            let alignment = IndexedNodes::ALIGNMENT;
            let chunk_distance = 1usize << (usize::BITS - 3);
            let addresses: Vec<_> = (0..130)
                .map(|index| {
                    0x1000
                        + (index / IndexedNodes::CHUNK_LEN) * chunk_distance
                        + (index % IndexedNodes::CHUNK_LEN) * alignment
                })
                .collect();
            let kinds: Vec<_> = (0..addresses.len())
                .map(|index| {
                    if index.is_multiple_of(2) {
                        RootNodeKind::Stmt
                    } else {
                        RootNodeKind::Identifier
                    }
                })
                .collect();
            let chunked = IndexedNodes::from_collected(CollectedNodes {
                addresses: addresses.clone(),
                kinds: kinds.clone(),
            });

            assert_eq!(chunked.chunks.len(), 3);
            assert!(
                chunked
                    .chunks
                    .iter()
                    .all(|chunk| matches!(chunk.layout, IndexChunkLayout::Relative))
            );
            for (index, (&address, &kind)) in addresses.iter().zip(&kinds).enumerate() {
                assert_eq!(chunked.get(index), (address, kind));
            }

            let wide = IndexedNodes::from_collected(CollectedNodes {
                addresses: vec![0x1000, 0x1001],
                kinds: vec![RootNodeKind::Stmt, RootNodeKind::Identifier],
            });
            assert!(matches!(wide.chunks[0].layout, IndexChunkLayout::Wide));
            assert_eq!(wide.get(0), (0x1000, RootNodeKind::Stmt));
            assert_eq!(wide.get(1), (0x1001, RootNodeKind::Identifier));
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
