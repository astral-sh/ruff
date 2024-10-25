# Comparison: Instances

## Rich Comparison

## Membership Test Comparison

### fdafa

```py
class A:
    def __contains__(self, item: str) -> bool: ...


reveal_type("hello" in A())  # revealed: bool
reveal_type("hello" not in A())  # revealed: bool
# TODO: should be Unknown, need to check arg type and should be failed
reveal_type(42 in A())  # revealed: bool
reveal_type(42 not in A())  # revealed: bool


class B:
    def __iter__(self) -> Iterator[str]: ...


reveal_type("hello" in A())  # revealed: bool
reveal_type("hello" not in A())  # revealed: bool
reveal_type(42 in A())  # revealed: bool
reveal_type(42 not in A())  # revealed: bool


class C:
    def __getitem__(self, key: int) -> str: ...


reveal_type("hello" in A())  # revealed: bool
reveal_type("hello" not in A())  # revealed: bool
reveal_type(42 in A())  # revealed: bool
reveal_type(42 not in A())  # revealed: bool
```

### Wrong Return Type

```py
class A:
    def __contains__(self, item: str) -> str: ...


reveal_type("hello" in A())  # revealed: bool
reveal_type("hello" not in A())  # revealed: bool
```

### Wrong Fallback

```py
class Contains: ...


class Iter: ...


class GetItem: ...


class A:
    def __contains__(self, item: Contains) -> bool: ...

    def __iter__(self) -> Iterator[Iter]: ...

    def __getitem__(self, key: int) -> GetItem: ...


reveal_type(Contains() in A())  # revealed: bool

# TODO: should be `Unknown`, need to check arg type, and should not fall back to __iter__ or __getitem__
reveal_type(Iter() in A())  # revealed: bool
reveal_type(GetItem() in A())  # revealed: bool


class B:
    def __iter__(self) -> Iterator[Iter]: ...

    def __getitem__(self, key: int) -> GetItem: ...


reveal_type(Iter() in B())  # revealed: bool
# GetItem instance use `__iter__`, it's because `Iter` and `GetItem` are comparable
reveal_type(GetItem() in B())  # revealed: bool
```

### Old protocol Unsupported

```py
class A:
    def __getitem__(self, key: str) -> str: ...


# TODO should be Unknown
reveal_type(42 in A())  # revealed: bool
reveal_type("hello" in A())  # revealed: bool
```
