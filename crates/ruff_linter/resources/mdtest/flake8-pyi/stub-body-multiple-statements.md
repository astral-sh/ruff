# `stub-body-multiple-statements` (`PYI048`)

```toml
[lint]
preview = true
select = ["PYI048"]
```

## Single-statement bodies

A stub function body holding exactly one statement is what the rule asks for.

```pyi
def ellipsis(): ...
def pass_statement():
    pass

def docstring():
    """oof"""
```

## Multiple statements

```pyi
def oof():  # error: [stub-body-multiple-statements]
    """oof"""
    print("foo")

def foo():  # error: [stub-body-multiple-statements]
    """foo"""
    print("foo")
    print("foo")

def buzz():  # error: [stub-body-multiple-statements]
    print("fizz")
    print("buzz")
    print("test")
```

## Placeholders after a docstring

Placeholders (`...` and `pass`) that follow a docstring are removed, leaving the docstring as the
body's only statement.

```pyi
def ellipsis():  # error: [stub-body-multiple-statements]
    """docstring"""
    ...

def pass_statement():  # error: [stub-body-multiple-statements]
    """docstring"""
    pass

def both():  # snapshot: stub-body-multiple-statements
    """docstring"""
    ...
    pass
```

```snapshot
error[PYI048]: Function body must contain exactly one statement
 --> src/mdtest_snippet.pyi:9:5
  |
9 | def both():  # snapshot: stub-body-multiple-statements
  |     ^^^^
help: Remove unnecessary placeholder statements
   |
10 |     """docstring"""
   -     ...
   -     pass
   |
```

## Placeholders around a statement

The surviving statement need not be a docstring; the rule only cares that a single statement is left
behind.

```pyi
def trailing_placeholder():  # error: [stub-body-multiple-statements]
    print("bar")
    pass

def leading_placeholders():  # snapshot: stub-body-multiple-statements
    ...
    pass
    print("bar")
```

```snapshot
error[PYI048]: Function body must contain exactly one statement
 --> src/mdtest_snippet.pyi:5:5
  |
5 | def leading_placeholders():  # snapshot: stub-body-multiple-statements
  |     ^^^^^^^^^^^^^^^^^^^^
help: Remove unnecessary placeholder statements
  |
5 | def leading_placeholders():  # snapshot: stub-body-multiple-statements
  -     ...
  -     pass
6 |     print("bar")
  |
```

## Placeholder-only bodies

When the body consists solely of placeholders, one is kept, since the body cannot be left empty. An
ellipsis is preferred, so that a body of `...` is left behind even when it isn't the first statement.

```pyi
def only_ellipses():  # snapshot: stub-body-multiple-statements
    ...
    ...

def pass_then_ellipsis():  # snapshot: stub-body-multiple-statements
    pass
    ...
```

```snapshot
error[PYI048]: Function body must contain exactly one statement
 --> src/mdtest_snippet.pyi:1:5
  |
1 | def only_ellipses():  # snapshot: stub-body-multiple-statements
  |     ^^^^^^^^^^^^^
help: Remove unnecessary placeholder statements
  |
2 |     ...
  -     ...
3 |
  |


error[PYI048]: Function body must contain exactly one statement
 --> src/mdtest_snippet.pyi:5:5
  |
5 | def pass_then_ellipsis():  # snapshot: stub-body-multiple-statements
  |     ^^^^^^^^^^^^^^^^^^
help: Remove unnecessary placeholder statements
  |
5 | def pass_then_ellipsis():  # snapshot: stub-body-multiple-statements
  -     pass
6 |     ...
  |
```

A body with no ellipsis at all keeps its first `pass`, which `PYI009` then rewrites to `...`.

```pyi
def only_passes():  # snapshot: stub-body-multiple-statements
    pass
    pass
```

```snapshot
error[PYI048]: Function body must contain exactly one statement
 --> src/mdtest_snippet.pyi:8:5
  |
8 | def only_passes():  # snapshot: stub-body-multiple-statements
  |     ^^^^^^^^^^^
help: Remove unnecessary placeholder statements
  |
9 |     pass
  -     pass
  |
```

## Trailing comments

A comment that trails a removed placeholder is preserved, as is a comment on the statement that
survives.

```pyi
def trailing_comments():  # snapshot: stub-body-multiple-statements
    """docstring"""
    ...  # keep me
    pass  # keep me too

def comment_on_kept_statement():  # snapshot: stub-body-multiple-statements
    """docstring"""  # kept
    ...
```

```snapshot
error[PYI048]: Function body must contain exactly one statement
 --> src/mdtest_snippet.pyi:1:5
  |
1 | def trailing_comments():  # snapshot: stub-body-multiple-statements
  |     ^^^^^^^^^^^^^^^^^
help: Remove unnecessary placeholder statements
  |
2 |     """docstring"""
  -     ...  # keep me
  -     pass  # keep me too
3 +     # keep me
4 +     # keep me too
5 |
  |


error[PYI048]: Function body must contain exactly one statement
 --> src/mdtest_snippet.pyi:6:5
  |
6 | def comment_on_kept_statement():  # snapshot: stub-body-multiple-statements
  |     ^^^^^^^^^^^^^^^^^^^^^^^^^
help: Remove unnecessary placeholder statements
  |
7 |     """docstring"""  # kept
  -     ...
  |
```

