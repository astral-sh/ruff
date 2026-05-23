# `f-string` (`UP032`)

Soundness fixes for [#15874](https://github.com/astral-sh/ruff/issues/15874): a `str.format` call is
only converted when the f-string behaves like the original. Conversions that can change behavior
become unsafe fixes; conversions that Python rejects, or that an f-string reads differently, are
skipped.

```toml
lint.select = ["UP032"]
```

## Dropped arguments emit an unsafe fix

An f-string only evaluates the arguments it interpolates, so dropping one that can change behavior is
unsafe.

<!-- snapshot-diagnostics -->

A walrus referenced only by the dropped argument:

```py
"{1}".format(x := 1, x)  # error: [f-string]
```

A dropped argument that calls a function or can raise:

```py
"a".format(foo())  # error: [f-string]
"1".format(1 / 0)  # error: [f-string]
```

## Format-time accessors with side-effecting arguments emit an unsafe fix

`str.format` evaluates every argument before formatting; an f-string interleaves the two. With a
`{[k]}` or `{.attr}` accessor the order is observable, so the fix is unsafe:

<!-- snapshot-diagnostics -->

```py
"{[x]} {}".format(d, len(d))  # error: [f-string]
"{0.attr} {1}".format(obj, side_effect())  # error: [f-string]
```

## Arguments that need parentheses

The argument is wrapped when its bare form would be misparsed: a lambda's colon starts a format spec,
and a leading `{` reads as an escaped brace.

<!-- snapshot-diagnostics -->

```py
"{}".format(lambda: 1)  # error: [f-string]
"{}".format({} | {})  # error: [f-string]
```

## Skipped: rejected by Python or read differently

These parse as `str.format` calls but have no behavior-preserving f-string, so nothing is emitted.

A signed field index (`str.format` raises `KeyError`):

```py
"{+0}".format(0)
```

A non-identifier attribute name (`"{. a}"` calls `getattr(x, " a")`, but `f"{x. a}"` would read
`x.a`):

```py
"{. a}".format(x)
```

A string index whose quote collides with the f-string quote:

```py
"{[']}".format(x)
```

An unknown conversion specifier (`str.format` raises `ValueError`):

```py
"{!?}".format(0)
```

## Drive-by: dropping a leading empty literal leaves no orphan whitespace

```py
"" "{}".format(x)  # error: [f-string]
"a" "" "{}".format(x)  # error: [f-string]
```
