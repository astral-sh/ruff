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

An ID stores the node's physical slot as a `NonZeroU32`. The builder reserves slot 0 for
an absent child, so `Option<AstNodeId<T>>` occupies the same four bytes as `AstNodeId<T>`.
Each parsed arena, including a separately parsed string annotation, reserves one unused
row and source range (24 bytes total). Allocation indices are zero-based. The first node occupies physical slot 1,
has `id.index() == 0`, and is returned by `ast.row(0)`. `ast.len()` counts stored nodes
and `ast.any_node(predicate)` scans them, including rows retained by parser recovery.
Both skip the reserved slot. Use `root.descendants()` to walk the reachable tree in source order.

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

Use `SyntaxNode` for an untyped row. Calling `node.cast::<Stmt>()` returns a statement
when the row contains one. The source-order visitor receives an `AnyNode` on entry and exit
so it can match on the concrete variant. It can walk both parsed and constructed trees.

## Row layout

Each node occupies a 16-byte row and an eight-byte source range. The first 15 bytes of the
row hold naturally aligned integer fields. The last byte holds the node's kind. Rows occupy
the first part of a shared allocation and source ranges occupy the second. The parser appends
both at the same slot with one capacity check. A visitor can read the kind and fields directly
from its borrowed row. To get the range, it uses the row's slot in the range column.

The schema declares each node's fields. The generator assigns their physical locations and
generates constructors, typed accessors, and traversal from the same declarations. A binary
expression fits both operand IDs and its operator inline. When the fields need more space, one
aligned four-byte field points into extra data. The remaining inline fields can occupy up to
eleven bytes. Each spilled field takes one or two four-byte words.
[Schema](ast.toml) · [Generator](generate.py)

Only the row constructor writes the kind. The union and its members are private. Construction
initializes every field byte, and generated setters write individual integers. For example, when
an assignment turns a name into a store target, the parser updates the name's expression context
without touching its identifier or kind. Assigning a whole union member could overwrite the
kind with trailing padding. Compile-time checks verify every field's offset and that it ends
before the kind. [Row implementation](src/flat/row.rs)

Lists store a starting slot and an element count. A `NodeList<T>` reads a row ID from one
four-byte word for each element and returns a `T::Borrowed` with the same tree borrow. A `Suite` is
a `NodeList<kind::Stmt>`. An empty list consumes no element slots in the extra-data column.
Call arguments and class pattern arguments store child IDs in source order. The child’s
kind distinguishes a positional argument from a keyword. The full list has a stored length.
Counting a filtered subset, such as the keyword arguments, visits the slots.

A `Parameters` row stores the starting slot and count of its source-ordered parameter IDs.
An adjacent list of role words distinguishes positional-only, ordinary, variadic and
keyword-only parameters. A role-only query can read those words without resolving child nodes.

A dictionary display stores a list of `DictItem` IDs. Each item holds its optional key and its
value. In `{**mapping}` the entry starts at `**` and its only child is `mapping`. Every AST list
element occupies one four-byte word. Side-table offsets have their own encoding and can start at
zero. UTF-8 text and arbitrary byte strings have separate buffers, so a bytes literal can contain
any byte without affecting text decoding. Number
values and interpolation debug text have their own side tables. Identifiers with the same spelling
share a four-byte index into the parse's name table. Each distinct spelling is a `Name` backed
by `CharStr`, which stores short names inline. The parser emits unescaped literals from the source and
decodes escaped literals before emitting them into the text or bytes buffer.
A string literal expression stores both its list of tokens and the range of its decoded text.
The text offset and a two-byte length fit beside the list in the row. Lengths of 65,535 bytes
or more use a full range in the extra-data column. A semantic
check can borrow the decoded text directly, including for implicitly concatenated literals.

Strings, bytes, f-strings, and t-strings all store their parts as lists of row IDs at the same
offset. The generator checks this layout and emits one accessor for the four expression kinds.
A formatter can iterate the parts in either direction and read each token's kind and flags.

## From source to rows

The parser creates a row when it finishes a grammar production. In `left + right`, it emits the
two names first and passes their slots to the binary expression's constructor. The parser's
temporary `Node<T>` carries the kind, source range and a `BuildId<T>`. A build ID holds the same
physical slot as a handle in a plain `u32`, so emitting a node and writing its slot into its
parent need no nonzero check. The finished tree and consumers that retain a node use `AstNodeId<T>`.

