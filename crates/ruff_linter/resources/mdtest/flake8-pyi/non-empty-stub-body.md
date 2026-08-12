# `non-empty-stub-body` (`PYI010`)

```toml
[lint]
select = ["PYI010"]
```

## Bodies that are already empty

`...` is the form a stub body should take, so it is never flagged. `pass` and docstrings are owned
by `pass-statement-stub-body` (`PYI009`) and `docstring-in-stub` (`PYI021`), and a body that piles
several of these up is owned by `stub-body-multiple-statements` (`PYI048`) and
`unnecessary-placeholder` (`PIE790`), so this rule leaves all of them to those rules.

```pyi
def ellipsis(): ...
def with_pass():
    pass

def with_docstring():
    """A docstring."""

def piled_up():
    """A docstring."""
    ...
    pass
```

A docstring built by implicit concatenation is still a single statement, so it stays whole.

```pyi
def concatenated():
    """Doc, part one.""" """Doc, part two."""
```

## Bodies with a single statement

Any other statement is flagged and replaced with `...`.

```pyi
def double(x: int) -> int:
    # error: [non-empty-stub-body]
    return x * 2

def expression():
    # error: [non-empty-stub-body]
    123

def assignment():
    # error: [non-empty-stub-body]
    x = 123
```

## Runtime files

Only stub files are checked. The same body in a `.py` file is left alone, because it actually runs.

```py
def single(x: int) -> int:
    return x

def double(x: int) -> int:
    doubled = x * 2
    return doubled
```

## Bodies with multiple statements

Outside of preview, a body holding more than one statement is left to
`stub-body-multiple-statements` (`PYI048`).

```pyi
def double(x: int) -> int:
    doubled = x * 2
    return doubled
```

## Preview: every statement in the body

In preview, each statement that is not `...`, `pass`, or a docstring is flagged on its own, however
many of them the body holds.

```toml
[lint]
preview = true
select = ["PYI010"]
```

### Several statements to remove

The first statement is replaced with `...` so that the body does not become empty, and the rest are
removed outright.

```pyi
def double(x: int) -> int:
    # snapshot: non-empty-stub-body
    doubled = x * 2
    # snapshot: non-empty-stub-body
    return doubled
```

```snapshot
error[PYI010]: Function body must contain only `...`
 --> src/mdtest_snippet.pyi:3:5
  |
3 |     doubled = x * 2
  |     ^^^^^^^^^^^^^^^
help: Replace function body with `...`
  |
2 |     # snapshot: non-empty-stub-body
  -     doubled = x * 2
3 +     ...
4 |     # snapshot: non-empty-stub-body
  |


error[PYI010]: Function body must contain only `...`
 --> src/mdtest_snippet.pyi:5:5
  |
5 |     return doubled
  |     ^^^^^^^^^^^^^^
help: Remove statement from function body
  |
4 |     # snapshot: non-empty-stub-body
  -     return doubled
  |
```

### Statements spanning several lines

A compound statement counts as one statement, so it is flagged once and removed as a whole.

```pyi
def branching(x: int) -> int:
    # error: [non-empty-stub-body]
    if x:
        x += 1
    # error: [non-empty-stub-body]
    return x
```

### Statements the fix keeps

When the body already holds a `...`, a `pass`, or a docstring, that statement stands in for the
removed ones, so nothing has to be replaced with `...`.

```pyi
def after_docstring():
    """A docstring."""
    # error: [non-empty-stub-body]
    print("side effect")

def before_pass():
    # error: [non-empty-stub-body]
    print("side effect")
    pass

def around_ellipsis():
    # error: [non-empty-stub-body]
    print("side effect")
    ...
    # error: [non-empty-stub-body]
    print("side effect")
```

### Several statements on one line

Statements separated by semicolons are flagged individually, and the fix rewrites the line in place.

```pyi
# snapshot: non-empty-stub-body
# snapshot: non-empty-stub-body
def semicolons(): x = 123; print("side effect")
```

