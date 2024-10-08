# Utility types for typeshed

This package and its submodules contains various common types used by
typeshed. It can also be used by packages outside typeshed, but beware
the API stability guarantees below.

## Usage

The `_typeshed` package and its types do not exist at runtime, but can be
used freely in stubs (`.pyi`) files. To import the types from this package in
implementation (`.py`) files, use the following construct:

```python
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from _typeshed import ...
```

Types can then be used in annotations by either quoting them or
using:

```python
from __future__ import annotations
```

## API Stability

You can use this package and its submodules outside of typeshed, but we
guarantee only limited API stability. Items marked as "stable" will not be
removed or changed in an incompatible way for at least one year.
Before making such a change, the "stable" moniker will be removed
and we will mark the type in question as deprecated. No guarantees
are made about unmarked types.
