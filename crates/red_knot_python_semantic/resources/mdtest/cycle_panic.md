# Cycle panic in tracked struct

```toml
log=true
```

## Case

```py
class Foo[T: Foo, U: (Foo, Foo)]:
    pass
```
