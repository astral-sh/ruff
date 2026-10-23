# `subclass-builtin` (`FURB189`)

```toml
[lint]
preview = true
select = ["FURB189"]
```

## Stub files

Subclassing a builtin in a stub must be allowed so the stub can faithfully represent the runtime
implementation.

```pyi
class D(dict): ...
class L(list): ...
class S(str): ...
class SubscriptDict(dict[str, str]): ...
class SubscriptList(list[str]): ...
```
