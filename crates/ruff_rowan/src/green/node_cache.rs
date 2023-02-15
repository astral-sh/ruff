use hashbrown::hash_map::{RawEntryMut, RawOccupiedEntryMut, RawVacantEntryMut};
use ruff_text_size::TextSize;
use rustc_hash::FxHasher;
use std::hash::{BuildHasherDefault, Hash, Hasher};

use crate::green::Slot;
use crate::syntax::{TriviaPiece, TriviaPieceKind};
use crate::{
    green::GreenElementRef, GreenNode, GreenNodeData, GreenToken, GreenTokenData, NodeOrToken,
    RawSyntaxKind,
};

use super::element::GreenElement;
use super::trivia::GreenTrivia;

type HashMap<K, V> = hashbrown::HashMap<K, V, BuildHasherDefault<FxHasher>>;

/// A token stored in the `NodeCache`.
/// Does intentionally not implement `Hash` to have compile-time guarantees that the `NodeCache`
/// uses the correct hash.
#[derive(Debug)]
struct CachedToken(GreenToken);

/// A node stored in the `NodeCache`. It stores a pre-computed hash
/// because re-computing the hash requires traversing the whole sub-tree.
/// The hash also differs from the `GreenNode` hash implementation as it
/// only hashes occupied slots and excludes empty slots.
///
/// Does intentionally not implement `Hash` to have compile-time guarantees that the `NodeCache`
/// uses the correct hash.
#[derive(Debug)]
struct CachedNode {
    node: GreenNode,
    // Store the hash as it's expensive to re-compute
    // involves re-computing the hash of the whole sub-tree
    hash: u64,
}

/// Interner for GreenTokens and GreenNodes
// XXX: the impl is a bit tricky. As usual when writing interners, we want to
// store all values in one HashSet.
//
// However, hashing trees is fun: hash of the tree is recursively defined. We
// maintain an invariant -- if the tree is interned, then all of its children
// are interned as well.
//
// That means that computing the hash naively is wasteful -- we just *know*
// hashes of children, and we can re-use those.
//
// So here we use *raw* API of hashbrown and provide the hashes manually,
// instead of going via a `Hash` impl. Our manual `Hash` and the
// `#[derive(Hash)]` are actually different! At some point we had a fun bug,
// where we accidentally mixed the two hashes, which made the cache much less
// efficient.
//
// To fix that, we additionally wrap the data in `Cached*` wrappers, to make sure
// we don't accidentally use the wrong hash!
#[derive(Default, Debug)]
pub struct NodeCache {
    nodes: HashMap<CachedNode, ()>,
    tokens: HashMap<CachedToken, ()>,
    trivia: TriviaCache,
}

fn token_hash_of(kind: RawSyntaxKind, text: &str) -> u64 {
    let mut h = FxHasher::default();
    kind.hash(&mut h);
    text.hash(&mut h);
    h.finish()
}

fn token_hash(token: &GreenTokenData) -> u64 {
    token_hash_of(token.kind(), token.text())
}

fn element_id(elem: GreenElementRef<'_>) -> *const () {
    match elem {
        NodeOrToken::Node(it) => it as *const GreenNodeData as *const (),
        NodeOrToken::Token(it) => it as *const GreenTokenData as *const (),
    }
}

impl NodeCache {
    /// Hash used for nodes that haven't been cached because it has too many slots or
    /// one of its children wasn't cached.
    const UNCACHED_NODE_HASH: u64 = 0;

