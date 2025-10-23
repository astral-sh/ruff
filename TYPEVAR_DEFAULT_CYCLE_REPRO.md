# Absolute Minimal Reproduction for ty Check Panic

## The Simplest Case (16 lines)

```python
"""Absolute bare minimum."""

from __future__ import annotations

from typing import Protocol
from typing_extensions import TypeVar

T = TypeVar("T", default="C", covariant=True)


class P(Protocol[T]):
    pass


class C(P[T]):
    pass
```

## Reproduce

```bash
~/astral/ruff/target/debug/ty check --python ../_steam.py_venv/ direct_cycle_test.py --output-format=concise
```

Full reproduction file is located at:
- `/private/tmp/mypy_primer/projects/steam.py/direct_cycle_test.py`

## Result

```
fatal[panic] Panicked at .../salsa.../src/function/fetch.rs:264:21
dependency graph cycle when querying TypeVarInstance::lazy_default_

Query stack:
[
    check_file_impl(Id(c00)),
    infer_scope_types(Id(1000)),
    infer_definition_types(Id(1405)),
    TypeVarInstance < 'db >::lazy_default_(Id(7800)),
    infer_deferred_types(Id(1403)),
    ClassLiteral < 'db >::is_typed_dict_(Id(3809)),
    ClassLiteral < 'db >::inherited_legacy_generic_context_(Id(3809)),
    place_by_id(Id(340a)),
    infer_definition_types(Id(61d7)),
    infer_definition_types(Id(6656)),
    ClassLiteral < 'db >::is_typed_dict_(Id(3805)),
    file_to_module(Id(c1b)),
    Type < 'db >::member_lookup_with_policy_(Id(2c06)),
    place_by_id(Id(3408)),
    infer_definition_types(Id(6291)),
    source_text(Id(c0f)),
]
```

## What Triggers It

The **absolute minimum** requirements (cannot be reduced further without changing the panic type):
1. **A TypeVar with a forward reference default** (`T` → `"C"`)
2. **A Protocol using that TypeVar** (`P[T]`)
3. **A class inheriting from the Protocol with the same TypeVar** (`C(P[T])`)
4. **The class name matches the TypeVar's default** (`"C"`)

**This creates a direct cycle:**
- To resolve class `C`, ty needs to resolve `P[T]`
- To resolve `P[T]`, ty needs to resolve TypeVar `T`'s default
- TypeVar `T`'s default is `"C"`
- This creates: `C` → `P[T]` → `T.default` → `"C"` → **cycle!**

## Key Findings

- **Only 16 lines needed** - the absolute minimum for this specific panic
- **Single file** - no cross-module imports needed
- **Single TypeVar** - no need for multiple TypeVars
- **Protocol required** - using `Generic` instead doesn't trigger this panic
- **Two classes required** - one class triggers a different error
- **No TypeAlias needed** - the cycle happens just from class inheritance
- The cycle is purely: **TypeVar default → Protocol → Class → TypeVar default**

## Root Cause

When ty checks class `C(P[T])`:
1. It needs to resolve the base class `P[T]`
2. To specialize `P[T]`, it needs to know what `T` is
3. Since `T` is not provided, it tries to use `T`'s default value `"C"`
4. To resolve `"C"`, it needs to resolve class `C`
5. **Cycle detected!**

The salsa query system correctly identifies this as:
```
TypeVarInstance::lazy_default_ → infer_deferred_types →
ClassLiteral::is_typed_dict_ → ClassLiteral::inherited_legacy_generic_context_ →
... → back to TypeVarInstance::lazy_default_
```

## Why This Happens

PEP 696 (TypeVar defaults) allows forward references in defaults, but creates a circular dependency when:
- A class inherits from a Protocol/Generic using a TypeVar
- That TypeVar's default points back to the class itself
- The type checker needs to resolve the default to check the inheritance

This is fundamentally a design issue with how TypeVar defaults interact with Protocol inheritance checking.

## Origin

Originally found in the `steam.py` project but reduced from 27+ lines to just 16 lines by:
1. Removing cross-module imports (not needed)
2. Using a single TypeVar instead of multiple (not needed)
3. Removing Generic classes (not needed)
4. Removing TypeAlias (not needed)
5. Simplifying class names to single letters
6. Having the TypeVar default point to the inheriting class (key insight!)