## Placeholder before a string literal

Removing the placeholder turns the string literal into the function's docstring, so the fix is
unsafe, matching `PIE790`.

```pyi
def placeholder_before_string():  # snapshot: stub-body-multiple-statements
    ...
    "not a docstring, until the fix is applied"
```

```snapshot
error[PYI048]: Function body must contain exactly one statement
 --> src/mdtest_snippet.pyi:1:5
  |
1 | def placeholder_before_string():  # snapshot: stub-body-multiple-statements
  |     ^^^^^^^^^^^^^^^^^^^^^^^^^
help: Remove unnecessary placeholder statements
  |
1 | def placeholder_before_string():  # snapshot: stub-body-multiple-statements
  -     ...
2 |     "not a docstring, until the fix is applied"
  |
note: This is an unsafe fix and may change runtime behavior
```

## Semicolon-separated statements

Deleting the last statement of a semicolon-separated line leaves the preceding semicolon behind. The
result is still valid Python, and `E703` removes the semicolon.

```pyi
def semicolons():  # snapshot: stub-body-multiple-statements
    pass; print("bar")

# snapshot: stub-body-multiple-statements
def trailing_semicolon(): ...; ...
```

```snapshot
error[PYI048]: Function body must contain exactly one statement
 --> src/mdtest_snippet.pyi:1:5
  |
1 | def semicolons():  # snapshot: stub-body-multiple-statements
  |     ^^^^^^^^^^
help: Remove unnecessary placeholder statements
  |
1 | def semicolons():  # snapshot: stub-body-multiple-statements
  -     pass; print("bar")
2 +     print("bar")
3 |
  |


error[PYI048]: Function body must contain exactly one statement
 --> src/mdtest_snippet.pyi:5:5
  |
5 | def trailing_semicolon(): ...; ...
  |     ^^^^^^^^^^^^^^^^^^
help: Remove unnecessary placeholder statements
  |
4 | # snapshot: stub-body-multiple-statements
  - def trailing_semicolon(): ...; ...
5 + def trailing_semicolon(): ...; 
  |
```

## Protocol members and abstract methods

`PIE790` leaves the ellipsis alone in protocol members and abstract methods, because in a runtime
file Pyright reads it as "this is a stub, not a default implementation". Every function in a stub
file is a stub already, so the ellipsis carries no such meaning here and is removed.

```pyi
from abc import ABC, abstractmethod
from typing import Protocol

class P(Protocol):
    def member(self) -> None:  # error: [stub-body-multiple-statements]
        """docstring"""
        ...

class A(ABC):
    @abstractmethod
    def method(self) -> None:  # error: [stub-body-multiple-statements]
        """docstring"""
        ...
```

## Async functions

```pyi
async def coroutine():  # error: [stub-body-multiple-statements]
    """docstring"""
    ...
```

## Compound statements

The statement that survives can itself be a compound statement, whose own body is left untouched.

```pyi
def compound():  # snapshot: stub-body-multiple-statements
    if True:
        ...
    pass
```

```snapshot
error[PYI048]: Function body must contain exactly one statement
 --> src/mdtest_snippet.pyi:1:5
  |
1 | def compound():  # snapshot: stub-body-multiple-statements
  |     ^^^^^^^^
help: Remove unnecessary placeholder statements
  |
3 |         ...
  -     pass
  |
```

## More than one statement would remain

No fix is offered when removing the placeholders would still leave more than one statement.

```pyi
def too_many_statements():  # snapshot: stub-body-multiple-statements
    print("bar")
    print("baz")
    pass
```

```snapshot
error[PYI048]: Function body must contain exactly one statement
 --> src/mdtest_snippet.pyi:1:5
  |
1 | def too_many_statements():  # snapshot: stub-body-multiple-statements
  |     ^^^^^^^^^^^^^^^^^^^
help: Remove unnecessary placeholder statements
```

## Fix requires preview mode

Outside of preview, the rule reports the same diagnostics but offers no fix.

```toml
[lint]
select = ["PYI048"]
```

```pyi
def placeholders():  # snapshot: stub-body-multiple-statements
    """docstring"""
    ...
    pass
```

```snapshot
error[PYI048]: Function body must contain exactly one statement
 --> src/mdtest_snippet.pyi:1:5
  |
1 | def placeholders():  # snapshot: stub-body-multiple-statements
  |     ^^^^^^^^^^^^
help: Remove unnecessary placeholder statements
```

## Non-stub files

The rule only applies to stub files.

```py
def oof():
    """oof"""
    print("foo")

def foo():
    """foo"""
    print("foo")
    print("foo")

def placeholders():
    """docstring"""
    ...
    pass
```
