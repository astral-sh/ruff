# Suppressing errors with `ty: ignore`

Type check errors can be suppressed by a `ty: ignore` comment on the same line as the violation.

## Simple `ty: ignore`

```py
a = 4 + test  # ty: ignore
```

## Suppressing a specific code

```py
a = 4 + test  # ty: ignore[unresolved-reference]
```

## Unused suppression

```py
test = 10
# snapshot
a = test + 3  # ty: ignore[possibly-unresolved-reference]
```

```snapshot
warning[unused-ignore-comment]: Unused `ty: ignore` directive
 --> src/mdtest_snippet.py:3:15
  |
1 | test = 10
2 | # snapshot
3 | a = test + 3  # ty: ignore[possibly-unresolved-reference]
  |               ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
  |
help: Remove the unused suppression comment
1 | test = 10
2 | # snapshot
  - a = test + 3  # ty: ignore[possibly-unresolved-reference]
3 + a = test + 3
```

## Unused suppression if the error codes don't match

```py
# snapshot: unused-ignore-comment
# error: [unresolved-reference]
a = test + 3  # ty: ignore[possibly-unresolved-reference]
print(a)
```

```snapshot
warning[unused-ignore-comment]: Unused `ty: ignore` directive
 --> src/mdtest_snippet.py:3:15
  |
1 | # snapshot: unused-ignore-comment
2 | # error: [unresolved-reference]
3 | a = test + 3  # ty: ignore[possibly-unresolved-reference]
  |               ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
4 | print(a)
  |
help: Remove the unused suppression comment
1 | # snapshot: unused-ignore-comment
2 | # error: [unresolved-reference]
  - a = test + 3  # ty: ignore[possibly-unresolved-reference]
3 + a = test + 3
4 | print(a)
```

## Suppressed unused comment

```py
# error: [unused-ignore-comment]
a = 10 / 2  # ty: ignore[division-by-zero]
a = 10 / 2  # ty: ignore[division-by-zero, unused-ignore-comment]
a = 10 / 2  # ty: ignore[unused-ignore-comment, division-by-zero]
a = 10 / 2  # ty: ignore[unused-ignore-comment] # type: ignore
a = 10 / 2  # type: ignore # ty: ignore[unused-ignore-comment]
```

## Unused ignore comment

```py
# snapshot
a = 10 / 0  # ty: ignore[division-by-zero, unused-ignore-comment]
```

```snapshot
warning[unused-ignore-comment]: Unused `ty: ignore` directive: 'unused-ignore-comment'
 --> src/mdtest_snippet.py:2:44
  |
1 | # snapshot
2 | a = 10 / 0  # ty: ignore[division-by-zero, unused-ignore-comment]
  |                                            ^^^^^^^^^^^^^^^^^^^^^
  |
help: Remove the unused suppression code
1 | # snapshot
  - a = 10 / 0  # ty: ignore[division-by-zero, unused-ignore-comment]
2 + a = 10 / 0  # ty: ignore[division-by-zero]
```

## Multiple unused comments

ty groups unused codes that are next to each other.

```py
# snapshot
a = 10 / 2  # ty: ignore[division-by-zero, unresolved-reference]
```

```snapshot
warning[unused-ignore-comment]: Unused `ty: ignore` directive
 --> src/mdtest_snippet.py:2:13
  |
1 | # snapshot
2 | a = 10 / 2  # ty: ignore[division-by-zero, unresolved-reference]
  |             ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
3 | # snapshot
4 | # snapshot
  |
help: Remove the unused suppression comment
1 | # snapshot
  - a = 10 / 2  # ty: ignore[division-by-zero, unresolved-reference]
2 + a = 10 / 2
3 | # snapshot
4 | # snapshot
5 | a = 10 / 0  # ty: ignore[invalid-assignment, division-by-zero, unresolved-reference]
```

```py
# snapshot
# snapshot
a = 10 / 0  # ty: ignore[invalid-assignment, division-by-zero, unresolved-reference]
```

```snapshot
warning[unused-ignore-comment]: Unused `ty: ignore` directive: 'invalid-assignment'
 --> src/mdtest_snippet.py:5:26
  |
3 | # snapshot
4 | # snapshot
5 | a = 10 / 0  # ty: ignore[invalid-assignment, division-by-zero, unresolved-reference]
  |                          ^^^^^^^^^^^^^^^^^^
6 | # snapshot
7 | a = 10 / 0  # ty: ignore[invalid-assignment, unresolved-reference, division-by-zero]
  |
help: Remove the unused suppression code
2 | a = 10 / 2  # ty: ignore[division-by-zero, unresolved-reference]
3 | # snapshot
4 | # snapshot
  - a = 10 / 0  # ty: ignore[invalid-assignment, division-by-zero, unresolved-reference]
5 + a = 10 / 0  # ty: ignore[division-by-zero, unresolved-reference]
6 | # snapshot
7 | a = 10 / 0  # ty: ignore[invalid-assignment, unresolved-reference, division-by-zero]


warning[unused-ignore-comment]: Unused `ty: ignore` directive: 'unresolved-reference'
 --> src/mdtest_snippet.py:5:64
  |
3 | # snapshot
4 | # snapshot
5 | a = 10 / 0  # ty: ignore[invalid-assignment, division-by-zero, unresolved-reference]
  |                                                                ^^^^^^^^^^^^^^^^^^^^
6 | # snapshot
7 | a = 10 / 0  # ty: ignore[invalid-assignment, unresolved-reference, division-by-zero]
  |
help: Remove the unused suppression code
2 | a = 10 / 2  # ty: ignore[division-by-zero, unresolved-reference]
3 | # snapshot
4 | # snapshot
  - a = 10 / 0  # ty: ignore[invalid-assignment, division-by-zero, unresolved-reference]
5 + a = 10 / 0  # ty: ignore[invalid-assignment, division-by-zero]
6 | # snapshot
7 | a = 10 / 0  # ty: ignore[invalid-assignment, unresolved-reference, division-by-zero]
```

