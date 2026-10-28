# `match-capture-shadows-variable` (`RUF078`)

```toml
lint.preview = true
lint.select = ["RUF078"]
```

## Capturing a variable from the same scope

A bare name in a `case` clause captures the subject instead of comparing against the variable.

```py
status = 200
not_found = 404

match status:
    case 404:
        pass
    case not_found:  # snapshot: match-capture-shadows-variable
        pass
```

```snapshot
error[RUF078]: Capture pattern `not_found` shadows an existing variable
 --> src/mdtest_snippet.py:7:10
  |
7 |     case not_found:  # snapshot: match-capture-shadows-variable
  |          ^^^^^^^^^
help: Use a value pattern to compare against `not_found`, or `case _ as not_found:` to capture explicitly
```

## Capturing a module-level constant inside a function

The capture creates a new local variable that shadows the constant.

```py
NOT_FOUND = 404


def handle(status):
    match status:
        case 200:
            return "ok"
        case NOT_FOUND:  # error: [match-capture-shadows-variable]
            return "not found"
```

## Capturing a parameter

```py
def handle(status, default):
    match status:
        case 200:
            return "ok"
        case default:  # error: [match-capture-shadows-variable]
            return "other"
```

## Captures with a guard

A guard does not change the fact that the name is captured.

```py
limit = 10


def check(value):
    match value:
        case limit if limit > 0:  # error: [match-capture-shadows-variable]
            pass
```

## Explicit captures

`case _ as name:` makes the capture explicit, so it is not flagged.

```py
not_found = 404

match 200:
    case _ as not_found:
        pass
```

## Value patterns

Dotted names are value patterns, so they compare against the value.

```py
from http import HTTPStatus

match 200:
    case HTTPStatus.NOT_FOUND:
        pass
```

## New names

Capturing a name that is not already bound is the intended use of capture patterns.

```py
match 200:
    case other:
        pass
```

## Nested captures

Captures inside a larger pattern, such as a sequence or class pattern, often rebind existing names on
purpose.

```py
x = 0

match [1]:
    case [x]:
        pass
```

## Names bound by an earlier capture

Reusing a name that was itself bound by a capture pattern is not flagged.

```py
match 1:
    case value if value > 0:
        pass
    case _:
        pass

match 2:
    case value:
        pass
```

## Builtins

Shadowing a builtin is not reported by this rule.

```py
match 1:
    case id:
        pass
```

## Class variables

Class variables are not visible inside methods, so capturing the same name there does not shadow
them.

```py
class Handler:
    default = 0

    def handle(self, status):
        match status:
            case default:
                return default
```
