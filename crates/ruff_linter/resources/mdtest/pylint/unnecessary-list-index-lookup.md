# `unnecessary-list-index-lookup` (`PLR1736`)

```toml
[lint]
select = ["PLR1736"]
```

## False positives related to [#25150] and [#25182]

The reference to `i` in the `else` clause is possibly unbound and should not emit a diagnostic:

```py
def foo(l: list):
    for i, v in enumerate(l):
        ...
    else:
        print(l[i])
```

We also shouldn't emit a diagnostic when the index has been shadowed by another loop:

```py
def foo(l: list):
    for i, v in enumerate(l):
        ...
        for i in v:
            print(l[i])
```

The same applies when the sequence itself is shadowed:

```py
def foo(l: list):
    for i, v in enumerate(l):
        ...
        for l in range(1):
            print(l[i])
```

And when the value binding is shadowed:

```py
def foo(l: list, values: list):
    for i, v in enumerate(l):
        ...
        for v in values:
            print(l[i])
```

Destructuring loop targets can shadow the tracked bindings too:

```py
def foo(l: list, values: list):
    for i, v in enumerate(l):
        ...
        for _, v in values:
            print(l[i])
```

The same applies to other same-scope binders:

```py
def foo(l: list, manager, subject):
    for i, v in enumerate(l):
        with manager as v:
            print(l[i])

        try:
            ...
        except Exception as i:
            print(l[i])

        if (i := 1):
            print(l[i])

        match subject:
            case {"i": i}:
                print(l[i])
```

Lookups evaluated before a rebinding target takes effect should still emit diagnostics:

```py
def foo(l: list, values, manager, exc_factory):
    for i, v in enumerate(l):
        if i := l[i]:  # error: [unnecessary-list-index-lookup]
            ...

    for i, v in enumerate(l):
        for i in values(l[i]):  # error: [unnecessary-list-index-lookup]
            ...

    for i, v in enumerate(l):
        with manager(l[i]) as i:  # error: [unnecessary-list-index-lookup]
            ...

    for i, v in enumerate(l):
        try:
            ...
        except exc_factory(l[i]) as i:  # error: [unnecessary-list-index-lookup]
            ...
```

[#25150]: https://github.com/astral-sh/ruff/issues/25150
[#25182]: https://github.com/astral-sh/ruff/issues/25182
