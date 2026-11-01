# Python AST storage

A parsed Python file owns one syntax tree. Its module, statements, expressions, identifiers, and
literal parts each have a row in the same table. Child fields contain four-byte row IDs. For
example, the operands of `left + right` are two IDs in the binary expression's payload. Reading
either operand requires the table that owns those IDs. Ruff and ty share the same parser and
tree representation.
[SWC describes the design that inspired this layout.](https://next.swc.rs/architecture/flat-ast)

## Borrowed nodes and stored IDs

`ExprBinOp<'ast>` borrows a binary expression in a parsed tree. Like every concrete node and
group such as `Expr<'ast>`, it wraps one `SyntaxNode<'ast>`. This holds a reference to the
row and a reference to its owning `Ast`. Copying a node copies those two references.
`binary.left()` resolves the left operand in that tree. `binary.syntax_node()` returns the
same row and arena through the common `AstNode` trait.

Use `AstNodeId<kind::ExprBinOp>` to store the identity of a binary expression without
borrowing the tree. The lifetime-free marker `kind::ExprBinOp` implements `NodeType`,
which associates it with its borrowed type `ExprBinOp<'ast>`. A group marker such as
`kind::Expr` accepts any expression kind. `Ast::view` takes the ID and returns the
corresponding borrowed node. IDs are local to a particular parse.

An ID stores the node’s allocation index plus one as a `NonZeroU32`. Zero represents an
absent child, so `Option<AstNodeId<T>>` occupies the same four bytes as `AstNodeId<T>`.
The first node occupies row 0 and has `id.index() == 0`. Use `ast.row(0)` to borrow it.
`ast.len()` counts stored nodes and `ast.any_node(predicate)` scans them, including rows
retained by parser recovery. Use `root.descendants()` to walk the reachable tree in source order.

Accept `Expr<'ast>` when reading any expression or `Stmt<'ast>` when reading a statement.
A concrete `ExprCall<'ast>` converts to `Expr<'ast>` with `.into()` and still refers to the
same row. Call `classify()` to match its variant.

```rust
use ruff_python_ast::{AstNode, ExprKind};
use ruff_python_parser::{ParseError, parse_expression};
use ruff_text_size::{Ranged, TextRange, TextSize};

fn main() -> Result<(), ParseError> {
    let parsed = parse_expression("left + right")?;
    let ExprKind::BinOp(binary) = parsed.root().body().classify() else {
        panic!("expected a binary expression");
    };
    let left = binary.left();
    assert_eq!(left.range(), TextRange::up_to(TextSize::new(4)));
    let saved = left.node_id();
    assert_eq!(parsed.ast().view(saved), Some(left));
    Ok(())
}
```

`expr.as_call_expr()` checks the kind and returns a typed call view. Reading `call.func()`
then resolves only the callee. The generated `fields()` method is useful for destructuring a
whole node. It constructs all of that node's borrowed fields, including fields a pattern
ignores with `..`.

Visitors that accept any node use `SyntaxNode`.
`node.cast::<Stmt>()` returns a statement view when the row contains a statement. The same
visitor can walk a parsed module or a tree constructed by a lint fix.

`first_child()` and `last_child()` return the endpoints of `children()` in source order.
`last_child()` walks the immediate children. Use `suite.last()` to read the last statement
of a known suite directly.

`descendants()` includes the starting node and visits parents before their children.
Use `descendants().skip_subtrees(|node| node.is_stmt_function_def())` to visit a function
definition and continue past its entire subtree. For statement searches, `descendant_statements()`
follows only suites and the clauses that contain them. It yields an import statement once
without visiting its aliases, and reaches statements in every handler of a `try`.

When the linter dispatches several rules on a call, it can resolve the callee and arguments
once and pass each rule a reference to the same `ExprCallFields`. The `fields()` method
returns the immediate fields for destructuring. Its `node` field is the original borrowed
node. Use the node’s generated accessor when only one field is needed.

## Row layout

Each row occupies 24 bytes, with 16 bytes for fields and the kind followed by an eight-byte source span. The
first 15 bytes hold integer fields for the node's kind. The last byte holds
the kind itself. A visitor can read the kind and span from the row it already borrows.
The parser appends each completed node in one operation.

The schema declares each node's fields. The generator assigns their physical locations and
generates constructors, typed accessors, and traversal from the same declarations. A binary
expression fits both operand IDs and its operator inline. When the fields need more space, one
aligned four-byte field points into extra data. The remaining inline fields can occupy up to
eleven bytes. Each spilled field takes one or two four-byte words.
[Schema](ast.toml) · [Generator](generate.py)

Only the row constructor writes the kind. Generated getters and setters read and write a
fixed range of bytes for each integer field. Updating a name’s expression context leaves
its identifier and kind intact. [Row implementation](src/flat/row.rs)

Lists store a starting slot and an element count. A `NodeList<T>` reads a row ID from one
four-byte word for each element and returns a `T::Borrowed` with the same tree borrow. A `Suite` is
a `NodeList<kind::Stmt>`. An empty list consumes no element slots in the extra-data column.
Call and pattern arguments share a boundary between their positional and keyword lists. The
row stores that boundary and both lengths in twelve bytes. The parser appends the lists next
to each other. A builder that supplies two nonadjacent, nonempty lists copies them into
consecutive slots.

A `Parameters` row stores its two optional variadic children inline. Its three groups of
ordinary parameters have list bounds in the extra-data column.

A dictionary display stores a list of `DictItem` IDs. Each item holds its optional key and its
value. In `{**mapping}` the entry starts at `**` and its only child is `mapping`. Every AST list
element occupies one four-byte word. Side-table offsets have their own encoding and can start at
zero. UTF-8 text and arbitrary byte strings have separate buffers, so a bytes literal can contain
any byte without affecting text decoding. Number
values and interpolation debug text have their own side tables. Identifiers with the same spelling
share a four-byte index into the parse's name table. Each distinct spelling is a `Name` backed
by `CharStr`, which stores short names inline. The parser copies unescaped literal text directly
from the source into the text buffer. A string literal expression stores both its list of tokens
and the range of its decoded text. The text offset and a two-byte length fit beside the list in
the row. Lengths of 65,535 bytes or more use a full range in the extra-data column. A semantic
check can borrow the decoded text directly, including for implicitly concatenated literals.

Strings, bytes, f-strings, and t-strings all store their parts as lists of row IDs at the same
offset. The generator checks this layout and emits one accessor for the four expression kinds.
A formatter can iterate the parts in either direction and read each token's kind and flags.

## From source to rows

The parser creates a row when it finishes a grammar production. In `left + right`, it emits the
two names first and passes their slots to the binary expression's constructor. The parser's
temporary `Node<T>` carries the kind, source range and a `BuildId<T>`. A build ID holds the same
one-based ID as a handle in a plain `u32`, so emitting a node and writing its ID into its
parent need no nonzero check. The finished tree and consumers that retain a node use `AstNodeId<T>`.

Nested lists of the same kind share a scratch buffer of four-byte slots. Each
list records the buffer length on entry. When the list closes, the parser copies that suffix
into the extra-data column and truncates the buffer to the recorded length. The enclosing list
keeps its handles.

During error recovery, a pattern can occupy a position that requires an expression. The parser
emits rows for the replacement syntax and reuses the handles of expressions the pattern already
contains. A nested recovery can therefore refer back to a literal's original row and text.

Some Python constructs require speculative parsing. For example, `match` can start a statement
or name a variable. A parser checkpoint records the ends of the columns alongside the token
position. Rewinding discards the abandoned rows and their variable-length data. The same builder
also spans all cells of a notebook. Each cell appends its top-level statement handles, and the
parser emits a single module for the concatenated source.

After parsing, the tree owns the rows and side tables. The builder releases its scratch buffers
and name lookup table. Returning a child view borrows the same tree. Freeing the tree releases
the rows and side tables without recursively dropping statements or expressions.

## Traversal

Source order and evaluation order differ in Python. In `f(k=x, *ys)`, the keyword precedes the
starred argument in the source, but the positional arguments are evaluated before the keywords.
The source-order visitor follows the ordering recorded when parsing the arguments. The evaluation-order visitor reads the
positional list first. Both walk the same rows.

`node.children()` yields the immediate children in forward source order. It keeps up to two
individual child IDs in the iterator. For a module it borrows the statement IDs
from the extra buffer. For nodes with a mix of child fields it opens each list as it reaches
that field. Calling `next()` takes one traversal step without counting or copying the remaining
children.

`node.descendants()` starts with the node itself and walks its subtree in preorder.
It pushes the remaining siblings onto a stack of child IDs in reverse source order.
A visitor can prune subtrees and maintain an ancestor stack on entry and exit. Rows store
links to their children and carry no parent pointer.

Ruff's linter records parents and branches during semantic traversal. For a module node this
table stores a typed statement or expression handle. An ancestor search can skip expression
handles before looking up a statement in the module. A string annotation has a separate arena,
so its semantic entry points to a view that retains the annotation's context.

The linter looks up a name’s binding by its starting offset in the source.

## Node identity in ty

When ty caches a parsed module, it walks the reachable syntax in source order, skipping the
module wrapper. It stores a node index for each visited arena row and the row ID for each
module index. ty can use the same index to resolve a reference after evicting and reparsing
the file. String annotations use an interval of indices reserved by their parent literal.

## Building replacement syntax

A lint fix can build replacement syntax in a temporary arena. For example, the fix for
`delattr(obj, "name")` copies the expression `obj` with `Builder::copy_node`. The copy keeps
its source ranges and owns its literal data. Its handle belongs to the new arena. The fix adds
an attribute and a `del` statement with `Builder::add`, then passes the statement view from
`Builder::get` to the code generator.

The parser's `syntax`, `suite`, and `expr` accessors all borrow the parsed tree.
Most lint fixes can edit source ranges directly. A fix can also print an existing expression
with `Generator::expr`. To change selected nodes while copying their enclosing expression,
pass a `Rewriter` to `Builder::rewrite_node`. Both copied and newly constructed nodes belong to
the destination arena. If the result must outlive the builder, `Builder::finish_tree` returns a
tree that owns those nodes.

For example, `UP013` checks whether a functional `TypedDict` declaration can use class syntax.
It validates field names and borrows their annotations. When the rule offers an edit, it copies
the base class, keywords, and field annotations into the replacement class definition.
[Rule implementation](../ruff_linter/src/rules/pyupgrade/rules/convert_typed_dict_functional_to_class.rs) ·
[Class replacement](../ruff_linter/src/rules/pyupgrade/fixes.rs)

## Measuring performance

The parser benchmark times construction using `parse_module` and drops its results outside the
timed loop. Parsing and immediately dropping a file also measures teardown, which changes when
one tree owns the allocations. A traversal benchmark can reuse a single tree and check the same
node kinds and source ranges in the same order. [Parser benchmarks](../ruff_benchmark/benches/parser.rs)

Peak heap during construction and memory retained by ty measure different parts of the tree's
lifetime. Growing the row array can copy its existing rows. Compacting a finished tree reclaims
spare capacity before ty caches it, after those copies have already happened. Ruff can lint or
format a short-lived tree directly. End-to-end measurements include work after parsing, such as
lint rule evaluation and type inference. [Ruff benchmarks](../ruff_benchmark/benches/linter.rs) ·
[ty benchmarks](../ruff_benchmark/benches/ty.rs)
