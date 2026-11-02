# `convert-typed-dict-functional-to-class` (`UP013`)

```toml
target-version = "py315"
lint.select = ["UP013"]
```

## Class keywords

The conversion preserves `closed` as a class keyword.

```py
from typing import TypedDict

Closed = TypedDict("Closed", {}, closed=True)  # snapshot: convert-typed-dict-functional-to-class
```

```snapshot
error[UP013]: Convert `Closed` from `TypedDict` functional to class syntax
 --> src/mdtest_snippet.py:3:1
  |
3 | Closed = TypedDict("Closed", {}, closed=True)  # snapshot: convert-typed-dict-functional-to-class
  | ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
help: Convert `Closed` to class syntax
  |
2 |
  - Closed = TypedDict("Closed", {}, closed=True)  # snapshot: convert-typed-dict-functional-to-class
3 + class Closed(TypedDict, closed=True):
4 +     pass  # snapshot: convert-typed-dict-functional-to-class
5 | ExtraItems = TypedDict("ExtraItems", {}, extra_items=str)  # snapshot: convert-typed-dict-functional-to-class
  |
```

The conversion also preserves `extra_items` as a class keyword.

```py
ExtraItems = TypedDict("ExtraItems", {}, extra_items=str)  # snapshot: convert-typed-dict-functional-to-class
```

```snapshot
error[UP013]: Convert `ExtraItems` from `TypedDict` functional to class syntax
 --> src/mdtest_snippet.py:4:1
  |
4 | ExtraItems = TypedDict("ExtraItems", {}, extra_items=str)  # snapshot: convert-typed-dict-functional-to-class
  | ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
help: Convert `ExtraItems` to class syntax
  |
3 | Closed = TypedDict("Closed", {}, closed=True)  # snapshot: convert-typed-dict-functional-to-class
  - ExtraItems = TypedDict("ExtraItems", {}, extra_items=str)  # snapshot: convert-typed-dict-functional-to-class
4 + class ExtraItems(TypedDict, extra_items=str):
5 +     pass  # snapshot: convert-typed-dict-functional-to-class
  |
```
