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
def ellipsis():  # snapshot: stub-body-multiple-statements
    """docstring"""
    ...

def pass_statement():  # snapshot: stub-body-multiple-statements
    """docstring"""
    pass

def both():  # snapshot: stub-body-multiple-statements
    """docstring"""
    ...
    pass
```

```snapshot
error[PYI048]: Function body must contain exactly one statement
 --> src/mdtest_snippet.pyi:1:5
  |
1 | def ellipsis():  # snapshot: stub-body-multiple-statements
  |     ^^^^^^^^
help: Remove unnecessary statements
  |
2 |     """docstring"""
  -     ...
3 |
  |


error[PYI048]: Function body must contain exactly one statement
 --> src/mdtest_snippet.pyi:5:5
  |
5 | def pass_statement():  # snapshot: stub-body-multiple-statements
  |     ^^^^^^^^^^^^^^
help: Remove unnecessary statements
  |
6 |     """docstring"""
  -     pass
7 |
  |


error[PYI048]: Function body must contain exactly one statement
 --> src/mdtest_snippet.pyi:9:5
  |
9 | def both():  # snapshot: stub-body-multiple-statements
  |     ^^^^
help: Remove unnecessary statements
   |
10 |     """docstring"""
   -     ...
   -     pass
   |
```

## Placeholders around a statement

The surviving statement doesn't need to be a docstring. The fix only needs a single statement to be left behind.

```pyi
def trailing_placeholder():  # snapshot: stub-body-multiple-statements
    print("bar")
    pass

def leading_placeholders():  # snapshot: stub-body-multiple-statements
    ...
    pass
    print("bar")
```

```snapshot
error[PYI048]: Function body must contain exactly one statement
 --> src/mdtest_snippet.pyi:1:5
  |
1 | def trailing_placeholder():  # snapshot: stub-body-multiple-statements
  |     ^^^^^^^^^^^^^^^^^^^^
help: Remove unnecessary statements
  |
2 |     print("bar")
  -     pass
3 |
  |


error[PYI048]: Function body must contain exactly one statement
 --> src/mdtest_snippet.pyi:5:5
  |
5 | def leading_placeholders():  # snapshot: stub-body-multiple-statements
  |     ^^^^^^^^^^^^^^^^^^^^
help: Remove unnecessary statements
  |
5 | def leading_placeholders():  # snapshot: stub-body-multiple-statements
  -     ...
  -     pass
6 |     print("bar")
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
help: Remove unnecessary statements
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
help: Remove unnecessary statements
  |
7 |     """docstring"""  # kept
  -     ...
  |
```

## Placeholder before a string literal

Removing the placeholder would turn the string literal into the function's docstring, so no fix is offered.

```pyi
def placeholder_before_string():  # snapshot: stub-body-multiple-statements
    ...
    "not a docstring"
```

```snapshot
error[PYI048]: Function body must contain exactly one statement
 --> src/mdtest_snippet.pyi:1:5
  |
1 | def placeholder_before_string():  # snapshot: stub-body-multiple-statements
  |     ^^^^^^^^^^^^^^^^^^^^^^^^^
help: Remove unnecessary statements
```

## Semicolon-separated statements

Deleting the last statement of a semicolon-separated line leaves the preceding semicolon behind. The
result is still valid Python, and `E703` removes the semicolon.

```pyi
def semicolons():  # snapshot: stub-body-multiple-statements
    pass; print("bar")

# snapshot: stub-body-multiple-statements
def trailing_semicolon(): """docstring"""; ...
```

```snapshot
error[PYI048]: Function body must contain exactly one statement
 --> src/mdtest_snippet.pyi:1:5
  |
1 | def semicolons():  # snapshot: stub-body-multiple-statements
  |     ^^^^^^^^^^
help: Remove unnecessary statements
  |
1 | def semicolons():  # snapshot: stub-body-multiple-statements
  -     pass; print("bar")
