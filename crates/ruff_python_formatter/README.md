# Rust Python Formatter

The goal of our formatter is to be compatible with Black except for rare edge cases (mostly
involving comment placement).

## Implementing a node

Formatting each node follows roughly the same structure. We start with a `Format{{Node}}` struct
that implements Default (and `AsFormat`/`IntoFormat` impls in `generated.rs`, see orphan rules below).

```rust
#[derive(Default)]
pub struct FormatStmtReturn;
```

We implement `FormatNodeRule<{{Node}}> for Format{{Node}}`. Inside, we destructure the item to make
sure we're not missing any field. If we want to write multiple items, we use an efficient `write!`
call, for single items `.format().fmt(f)` or `.fmt(f)` is sufficient.

```rust
impl FormatNodeRule<StmtReturn> for FormatStmtReturn {
    fn fmt_fields(&self, item: &StmtReturn, f: &mut PyFormatter) -> FormatResult<()> {
        // Here we destructure item and make sure each field is listed.
        // We generally don't need range is it's underscore-ignored
        let StmtReturn { range: _, value } = item;
        // Implement some formatting logic, in this case no space (and no value) after a return with
        // no value
        if let Some(value) = value {
            write!(
                f,
                [
                    text("return"),
                    // There are multiple different space and newline types (e.g.
                    // `soft_line_break_or_space()`, check the builders module), this one will
                    // always be translate to a normal ascii whitespace character
                    space(),
                    // `return a, b` is valid, but if it wraps we'd need parentheses.
                    // This is different from `(a, b).count(1)` where the parentheses around the
                    // tuple are mandatory
                    value.format().with_options(Parenthesize::IfBreaks)
                ]
            )
        } else {
            text("return").fmt(f)
        }
    }
}
```

Check the `builders` module for the primitives that you can use.

If something such as list or a tuple can break into multiple lines if it is too long for a single
line, wrap it into a `group`. Ignoring comments, we could format a tuple with two items like this:

```rust
write!(
    f,
    [group(&format_args![
        text("("),
        soft_block_indent(&format_args![
            item1.format()
            text(","),
            soft_line_break_or_space(),
            item2.format(),
            if_group_breaks(&text(","))
        ]),
        text(")")
    ])]
)
```

If everything fits on a single line, the group doesn't break and we get something like `("a", "b")`.
If it doesn't, we get something like

```Python
(
    "a",
    "b",
)
```

For a list of expression, you don't need to format it manually but can use the `JoinBuilder` util,
accessible through `.join_with`. Finish will write to the formatter internally.

```rust
f.join_with(&format_args!(text(","), soft_line_break_or_space()))
    .entries(self.elts.iter().formatted())
    .finish()?;
// Here we need a trailing comma on the last entry of an expanded group since we have more
// than one element
write!(f, [if_group_breaks(&text(","))])
```

If you need avoid second mutable borrows with a builder, you can use `format_with(|f| { ... })` as
a formattable element similar to `text()` or `group()`.

## Comments

Comments can either be own line or end-of-line and can be marked as `Leading`, `Trailing` and `Dangling`.

```python
# Leading comment (always own line)
print("hello world")  # Trailing comment (end-of-line)
# Trailing comment (own line)
```