Before appending a parent such as `left + right`, the builder checks that both operand
IDs resolve to rows in the destination arena. It also checks list references and interned
name IDs when storing them. Children precede their parents, and a rewind restores the
column lengths together. A getter on a borrowed row can therefore follow a stored ID
without repeating the bounds check. `Ast::view` checks the slot and kind when a caller
supplies an ID. [Builder invariants](src/flat/mod.rs)

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

The initial allocation includes one row for every 18 source bytes, up to 8 KiB of rows and ranges.
When the rows fill, the builder estimates how many more it needs
from the number of rows and source bytes consumed so far. It also reserves extra-data words in
proportion to their use. The estimate allows the remaining source to be denser than the prefix,
as happens when a module starts with a long docstring. A dense prefix can also precede a long
comment. The builder caps speculative growth at one MiB per buffer. A full row buffer still
grows by at least one quarter of its length to keep repeated reallocations amortized.

After parsing, the tree owns the rows and side tables. The builder releases its scratch buffers
and name lookup table. Returning a child view borrows the same tree. Freeing the tree releases
the rows and side tables without recursively dropping statements or expressions.

## Traversal

Source order and evaluation order differ in Python. In `f(k=x, *ys)`, the keyword precedes the
starred argument in the source, but the positional arguments are evaluated before the keywords.
The source-order visitor walks the stored list directly. The evaluation-order visitor visits
the positional arguments before the keywords in that list.

`node.children()` yields the immediate children in forward source order. For a binary expression
it borrows the two adjacent operand IDs from the row. For a module it borrows the statement IDs
from the extra buffer. For nodes with a mix of child fields it opens each list as it reaches
that field. Calling `next()` takes one traversal step without counting or copying the remaining
children.

`node.descendants()` starts with the node itself and walks its subtree in preorder.
When it enters a list, it keeps a
borrowed slice of the remaining child handles. The stack grows with nesting depth, so a
search for the first statement in a long module can stop without copying the rest of the suite.
A visitor can prune subtrees and maintain an ancestor stack on entry and exit. Rows store
links to their children and carry no parent pointer.

Ruff's linter records a parent and branch for each statement or expression it visits.
Each entry holds a borrowed `SyntaxNode`, which carries its row and owning AST. This lets the
same table hold a function definition and expressions from a separately parsed string annotation.

For `return value`, the linter records the binding of `value` in a two-byte slot indexed by
the expression's row. It allocates this table on the first resolved name. Parser recovery
and reparsed annotations can give the same source occurrence multiple rows. In these cases,
or when a binding ID exceeds the two-byte slot, the linter uses a map keyed by source offset.

## Node identity

ty keeps a typed arena handle in each `AstNodeRef<T>`. Parsing the same source with the same
options emits the same rows, so a reference still resolves after ty evicts and reparses the
module. Resolving a handle reads the row directly. Allocation order can differ from source order.
In `a.b()[i]`, for example, the parser emits the attribute and call before the enclosing subscript.

Before caching a module, ty calls `index_module()` to use each row's zero-based allocation index
as its node key. This takes constant time because the rows share one base offset. ty then calls
`shrink_to_fit()` to release spare capacity in the parse buffers.

A separately parsed string annotation has its own short-lived arena. ty calls
`index_string_annotation()` to give its nodes keys in the range reserved by the containing
string literal. A node's key is the start of the range plus its zero-based allocation index.
If auxiliary rows would put an annotation over its 256-key limit (eight for a nested annotation),
ty keeps a sorted list of the rows reached by source-order traversal. Each row's position in the
list gives its offset within the reserved range.

The IDE finds the enclosing annotation by searching annotation roots sorted by source position.
To identify an unassigned call such as `return type("C", (), {})`, ty records its row relative to
the first expression row in its scope. Rows inserted before the scope shift both by the same
amount. A diagnostic can recover the call's current range even after whitespace edits.

For a scoped use, ty records which rows have a use ID in a bitmap spanning up to 64
node indices. Counting the set bits preceding a row gives its offset into the page’s
packed use IDs. For a definition, ty looks up its node key in a sorted table.
[Scoped uses](../ty_python_core/src/ast_ids.rs) · [Definitions](../ty_python_core/src/lib.rs)

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
