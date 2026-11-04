# `stub-body-multiple-statements` (`PYI048`)

```toml
[lint]
select = ["PYI048"]
```

## Single-statement bodies

```pyi
def bar(): ...  # ok
def bar():
    pass  # ok

def bar():
    """oof"""  # ok
```

## Multiple statements

```pyi
def oof():  # snapshot: stub-body-multiple-statements
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

```snapshot
error[PYI048]: Function body must contain exactly one statement
 --> src/mdtest_snippet.pyi:1:5
  |
1 | def oof():  # snapshot: stub-body-multiple-statements
  |     ^^^
```

## Non-stub files

```py
def bar():
    ...  # ok


def bar():
    pass  # ok


def bar():
    """oof"""  # ok


def oof():  # ok
    """oof"""
    print("foo")


def foo():  # ok
    """foo"""
    print("foo")
    print("foo")


def buzz():  # ok
    print("fizz")
    print("buzz")
    print("test")
```
