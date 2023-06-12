# Rust Python Formatter

The goal of out formatter is to be compatible to black except for rare edge cases (mostly involving
comment placement).

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

```python
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

The generic comment formatting in `FormatNodeRule` handles comments correctly for most nodes, e.g.
preceding and end-of-line comments depending on the node range. Sometimes however, you may have
dangling comments that are not before or after a node but inside of it, e.g.

```python
[
    # here we use an empty list
]
```

Here, you have to call `dangling_comments` manually and stubbing out `fmt_dangling_comments` in list
formatting.

```rust
impl FormatNodeRule<ExprList> for FormatExprList {
    fn fmt_fields(&self, item: &ExprList, f: &mut PyFormatter) -> FormatResult<()> {
        // ...

        write!(
            f,
            [group(&format_args![
                text("["),
                dangling_comments(dangling),
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

Comments are categorized into `Leading`, `Trailing` and `Dangling`, you can override this in
`place_comment`.

## Development notes

Handling parentheses and comments are two major challenges in a python formatter.

We have copied the majority of tests over from black and use [insta](https://insta.rs/docs/cli/) for
snapshot testing with the diff between ruff and black, black output and ruff output. We put
additional test cases in `resources/test/fixtures/ruff`.

The full ruff test suite is slow, `cargo test -p ruff_python_formatter` is a lot faster.

There is a `ruff_python_formatter` binary that avoid building and linking the main `ruff` crate.

You can use `scratch.py` as a playground, e.g.
`cargo run --bin ruff_python_formatter -- --emit stdout scratch.py`, which additional `--print-ir`
and `--print-comments` options.

## The orphan rules and out trait structure

For the formatter, we would like to implement `Format` from the rust_formatter crate for all AST
nodes, defined in the rustpython_parser crate. This violates rust's orphan rules. We therefore
generate in `generate.py` a newtype for each AST node with implementations of `FormatNodeRule`,
`FormatRule`, `AsFormat` and `IntoFormat` on it.

![excalidraw showing the relationships between the different types](orphan_rules_in_the_formatter.svg)