Comments are automatically attached as `Leading` or `Trailing` to a node close to them, or `Dangling`
if there are only tokens and no nodes surrounding it. Categorization is automatic but sometimes
needs to be overridden in
[`place_comment`](https://github.com/astral-sh/ruff/blob/be11cae619d5a24adb4da34e64d3c5f270f9727b/crates/ruff_python_formatter/src/comments/placement.rs#L13)
in `placement.rs`, which this section is about.

```Python
[
    # This needs to be handled as a dangling comment
]
```

Here, the comment is dangling because it is preceded by `[`, which is a non-trivia token but not a
node, and  followed by `]`, which is also a non-trivia token but not a node. In the `FormatExprList`
implementation, we have to call `dangling_comments` manually and stub out the
`fmt_dangling_comments` default from `FormatNodeRule`.

```rust
impl FormatNodeRule<ExprList> for FormatExprList {
    fn fmt_fields(&self, item: &ExprList, f: &mut PyFormatter) -> FormatResult<()> {
        // ...

        write!(
            f,
            [group(&format_args![
                text("["),
                dangling_comments(dangling), // Gets all the comments marked as dangling for the node
                soft_block_indent(&items),
                text("]")
            ])]
        )
    }

    fn fmt_dangling_comments(&self, _node: &ExprList, _f: &mut PyFormatter) -> FormatResult<()> {
        // Handled as part of `fmt_fields`
        Ok(())
    }
}
```

A related common challenge is that we want to attach comments to tokens (think keywords and
syntactically meaningful characters such as `:`) that have no node on their own. A slightly
simplified version of the `while` node in our AST looks like the following:

```rust
pub struct StmtWhile {
    pub range: TextRange,
    pub test: Box<Expr<TextRange>>,
    pub body: Vec<Stmt<TextRange>>,
    pub orelse: Vec<Stmt<TextRange>>,
}
```

That means in

```python
while True:  # Trailing condition comment
    if f():
        break
    # trailing while comment
# leading else comment
else:
    print("while-else")
```

the `else` has no node, we're just getting the statements in its body.

The preceding token of the leading else comment is the `break`, which has a node, the following
token is the `else`, which lacks a node, so by default the comment would be marked as trailing
the `break` and wrongly formatted as such. We can identify these cases by looking for comments
between two bodies that have the same indentation level as the keyword, e.g. in our case the
leading else comment is inside the `while` node (which spans the entire snippet) and on the same
level as the `else`. We identify those case in
[`handle_in_between_bodies_own_line_comment`](https://github.com/astral-sh/ruff/blob/be11cae619d5a24adb4da34e64d3c5f270f9727b/crates/ruff_python_formatter/src/comments/placement.rs#L196)
and mark them as dangling for manual formatting later. Similarly, we find and mark comment after
the colon(s) in
[`handle_trailing_end_of_line_condition_comment`](https://github.com/astral-sh/ruff/blob/main/crates/ruff_python_formatter/src/comments/placement.rs#L518)
.

The comments don't carry any extra information such as why we marked the comment as trailing,
instead they are sorted into one list of leading, one list of trailing and one list of dangling
comments per node. In `FormatStmtWhile`, we can have multiple types of dangling comments, so we
have to split the dangling list into after-colon-comments, before-else-comments, etc. by some
element separating them (e.g. all comments trailing the colon come before the first statement in
the body) and manually insert them in the right position.

A simplified implementation with only those two kinds of comments:

```rust
fn fmt_fields(&self, item: &StmtWhile, f: &mut PyFormatter) -> FormatResult<()> {

    // ...

    // See FormatStmtWhile for the real, more complex implementation
    let first_while_body_stmt = item.body.first().unwrap().end();
    let trailing_condition_comments_end =
        dangling_comments.partition_point(|comment| comment.slice().end() < first_while_body_stmt);
    let (trailing_condition_comments, or_else_comments) =
        dangling_comments.split_at(trailing_condition_comments_end);

    write!(
        f,
        [
            text("while"),
            space(),
            test.format(),
            text(":"),
            trailing_comments(trailing_condition_comments),
            block_indent(&body.format())
            leading_comments(or_else_comments),
            text("else:"),
            block_indent(&orelse.format())
        ]
    )?;
}
```

## Development notes

Handling parentheses and comments are two major challenges in a Python formatter.

We have copied the majority of tests over from Black and use [insta](https://insta.rs/docs/cli/) for
snapshot testing with the diff between Ruff and Black, Black output and Ruff output. We put
additional test cases in `resources/test/fixtures/ruff`.

The full Ruff test suite is slow, `cargo test -p ruff_python_formatter` is a lot faster.

There is a `ruff_python_formatter` binary that avoid building and linking the main `ruff` crate.

You can use `scratch.py` as a playground, e.g.
`cargo run --bin ruff_python_formatter -- --emit stdout scratch.py`, which additional `--print-ir`
and `--print-comments` options.

The origin of Ruff's formatter is the [Rome formatter](https://github.com/rome/tools/tree/main/crates/rome_json_formatter),
e.g. the ruff_formatter crate is forked from the [rome_formatter crate](https://github.com/rome/tools/tree/main/crates/rome_formatter).
The Rome repository can be a helpful reference when implementing something in the Ruff formatter

### Checking formatter stability and panics

There are tree common problems with the formatter: The second formatting pass looks different than
the first (formatter instability or lack of idempotency), we print invalid syntax (e.g. missing
parentheses around multiline expressions) and panics (mostly in debug assertions). We test for all
of these using the `check-formatter-stability` subcommand in `ruff_dev`

The easiest is to check CPython:

```shell
git clone --branch 3.10 https://github.com/python/cpython.git crates/ruff/resources/test/cpython
cargo run --bin ruff_dev -- check-formatter-stability crates/ruff/resources/test/cpython
```

It is also possible large number of repositories using ruff. This dataset is large (~60GB), so we
only do this occasionally:

```shell
curl https://raw.githubusercontent.com/akx/ruff-usage-aggregate/master/data/known-github-tomls.jsonl > github_search.jsonl
python scripts/check_ecosystem.py --checkouts target/checkouts --projects github_search.jsonl -v $(which true) $(which true)
cargo run --bin ruff_dev -- check-formatter-stability --multi-project target/checkouts
```

## The orphan rules and trait structure

For the formatter, we would like to implement `Format` from the rust_formatter crate for all AST
nodes, defined in the rustpython_parser crate. This violates Rust's orphan rules. We therefore
generate in `generate.py` a newtype for each AST node with implementations of `FormatNodeRule`,
`FormatRule`, `AsFormat` and `IntoFormat` on it.

![excalidraw showing the relationships between the different types](orphan_rules_in_the_formatter.svg)
