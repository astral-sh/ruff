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

## Unused suppression

```py
test = 10
# error: [unused-ignore-comment] "Unused `knot: ignore` directive: 'possibly-unresolved-reference'"
a = test + 3  # knot: ignore[possibly-unresolved-reference]
```

## Unused suppression if the error codes don't match

```py
# error: [unresolved-reference]
# error: [unused-ignore-comment] "Unused `knot: ignore` directive: 'possibly-unresolved-reference'"
a = test + 3  # knot: ignore[possibly-unresolved-reference]
```

## Suppressed unused comment

```py
# error: [unused-ignore-comment]
a = 10 / 2  # knot: ignore[division-by-zero]
a = 10 / 2  # knot: ignore[division-by-zero, unused-ignore-comment]
a = 10 / 2  # knot: ignore[unused-ignore-comment, division-by-zero]
a = 10 / 2  # knot: ignore[unused-ignore-comment] # type: ignore
a = 10 / 2  # type: ignore # knot: ignore[unused-ignore-comment]
```

## Unused ignore comment

```py
# error: [unused-ignore-comment] "Unused `knot: ignore` directive: 'unused-ignore-comment'"
a = 10 / 0  # knot: ignore[division-by-zero, unused-ignore-comment]
```

## Multiple unused comments

Today, Red Knot emits a diagnostic for every unused code. We might want to group the codes by
comment at some point in the future.

```py
# error: [unused-ignore-comment] "Unused `knot: ignore` directive: 'division-by-zero'"
# error: [unused-ignore-comment] "Unused `knot: ignore` directive: 'unresolved-reference'"
a = 10 / 2  # knot: ignore[division-by-zero, unresolved-reference]

# error: [unused-ignore-comment] "Unused `knot: ignore` directive: 'invalid-assignment'"
# error: [unused-ignore-comment] "Unused `knot: ignore` directive: 'unresolved-reference'"
a = 10 / 0  # knot: ignore[invalid-assignment, division-by-zero, unresolved-reference]
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
# error: [unused-ignore-comment]
def test($):  # knot: ignore
    pass
```

<!-- blacken-docs:on -->

## Can't suppress `revealed-type` diagnostics

```py
a = 10
# revealed: Literal[10]
# error: [unknown-rule] "Unknown rule `revealed-type`"
reveal_type(a)  # knot: ignore[revealed-type]
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
# error: [invalid-ignore-comment] "Invalid `knot: ignore` comment: expected a alphanumeric character or `-` or `_` as code"
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
# error: [invalid-ignore-comment] "Invalid `knot: ignore` comment: expected a comma separating the rule codes"
a = x / 0  # knot: ignore[division-by-zero unresolved-reference]
```

## Missing closing bracket

```py
# error: [unresolved-reference] "Name `x` used when not defined"
# error: [invalid-ignore-comment] "Invalid `knot: ignore` comment: expected a comma separating the rule codes"
a = x / 2  # knot: ignore[unresolved-reference
```

## Empty codes

An empty codes array suppresses no-diagnostics and is always useless

```py
# error: [division-by-zero]
# error: [unused-ignore-comment] "Unused `knot: ignore` without a code"
a = 4 / 0  # knot: ignore[]
```

## File-level suppression comments

File level suppression comments are currently intentionally unsupported because we've yet to decide
if they should use a different syntax that also supports enabling rules or changing the rule's
severity: `knot: possibly-undefined-reference=error`

```py
# error: [unused-ignore-comment]
# knot: ignore[division-by-zero]

a = 4 / 0  # error: [division-by-zero]
```

## Unknown rule

```py
# error: [unknown-rule] "Unknown rule `is-equal-14`"
a = 10 + 4  # knot: ignore[is-equal-14]
```

## Code with `lint:` prefix

```py
# error:[unknown-rule] "Unknown rule `lint:division-by-zero`. Did you mean `division-by-zero`?"
# error: [division-by-zero]
a = 10 / 0  # knot: ignore[lint:division-by-zero]
```