```snapshot
error[PYI010]: Function body must contain only `...`
 --> src/mdtest_snippet.pyi:3:19
  |
3 | def semicolons(): x = 123; print("side effect")
  |                   ^^^^^^^
help: Replace function body with `...`
  |
2 | # snapshot: non-empty-stub-body
  - def semicolons(): x = 123; print("side effect")
3 + def semicolons(): ...; print("side effect")
  |


error[PYI010]: Function body must contain only `...`
 --> src/mdtest_snippet.pyi:3:28
  |
3 | def semicolons(): x = 123; print("side effect")
  |                            ^^^^^^^^^^^^^^^^^^^^
help: Remove statement from function body
  |
2 | # snapshot: non-empty-stub-body
  - def semicolons(): x = 123; print("side effect")
3 + def semicolons(): x = 123
  |
```

### A semicolon after the statement being removed

When the statement being removed opens the body, the semicolon that separates it from the next
statement goes with it, so the statement standing in for it is all that is left on the line.

```pyi
# snapshot: non-empty-stub-body
def leading_semicolon(): x = 123; pass
```

```snapshot
error[PYI010]: Function body must contain only `...`
 --> src/mdtest_snippet.pyi:2:26
  |
2 | def leading_semicolon(): x = 123; pass
  |                          ^^^^^^^
help: Remove statement from function body
  |
1 | # snapshot: non-empty-stub-body
  - def leading_semicolon(): x = 123; pass
2 + def leading_semicolon(): pass
  |
```

### Strings that are not the docstring

Only the statement that opens the body is a docstring. A second string is dead weight, so it is
flagged and removed like any other statement.

```pyi
def two_strings():
    """A docstring."""
    # error: [non-empty-stub-body]
    """Not a docstring."""
```

### Trailing comments make the fix unsafe

Deleting a statement deletes the line it sits on, so a comment trailing that statement goes with it
and the fix is unsafe. Replacing a statement with `...` leaves the rest of the line alone, so the
trailing comment survives and that fix stays safe.

```pyi
def trailing(x: int) -> int:
    # snapshot: non-empty-stub-body
    doubled = x * 2  # keeps this
    # snapshot: non-empty-stub-body
    return doubled  # loses this

def multiline(x: int) -> int:
    """A docstring."""
    # snapshot: non-empty-stub-body
    if x:
        x += 1  # loses this
```

```snapshot
error[PYI010]: Function body must contain only `...`
 --> src/mdtest_snippet.pyi:3:5
  |
3 |     doubled = x * 2  # keeps this
  |     ^^^^^^^^^^^^^^^
help: Replace function body with `...`
  |
2 |     # snapshot: non-empty-stub-body
  -     doubled = x * 2  # keeps this
3 +     ...  # keeps this
4 |     # snapshot: non-empty-stub-body
  |


error[PYI010]: Function body must contain only `...`
 --> src/mdtest_snippet.pyi:5:5
  |
5 |     return doubled  # loses this
  |     ^^^^^^^^^^^^^^
help: Remove statement from function body
  |
4 |     # snapshot: non-empty-stub-body
  -     return doubled  # loses this
5 |
  |
note: This is an unsafe fix and may change runtime behavior


error[PYI010]: Function body must contain only `...`
  --> src/mdtest_snippet.pyi:10:5
   |
10 | /     if x:
11 | |         x += 1  # loses this
   | |______________^
help: Remove statement from function body
  |
9 |     # snapshot: non-empty-stub-body
  -     if x:
  -         x += 1  # loses this
  |
note: This is an unsafe fix and may change runtime behavior
```

### Comments inside and around removed statements

A comment nested inside a removed statement describes that statement, so it goes with it. A comment
on its own line is kept, because it may just as well be about the function or a surrounding
statement as about the one being removed.

```pyi
def commented(x: int) -> int:
    # Explains the function.
    # snapshot: non-empty-stub-body
    if x:  # why
        # nested note
        x += 1
    # Explains the return.
    # snapshot: non-empty-stub-body
    return x
```

```snapshot
error[PYI010]: Function body must contain only `...`
 --> src/mdtest_snippet.pyi:4:5
  |
4 | /     if x:  # why
5 | |         # nested note
6 | |         x += 1
  | |______________^
help: Replace function body with `...`
  |
3 |     # snapshot: non-empty-stub-body
  -     if x:  # why
  -         # nested note
  -         x += 1
4 +     ...
5 |     # Explains the return.
  |


error[PYI010]: Function body must contain only `...`
 --> src/mdtest_snippet.pyi:9:5
  |
9 |     return x
  |     ^^^^^^^^
help: Remove statement from function body
  |
8 |     # snapshot: non-empty-stub-body
  -     return x
  |
```