2 +     print("bar")
3 |
  |


error[PYI048]: Function body must contain exactly one statement
 --> src/mdtest_snippet.pyi:5:5
  |
5 | def trailing_semicolon(): """docstring"""; ...
  |     ^^^^^^^^^^^^^^^^^^
help: Remove unnecessary statements
  |
4 | # snapshot: stub-body-multiple-statements
  - def trailing_semicolon(): """docstring"""; ...
5 + def trailing_semicolon(): """docstring"""; 
  |
```

## Line continuations

When placeholders are joined by a semicolon and a line continuation, deleting each of them would remove overlapping ranges of the source, so no fix is offered.

```pyi
def continuation():  # snapshot: stub-body-multiple-statements
    """docstring"""
    pass; \
    pass
```

```snapshot
error[PYI048]: Function body must contain exactly one statement
 --> src/mdtest_snippet.pyi:1:5
  |
1 | def continuation():  # snapshot: stub-body-multiple-statements
  |     ^^^^^^^^^^^^
help: Remove unnecessary statements
```

When a line continuation joins the surviving statement to a placeholder, only the placeholder is deleted. The semicolon is left behind, as with other semicolon-separated statements.

```pyi
def continuation_after_statement():  # snapshot: stub-body-multiple-statements
    print("bar"); \
    pass
```

```snapshot
error[PYI048]: Function body must contain exactly one statement
 --> src/mdtest_snippet.pyi:5:5
  |
5 | def continuation_after_statement():  # snapshot: stub-body-multiple-statements
  |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^
help: Remove unnecessary statements
  |
5 | def continuation_after_statement():  # snapshot: stub-body-multiple-statements
  -     print("bar"); \
  -     pass
6 +     print("bar"); 
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
    def member(self) -> None:  # snapshot: stub-body-multiple-statements
        """docstring"""
        ...

class A(ABC):
    @abstractmethod
    def method(self) -> None:  # snapshot: stub-body-multiple-statements
        """docstring"""
        ...
```

```snapshot
error[PYI048]: Function body must contain exactly one statement
 --> src/mdtest_snippet.pyi:5:9
  |
5 |     def member(self) -> None:  # snapshot: stub-body-multiple-statements
  |         ^^^^^^
help: Remove unnecessary statements
  |
6 |         """docstring"""
  -         ...
7 |
  |


error[PYI048]: Function body must contain exactly one statement
  --> src/mdtest_snippet.pyi:11:9
   |
11 |     def method(self) -> None:  # snapshot: stub-body-multiple-statements
   |         ^^^^^^
help: Remove unnecessary statements
   |
12 |         """docstring"""
   -         ...
   |
```

## Async functions

```pyi
async def coroutine():  # snapshot: stub-body-multiple-statements
    """docstring"""
    ...
```

```snapshot
error[PYI048]: Function body must contain exactly one statement
 --> src/mdtest_snippet.pyi:1:11
  |
1 | async def coroutine():  # snapshot: stub-body-multiple-statements
  |           ^^^^^^^^^
help: Remove unnecessary statements
  |
2 |     """docstring"""
  -     ...
  |
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
help: Remove unnecessary statements
  |
3 |         ...
  -     pass
  |
```

## Placeholder-only bodies

When the body consists solely of placeholders, one of them has to stay, since a body can't be empty. The fix doesn't choose which placeholder to keep, so none is offered.

```pyi
def only_placeholders():  # snapshot: stub-body-multiple-statements
    ...
    pass
```

```snapshot
error[PYI048]: Function body must contain exactly one statement
 --> src/mdtest_snippet.pyi:1:5
  |
1 | def only_placeholders():  # snapshot: stub-body-multiple-statements
  |     ^^^^^^^^^^^^^^^^^
help: Remove unnecessary statements
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
help: Remove unnecessary statements
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
help: Remove unnecessary statements
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
