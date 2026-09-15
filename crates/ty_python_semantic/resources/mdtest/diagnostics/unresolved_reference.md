# Diagnostics for unresolved references

## New builtin used on old Python version

<!-- snapshot-diagnostics -->

```toml
[environment]
python-version = "3.12"
```

```py
PythonFinalizationError  # error: [unresolved-reference]
```

## Typing builtin has Info help

A special diagnostic is emitted when using a deprecated alias from Typing that is builtin in this
version of Python. (full diagnostic captured in snapshot)

### Info present in Python 3.9+

```toml
[environment]
python-version = "3.9"
```

```py
foo: List[int]  # snapshot: unresolved-reference
bar: Type  # snapshot: unresolved-reference
```

```snapshot
error[unresolved-reference]: Name `List` used when not defined
 --> src/mdtest_snippet.py:1:6
  |
1 | foo: List[int]  # snapshot: unresolved-reference
  |      ^^^^ Did you mean `list`?
help: Replace with `list`
  |
  - foo: List[int]  # snapshot: unresolved-reference
1 + foo: list[int]  # snapshot: unresolved-reference
2 | bar: Type  # snapshot: unresolved-reference
  |
note: This is an unsafe fix and may change runtime behavior


error[unresolved-reference]: Name `Type` used when not defined
 --> src/mdtest_snippet.py:2:6
  |
2 | bar: Type  # snapshot: unresolved-reference
  |      ^^^^ Did you mean `type`?
help: Replace with `type`
  |
1 | foo: List[int]  # snapshot: unresolved-reference
  - bar: Type  # snapshot: unresolved-reference
2 + bar: type  # snapshot: unresolved-reference
  |
note: This is an unsafe fix and may change runtime behavior
```

### Builtin replacement shadowed at module scope

A module-level binding named `list` also shadows the standard builtin inside a nested function, so
the unresolved `List` annotation cannot safely be replaced with `list`.

```py
list = object

def check():
    value: List[int]  # snapshot: unresolved-reference
```

```snapshot
error[unresolved-reference]: Name `List` used when not defined
 --> src/mdtest_snippet.py:4:12
  |
4 |     value: List[int]  # snapshot: unresolved-reference
  |            ^^^^ Did you mean `list`?
```

### Info not present before Python 3.9

<!-- snapshot-diagnostics -->

```toml
[environment]
python-version = "3.8"
```

```py
foo: List[int]  # error: [unresolved-reference]
bar: Type  # error: [unresolved-reference]
```
