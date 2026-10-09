# `mutable-argument-default` (`B006`)

```toml
lint.select = ["B006", "B008"]
```

## `collections`

Mutable types from `collections` should trigger `mutable-argument-default` (`B006`) instead of `function-call-in-default-argument` (`B008`).

```py
import collections

def f(
    a=collections.ChainMap(),  # error: [mutable-argument-default]
    b=collections.UserDict(),  # error: [mutable-argument-default]
):
    pass
```

## `weakref`

Weak reference container types from `weakref` should trigger `mutable-argument-default` (`B006`) instead of `function-call-in-default-argument` (`B008`).

```py
import weakref

def f(
    a=weakref.WeakKeyDictionary(),  # error: [mutable-argument-default]
    b=weakref.WeakValueDictionary(),  # error: [mutable-argument-default]
    c=weakref.WeakSet(),  # error: [mutable-argument-default]
):
    pass
```