    /// Tries to retrieve a node with the given `kind` and `children` from the cache.
    ///
    /// Returns an entry that allows the caller to:
    /// * Retrieve the cached node if it is present in the cache
    /// * Insert a node if it isn't present in the cache
    pub(crate) fn node(
        &mut self,
        kind: RawSyntaxKind,
        children: &[(u64, GreenElement)],
    ) -> NodeCacheNodeEntryMut {
        if children.len() > 3 {
            return NodeCacheNodeEntryMut::NoCache(Self::UNCACHED_NODE_HASH);
        }

        let hash = {
            let mut h = FxHasher::default();
            kind.hash(&mut h);
            for &(hash, _) in children {
                if hash == Self::UNCACHED_NODE_HASH {
                    return NodeCacheNodeEntryMut::NoCache(Self::UNCACHED_NODE_HASH);
                }
                hash.hash(&mut h);
            }
            h.finish()
        };

        // Green nodes are fully immutable, so it's ok to deduplicate them.
        // This is the same optimization that Roslyn does
        // https://github.com/KirillOsenkov/Bliki/wiki/Roslyn-Immutable-Trees
        //
        // For example, all `#[inline]` in this file share the same green node!
        // For `libsyntax/parse/parser.rs`, measurements show that deduping saves
        // 17% of the memory for green nodes!
        let entry = self.nodes.raw_entry_mut().from_hash(hash, |no_hash| {
            no_hash.node.kind() == kind && {
                let lhs = no_hash.node.slots().filter_map(|slot| match slot {
                    // Ignore empty slots. The queried node only has the present children
                    Slot::Empty { .. } => None,
                    Slot::Node { node, .. } => Some(element_id(NodeOrToken::Node(node))),
                    Slot::Token { token, .. } => Some(element_id(NodeOrToken::Token(token))),
                });

                let rhs = children
                    .iter()
                    .map(|(_, element)| element_id(element.as_deref()));

                lhs.eq(rhs)
            }
        });

        match entry {
            RawEntryMut::Occupied(entry) => NodeCacheNodeEntryMut::Cached(CachedNodeEntry {
                hash,
                raw_entry: entry,
            }),
            RawEntryMut::Vacant(entry) => NodeCacheNodeEntryMut::Vacant(VacantNodeEntry {
                raw_entry: entry,
                original_kind: kind,
                hash,
            }),
        }
    }

    pub(crate) fn token(&mut self, kind: RawSyntaxKind, text: &str) -> (u64, GreenToken) {
        self.token_with_trivia(kind, text, &[], &[])
    }

    pub(crate) fn token_with_trivia(
        &mut self,
        kind: RawSyntaxKind,
        text: &str,
        leading: &[TriviaPiece],
        trailing: &[TriviaPiece],
    ) -> (u64, GreenToken) {
        let hash = token_hash_of(kind, text);

        let entry = self.tokens.raw_entry_mut().from_hash(hash, |token| {
            token.0.kind() == kind && token.0.text() == text
        });

        let token = match entry {
            RawEntryMut::Occupied(entry) => entry.key().0.clone(),
            RawEntryMut::Vacant(entry) => {
                let leading = self.trivia.get(leading);
                let trailing = self.trivia.get(trailing);

                let token = GreenToken::with_trivia(kind, text, leading, trailing);
                entry
                    .insert_with_hasher(hash, CachedToken(token.clone()), (), |t| token_hash(&t.0));
                token
            }
        };

        (hash, token)
    }
}

pub(crate) enum NodeCacheNodeEntryMut<'a> {
    Cached(CachedNodeEntry<'a>),

    /// A node that should not be cached
    NoCache(u64),
    Vacant(VacantNodeEntry<'a>),
}

/// Represents a vacant entry, a node that hasn't been cached yet.
/// The `insert` method allows to place a node inside of the vacant entry. The inserted node
/// may have a different representation (kind or children) than the originally queried node.
/// For example, a node may change its kind to bogus or add empty slots. The only importance is
/// that these changes apply for all nodes that have the same shape as the originally queried node.
pub(crate) struct VacantNodeEntry<'a> {
    hash: u64,
    original_kind: RawSyntaxKind,
    raw_entry: RawVacantEntryMut<'a, CachedNode, (), BuildHasherDefault<FxHasher>>,
}

/// Represents an entry of a cached node.
pub(crate) struct CachedNodeEntry<'a> {
    hash: u64,
    raw_entry: RawOccupiedEntryMut<'a, CachedNode, (), BuildHasherDefault<FxHasher>>,
}

impl<'a> CachedNodeEntry<'a> {
    pub fn node(&self) -> &GreenNode {
        &self.raw_entry.key().node
    }

    pub fn hash(&self) -> u64 {
        self.hash
    }
}