```py
# snapshot
a = 10 / 0  # ty: ignore[invalid-assignment, unresolved-reference, division-by-zero]
```

```snapshot
warning[unused-ignore-comment]: Unused `ty: ignore` directive: 'invalid-assignment', 'unresolved-reference'
 --> src/mdtest_snippet.py:7:26
  |
5 | a = 10 / 0  # ty: ignore[invalid-assignment, division-by-zero, unresolved-reference]
6 | # snapshot
7 | a = 10 / 0  # ty: ignore[invalid-assignment, unresolved-reference, division-by-zero]
  |                          ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
  |
help: Remove the unused suppression codes
4 | # snapshot
5 | a = 10 / 0  # ty: ignore[invalid-assignment, division-by-zero, unresolved-reference]
6 | # snapshot
  - a = 10 / 0  # ty: ignore[invalid-assignment, unresolved-reference, division-by-zero]
7 + a = 10 / 0  # ty: ignore[division-by-zero]
```

## Multiple suppressions

```py
# fmt: off
def test(a: f"f-string type annotation", b: unresolved_ref): ...  # ty: ignore[invalid-type-form, unresolved-reference]
```

## Can't suppress syntax errors

<!-- fmt:off -->

```py
# error: [invalid-syntax]
# error: [unused-ignore-comment]
def test($):  # ty: ignore
    pass
```

<!-- fmt:on -->

## Can't suppress `revealed-type` diagnostics

```py
a = 10
# revealed: Literal[10]
# error: [ignore-comment-unknown-rule] "Unknown rule `revealed-type`"
reveal_type(a)  # ty: ignore[revealed-type]
```

## Extra whitespace in type ignore comments is allowed

```py
a = 10 / 0  # ty   :   ignore
a = 10 / 0  # ty: ignore  [    division-by-zero   ]
```

## Whitespace is optional

```py
# fmt: off
a = 10 / 0  #ty:ignore[division-by-zero]
```

## Trailing codes comma

Trailing commas in the codes section are allowed:

```py
a = 10 / 0  # ty: ignore[division-by-zero,]
```

## Invalid characters in codes

```py
# error: [division-by-zero]
# error: [invalid-ignore-comment] "Invalid `ty: ignore` comment: expected a alphanumeric character or `-` or `_` as code"
a = 10 / 0  # ty: ignore[*-*]
```

## Trailing whitespace

<!-- fmt:off -->

```py
a = 10 / 0  # ty: ignore[division-by-zero]       
            #                               ^^^^^^ trailing whitespace
```

<!-- fmt:on -->

## Missing comma

A missing comma results in an invalid suppression comment. We may want to recover from this in the
future.

```py
# error: [unresolved-reference]
# error: [invalid-ignore-comment] "Invalid `ty: ignore` comment: expected a comma separating the rule codes"
a = x / 0  # ty: ignore[division-by-zero unresolved-reference]
```

## Missing closing bracket

```py
# error: [unresolved-reference] "Name `x` used when not defined"
# error: [invalid-ignore-comment] "Invalid `ty: ignore` comment: expected a comma separating the rule codes"
a = x / 2  # ty: ignore[unresolved-reference
```

## Empty codes

An empty codes array suppresses no-diagnostics and is always useless

```py
# error: [division-by-zero]
# error: [unused-ignore-comment] "Unused `ty: ignore` without a code"
a = 4 / 0  # ty: ignore[]
```

## File-level suppression comments

File level suppression comments suppress all errors in a file with a given code.

```py
# ty: ignore[division-by-zero]

a = 4 / 0
b = a + c  # error: [unresolved-reference]
```

## Unknown rule

```py
# snapshot
a = 10 + 4  # ty: ignore[division-by-zer]
```

```snapshot
warning[ignore-comment-unknown-rule]: Unknown rule `division-by-zer`. Did you mean `division-by-zero`?
 --> src/mdtest_snippet.py:2:26
  |
1 | # snapshot
2 | a = 10 + 4  # ty: ignore[division-by-zer]
  |                          ^^^^^^^^^^^^^^^
  |
```

## Code with `lint:` prefix

```py
# error:[ignore-comment-unknown-rule] "Unknown rule `lint:division-by-zero`. Did you mean `division-by-zero`?"
# error: [division-by-zero]
a = 10 / 0  # ty: ignore[lint:division-by-zero]
```

## Suppression of specific diagnostics

In this section, we make sure that specific diagnostics can be suppressed in various forms that
users might expect to work.

### Invalid assignment

An invalid assignment can be suppressed in the following ways:

```py
# fmt: off

x1: str = 1 + 2 + 3  # ty: ignore

x2: str = (  # ty: ignore
    1 + 2 + 3
)

x4: str = (
    1 + 2 + 3
)  # ty: ignore
```

It can *not* be suppressed by putting the `# ty: ignore` on the inner expression. The range targeted
by the suppression comment needs to overlap with one of the boundaries of the value range (the outer
parentheses in this case):

```py
# fmt: off

# error: [invalid-assignment]
x4: str = (
    # error: [unused-ignore-comment]
    1 + 2 + 3  # ty: ignore
)
```
