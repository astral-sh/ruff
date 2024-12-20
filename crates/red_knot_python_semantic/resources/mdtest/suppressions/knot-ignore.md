# Suppressing errors with `knot: ignore`

Type check errors can be suppressed by a `knot: ignore` comment on the same line as the violation.

## Simple `knot: ignore`

```py
a = 4 + test  # knot: ignore
```

## Suppressing a specific code

```py
a = 4 + test  # knot: ignore[unresolved-reference]
```

## Useless suppression

TODO: Red Knot should emit an `unused-suppression` diagnostic for the
`possibly-unresolved-reference` suppression.

```py
test = 10
a = test + 3  # knot: ignore[possibly-unresolved-reference]
```

## Useless suppression if the error codes don't match

TODO: Red Knot should emit a `unused-suppression` diagnostic for the `possibly-unresolved-reference`
suppression because it doesn't match the actual `unresolved-reference` diagnostic.

```py
# error: [unresolved-reference]
a = test + 3  # knot: ignore[possibly-unresolved-reference]
```

## Multiple suppressions

```py
# fmt: off
def test(a: f"f-string type annotation", b: b"byte-string-type-annotation"): ...  # knot: ignore[fstring-type-annotation, byte-string-type-annotation]
```

## Can't suppress syntax errors

<!-- blacken-docs:off -->

```py
# error: [invalid-syntax]
def test(  # knot: ignore
```

<!-- blacken-docs:on -->

## Can't suppress `revealed-type` diagnostics

```py
a = 10
# revealed: Literal[10]
reveal_type(a)  # knot: ignore
```

## Extra whitespace in type ignore comments is allowed

```py
a = 10 / 0  # knot   :   ignore
a = 10 / 0  # knot: ignore  [    division-by-zero   ]
```

## Whitespace is optional

```py
# fmt: off
a = 10 / 0  #knot:ignore[division-by-zero]
```

## Trailing codes comma

Trailing commas in the codes section are allowed:

```py
a = 10 / 0  # knot: ignore[division-by-zero,]
```

## Invalid characters in codes

```py
# error: [division-by-zero]
a = 10 / 0  # knot: ignore[*-*]
```

## Trailing whitespace

<!-- blacken-docs:off -->

```py
a = 10 / 0  # knot: ignore[division-by-zero]      
            #                               ^^^^^^ trailing whitespace
```

<!-- blacken-docs:on -->

## Missing comma

A missing comma results in an invalid suppression comment. We may want to recover from this in the
future.

```py
# error: [unresolved-reference]
a = x / 0  # knot: ignore[division-by-zero unresolved-reference]
```

## Empty codes

An empty codes array suppresses no-diagnostics and is always useless

```py
# error: [division-by-zero]
a = 4 / 0  # knot: ignore[]
```

## File-level suppression comments

File level suppression comments are currently intentionally unsupported because we've yet to decide
if they should use a different syntax that also supports enabling rules or changing the rule's
severity: `knot: possibly-undefined-reference=error`

```py
# knot: ignore[division-by-zero]

a = 4 / 0  # error: [division-by-zero]
```