impl<'a> VacantNodeEntry<'a> {
    /// Inserts the `node` into the cache so that future queries for the same kind and children resolve to the passed `node`.
    ///
    /// Returns the hash of the node.
    ///
    /// The cache does not cache the `node` if the kind doesn't match the `kind` of the queried node because
    /// cache lookups wouldn't be successful because the hash collision prevention check compares the kinds of the
    /// cached and queried node.
    pub fn cache(self, node: GreenNode) -> u64 {
        if self.original_kind != node.kind() {
            // The kind has changed since it has been queried. For example, the node has been converted to an
            // unknown node. Never cache these nodes because cache lookups will never match.
            NodeCache::UNCACHED_NODE_HASH
        } else {
            self.raw_entry.insert_with_hasher(
                self.hash,
                CachedNode {
                    node,
                    hash: self.hash,
                },
                (),
                |n| n.hash,
            );
            self.hash
        }
    }
}

/// A cached [GreenTrivia].
/// Deliberately doesn't implement `Hash` to make sure all
/// usages go through the custom `FxHasher`.
#[derive(Debug)]
struct CachedTrivia(GreenTrivia);

#[derive(Debug)]
struct TriviaCache {
    /// Generic cache for trivia
    cache: HashMap<CachedTrivia, ()>,

    /// Cached single whitespace trivia.
    whitespace: GreenTrivia,
}

impl Default for TriviaCache {
    fn default() -> Self {
        Self {
            cache: Default::default(),
            whitespace: GreenTrivia::new([TriviaPiece::whitespace(1)]),
        }
    }
}

impl TriviaCache {
    /// Tries to retrieve a [GreenTrivia] with the given pieces from the cache or creates a new one and caches
    /// it for further calls.
    fn get(&mut self, pieces: &[TriviaPiece]) -> GreenTrivia {
        match pieces {
            [] => GreenTrivia::empty(),
            [TriviaPiece {
                kind: TriviaPieceKind::Whitespace,
                length,
            }] if *length == TextSize::from(1) => self.whitespace.clone(),

            _ => {
                let hash = Self::trivia_hash_of(pieces);

                let entry = self
                    .cache
                    .raw_entry_mut()
                    .from_hash(hash, |trivia| trivia.0.pieces() == pieces);

                match entry {
                    RawEntryMut::Occupied(entry) => entry.key().0.clone(),
                    RawEntryMut::Vacant(entry) => {
                        let trivia = GreenTrivia::new(pieces.iter().copied());
                        entry.insert_with_hasher(
                            hash,
                            CachedTrivia(trivia.clone()),
                            (),
                            |cached| Self::trivia_hash_of(cached.0.pieces()),
                        );
                        trivia
                    }
                }
            }
        }
    }

    fn trivia_hash_of(pieces: &[TriviaPiece]) -> u64 {
        let mut h = FxHasher::default();

        pieces.len().hash(&mut h);

        for piece in pieces {
            piece.hash(&mut h);
        }

        h.finish()
    }
}

#[cfg(test)]
mod tests {
    use crate::green::node_cache::token_hash;
    use crate::green::trivia::GreenTrivia;
    use crate::{GreenToken, RawSyntaxKind};
    use ruff_text_size::TextSize;

    #[test]
    fn green_token_hash() {
        let kind = RawSyntaxKind(0);
        let text = " let ";
        let t1 = GreenToken::with_trivia(
            kind,
            text,
            GreenTrivia::whitespace(TextSize::from(1)),
            GreenTrivia::whitespace(TextSize::from(1)),
        );
        let t2 = GreenToken::with_trivia(
            kind,
            text,
            GreenTrivia::whitespace(1),
            GreenTrivia::whitespace(1),
        );

        assert_eq!(token_hash(&t1), token_hash(&t2));

        let t3 = GreenToken::new(kind, "let");
        assert_ne!(token_hash(&t1), token_hash(&t3));

        let t4 = GreenToken::with_trivia(
            kind,
            "\tlet ",
            GreenTrivia::whitespace(1),
            GreenTrivia::whitespace(1),
        );
        assert_ne!(token_hash(&t1), token_hash(&t4));
    }
}
