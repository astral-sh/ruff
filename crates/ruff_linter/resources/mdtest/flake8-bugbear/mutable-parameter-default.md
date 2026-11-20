# `mutable-parameter-default` (`B006`)

```toml
lint.select = ["B006", "B008"]
```

## `collections`

Mutable types from `collections` should trigger `mutable-parameter-default` (`B006`) instead of `function-call-in-parameter-default` (`B008`).

```py
import collections

def f(
    a=collections.ChainMap(),  # error: [mutable-parameter-default]
    b=collections.UserDict(),  # error: [mutable-parameter-default]
    c=collections.UserList(),  # error: [mutable-parameter-default]
    d=collections.UserString(""),  # error: [mutable-parameter-default]
):
    pass
```

## `weakref`

Weak reference container types from `weakref` should trigger `mutable-parameter-default` (`B006`) instead of `function-call-in-parameter-default` (`B008`).

```py
import weakref

def f(
    a=weakref.WeakKeyDictionary(),  # error: [mutable-parameter-default]
    b=weakref.WeakValueDictionary(),  # error: [mutable-parameter-default]
    c=weakref.WeakSet(),  # error: [mutable-parameter-default]
):
    pass
```
