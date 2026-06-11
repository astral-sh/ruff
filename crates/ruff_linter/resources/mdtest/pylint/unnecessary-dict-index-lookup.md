# `unnecessary-dict-index-lookup` (`PLR1733`)

```toml
[lint]
select = ["PLR1733"]
```

## False positives related to [#25150] and [#25182]

The reference to `k` in the `else` clause is possibly unbound and should not emit a diagnostic:

```py
def foo(d: dict):
    for k, v in d.items():
        ...
    else:
        print(d[k])
```

We also shouldn't emit a diagnostic when the key has been shadowed by another loop:

```py
def foo(d: dict):
    for k, v in d.items():
        ...
        for k in v:
            print(d[k])
```

The same applies when the dictionary itself is shadowed:

```py
def foo(d: dict):
    for k, v in d.items():
        ...
        for d in range(1):
            print(d[k])
```

And when the value binding is shadowed:

```py
def foo(d: dict, values: list):
    for k, v in d.items():
        ...
        for v in values:
            print(d[k])
```

Destructuring loop targets can shadow the tracked bindings too:

```py
def foo(d: dict, values: list):
    for k, v in d.items():
        ...
        for _, v in values:
            print(d[k])
```

The same applies to other same-scope binders:

```py
def foo(d: dict, manager, subject):
    for k, v in d.items():
        with manager as v:
            print(d[k])

        try:
            ...
        except Exception as k:
            print(d[k])

        if (k := 1):
            print(d[k])

        match subject:
            case {"k": k}:
                print(d[k])
```

Lookups evaluated before a rebinding target takes effect should still emit diagnostics:

```py
def foo(d: dict, values, manager, exc_factory):
    for k, v in d.items():
        if k := d[k]:  # error: [unnecessary-dict-index-lookup]
            ...

    for k, v in d.items():
        for k in values(d[k]):  # error: [unnecessary-dict-index-lookup]
            ...

    for k, v in d.items():
        with manager(d[k]) as k:  # error: [unnecessary-dict-index-lookup]
            ...

    for k, v in d.items():
        try:
            ...
        except exc_factory(d[k]) as k:  # error: [unnecessary-dict-index-lookup]
            ...
```

[#25150]: https://github.com/astral-sh/ruff/issues/25150
[#25182]: https://github.com/astral-sh/ruff/issues/25182
