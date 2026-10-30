# Python AST storage

A parsed Python file owns one syntax tree. Its module, statements, expressions, identifiers, and
literal parts each have a row in the same table. Child fields contain four-byte row IDs. For
example, the operands of `left + right` are two IDs in the binary expression's payload. Reading
either operand requires the table that owns those IDs. Ruff and ty share the same parser and
tree representation.
[SWC describes the design that inspired this layout.](https://next.swc.rs/architecture/flat-ast)

## Handles and views

A `Handle<Expr>` can refer to any expression. A `Handle<ExprBinOp>` refers to a binary expression.
Both occupy four bytes. A handle is local to a particular parse, even if a later parse of the same
file produces identical text and row numbers.

`Parsed::root` returns a `View` that holds a reference to the node's row followed by a reference
to its owning `Ast`. Copying a view copies those references. A field accessor resolves a child ID in
the same tree and returns a view with the same lifetime.

Accept a `View<Expr>` when reading a parsed expression or a `View<Stmt>` when reading a statement.
A concrete view such as `View<ExprCall>` converts to `View<Expr>` with `.into()` and still refers
to the same row. Call `classify()` when you need to match its variant. The example below matches a
binary expression, saves the left operand's handle, and looks it up again while the parse is alive.

```rust
use ruff_python_ast::flat::ExprKind;
use ruff_python_parser::{ParseError, parse_expression};
use ruff_text_size::{Ranged, TextRange, TextSize};

fn main() -> Result<(), ParseError> {
    let parsed = parse_expression("left + right")?;
    let ExprKind::BinOp(binary) = parsed.root().body().classify() else {
        panic!("expected a binary expression");
    };
    let left = binary.left();
    assert_eq!(left.range(), TextRange::up_to(TextSize::new(4)));
    let saved = left.handle();
    assert_eq!(parsed.ast().view(saved), Some(left));
    Ok(())
}
```

`expr.as_call_expr()` checks the kind and returns a typed call view. Reading `call.func()`
then resolves only the callee. The generated `fields()` method is useful for destructuring a
whole node. It constructs all of that node's borrowed fields, including fields a pattern
ignores with `..`.

Visitors that accept any parsed node use `flat::AnyNodeRef`, an alias for `View<Any>`.
`node.cast::<Stmt>()` returns a statement view when the row contains a statement. Walks over
independently owned syntax use `ruff_python_ast::AnyNodeRef`, whose variants borrow the
corresponding Rust structs.

When the linter dispatches several rules on a call, it can resolve the callee and arguments
once and pass each rule a reference to the same `fields::ExprCall`. Rules that need only one
field can use its generated accessor directly.

## Row layout

Each row occupies 24 bytes, with a 16-byte payload followed by an eight-byte source span. The payload
holds up to 15 bytes of fields and a one-byte node kind. A visitor can read both the kind and the
span from the row it already borrows. The parser appends each completed node in one operation.

The schema declares each node's fields. The generator assigns their physical locations and
generates constructors, typed accessors, and traversal from the same declarations. A binary
expression fits both operand IDs and its operator inline. When the fields need more than 15
bytes, the first four bytes hold an offset into extra data and the smallest fields occupy up to
eleven bytes beside it. Each spilled field takes one or two four-byte words. The generated read
and write offsets are checked against the payload size at compile time.
[Schema](ast.toml) · [Generator](generate.py)

Lists store a starting slot and an element count. A `NodeList<T>` reads a row ID from one
four-byte word for each element and returns a `View<T>` with the same tree borrow. A `Suite` is
a `NodeList<Stmt>`. An empty list consumes no element slots in the extra-data column.
Call and pattern arguments share a boundary between their positional and keyword lists. The
row stores that boundary and both lengths in twelve bytes. The parser appends the lists next
to each other. A builder that supplies two nonadjacent, nonempty lists copies them into
consecutive slots.

An `ExprCompare` row shares a boundary between its operators and operands. For a single
comparison such as `a == b`, a spare byte holds the operator. The `as_single` accessor can
therefore reject a chain such as `a < b < c` before reading either list.

A `Parameters` row stores the positional-or-keyword list inline. For a signature such as
`def f(x, y=0)`, this is the whole parameter list. The row allocates extra-data slots for
the other four fields only when it has positional-only, variadic, or keyword-only parameters.

Dictionary items use two words per entry, one for the optional key and one for the value. An
optional child uses the reserved row ID `u32::MAX`. UTF-8 text and arbitrary byte strings have
separate buffers, so a bytes literal can contain any byte without affecting text decoding. Number
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
two names first and passes their handles to the binary expression's constructor. A parser result
also carries its kind and range while the grammar still needs them. Only the handle goes into the
parent payload. Nested lists of the same kind share a scratch buffer of four-byte handles. Each
list records the buffer length on entry. When the list closes, the parser copies that suffix
into the extra-data column and truncates the buffer to the recorded length. The enclosing list
keeps its handles.

Some Python constructs require speculative parsing. For example, `match` can start a statement
or name a variable. A parser checkpoint records the ends of the columns alongside the token
position. Rewinding discards the abandoned rows and their variable-length data. The same builder
also spans all cells of a notebook. Each cell appends its top-level statement handles, and the
parser emits a single module for the concatenated source.

For modules up to 4 KiB, the builder reserves row space from the source length. Larger modules
start with at most 128 rows. When the rows fill, the builder estimates how many more it needs
from the number of rows and source bytes consumed so far. It also reserves extra-data words in
proportion to their use. The estimate allows the remaining source to be denser than the prefix,
as happens when a module starts with a long docstring.

A function's body occupies a contiguous interval of rows before the function's own row. The
parser records the length of that interval in two unused bytes of the function row. The arena
also records the position of its last `yield` or `yield from`. If that position precedes the
body, or the arena has no yields, Ruff can rule out a generator without reading any body rows.
If the last yield comes later, Ruff scans the body's row tags. A match can belong to a nested
scope, so Ruff follows child handles to decide whether the function itself yields.

Without a yield, the return collector can stop scanning the rows at the first `return` and visit
only statements to collect returns in source order. Functions with more than 65,535 body rows
and trees built from owned nodes have no recorded body interval. They use a scope-aware walk
when the arena contains a yield.

After parsing, the tree owns the rows and side tables. The builder releases its scratch buffers
and name lookup table. Returning a child view borrows the same tree. Freeing the tree releases
the rows and side tables without recursively dropping statements or expressions.

## Traversal

Source order and evaluation order differ in Python. In `f(k=x, *ys)`, the keyword precedes the
starred argument in the source, but the positional arguments are evaluated before the keywords.
The source-order visitor merges their two lists by position. The evaluation-order visitor reads the
positional list first. Both walk the same rows.

`View::descendants()` yields the starting view and then its descendants in source order. A
search can stop at the first match with `find` or `any`. The iterator keeps a stack of pending
child handles, so it can walk a single subtree without scanning the rest of the file. A visitor
can prune subtrees and maintain an ancestor stack on entry and exit. Rows store links to their
children and carry no parent pointer.

Ruff's linter records parents and branches during semantic traversal. For a module node this
table stores a typed statement or expression handle. An ancestor search can skip expression
handles before looking up a statement in the module. A string annotation has a separate arena,
so its semantic entry points to a view that retains the annotation's context.

The linter indexes a module's name bindings by row. Names parsed inside string tokens use a
separate table keyed by source offset because two names in a concatenated string annotation can
share an offset. If another tree overlaps a module name's offset, the linter looks up names from
both trees in a shared offset table.

## Source-order identity

ty assigns a logical `NodeIndex` in source order when it indexes a parsed module. It shrinks the
row and side-table buffers before caching the module and stores both directions of the mapping.
An `AstNodeRef<T>` can keep the logical index between queries and resolve it to a typed view
for the duration of a module borrow. String annotations reserve logical indices relative to their
containing string. This gives nodes in a separately parsed annotation distinct identities even
when their ranges refer to the same source token.

ty uses those indices to look up scoped use IDs and definitions. Each block of 64 module indices
stores a bitmap of the rows with a value and the starting offset of those values. Looking up a
row counts the set bits before it and adds the block offset. Only rows with a value occupy space
in the values array. Annotation indices use separate sparse tables.
[Node table](../ty_python_core/src/frozen.rs)

## Owned syntax

The parser's `syntax`, `suite`, and `expr` accessors borrow rows from the same tree. A
transformation that needs to change a subtree can call `View::to_owned_node`. The result owns its
descendants and literal data and can outlive the parse. The consuming `into_syntax`,
`into_suite`, and `into_expr` methods provide the same ownership boundary for a complete parse.

Rebuilding a subtree copies each node's `NodeIndex`. A lint fix that creates a new node uses
`NodeIndex::NONE` because the new node has no row in the parsed module.

Most lint fixes work with source ranges. For example, replacing an operator needs its range and
the replacement text. A fix can also generate source from an existing expression with
`Generator::expr_view`. Use `to_owned_node` when the replacement needs independent syntax.
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
