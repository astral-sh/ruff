# `f-string` (`UP032`)

These tests cover the soundness fixes for
[#15874](https://github.com/astral-sh/ruff/issues/15874): a `str.format` call is only converted when
the resulting f-string behaves like the original. Conversions that may change runtime behavior are
downgraded to an unsafe fix, and conversions that Python rejects (or that an f-string would interpret
differently) are skipped entirely.

```toml
lint.select = ["UP032"]
```

## Dropped arguments emit an unsafe fix

`str.format` evaluates every argument, while an f-string only evaluates the ones it interpolates.
When an unused argument can change behavior, the fix is downgraded to unsafe.

<!-- snapshot-diagnostics -->

A walrus binding referenced only by the dropped argument would raise `NameError` after conversion:

```py
"{1}".format(x := 1, x)  # error: [f-string]
```

A dropped argument with side effects — a call, or arithmetic that may raise:

```py
"a".format(foo())  # error: [f-string]
"1".format(1 / 0)  # error: [f-string]
```

## Format-time accessors with side-effecting arguments emit an unsafe fix

`str.format` evaluates all arguments before formatting any of them, whereas an f-string interleaves
evaluation and formatting. With a field accessor like `{[k]}` or `{.attr}`, that reordering is
observable — for example, a `defaultdict` whose key is inserted by a later `len()` argument:

<!-- snapshot-diagnostics -->

```py
"{[x]} {}".format(d, len(d))  # error: [f-string]
"{0.attr} {1}".format(obj, side_effect())  # error: [f-string]
```

## Arguments that need parentheses

The fix wraps an argument whose unparenthesized form would be misparsed inside the f-string: a
lambda's colon would start a format spec, and a leading `{` would read as an escaped brace.

<!-- snapshot-diagnostics -->

```py
"{}".format(lambda: 1)  # error: [f-string]
"{}".format({} | {})  # error: [f-string]
```

## Skipped: rejected by Python or interpreted differently

These all parse as `str.format` calls, but have no behavior-preserving f-string equivalent, so no
diagnostic is emitted.

A signed field index — `str.format` raises `KeyError`, even though it parses as an integer:

```py
"{+0}".format(0)
```

An attribute name that isn't an identifier — `"{. a}"` resolves via `getattr(x, " a")`, but
`f"{x. a}"` would read `x.a`:

```py
"{. a}".format(x)
```

A string index whose quote collides with the only quote available inside the f-string:

```py
"{[']}".format(x)
```

An unknown conversion specifier — Python raises `ValueError` at runtime:

```py
"{!?}".format(0)
```

## Drive-by: a dropped leading empty literal leaves no orphan whitespace

When an implicitly-concatenated string starts with an empty literal, dropping it must not leave the
inter-literal whitespace behind (which previously produced invalid indentation):

<!-- snapshot-diagnostics -->

```py
"" "{}".format(x)  # error: [f-string]
"a" "" "{}".format(x)  # error: [f-string]
```
