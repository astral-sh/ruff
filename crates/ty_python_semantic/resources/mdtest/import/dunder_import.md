# `__import__`

The global function `__import__()` allows for dynamic imports.

A few of its call patterns are recognized and resolved to literal module types instead of the
general `ModuleType`.

## Basic

```py
reveal_type(__import__("sys"))  # revealed: <module 'sys'>
reveal_type(__import__(name="shutil"))  # revealed: <module 'shutil'>

reveal_type(__import__("nonexistent"))  # revealed: ModuleType
reveal_type(__import__("collections.abc"))  # revealed: ModuleType
reveal_type(__import__("fnmatch", globals()))  # revealed: ModuleType
```

## Unions

```py
def _(flag: bool):
    if flag:
        name = "sys"
    else:
        name = "os"

    reveal_type(name)  # revealed: Literal["sys", "os"]
    reveal_type(__import__(name))  # revealed: ModuleType

    if flag:
        module = __import__("heapq")
    else:
        module = __import__("curses")

    reveal_type(module)  # revealed: <module 'heapq'> | <module 'curses'>
```

## `importlib.import_module()`

`importlib.import_module()` has similar semantics, but returns the submodule.

```py
import importlib

reveal_type(importlib.import_module("bisect"))  # revealed: <module 'bisect'>
reveal_type(importlib.import_module("os.path"))  # revealed: <module 'os.path'>
reveal_type(importlib.import_module(name="tempfile"))  # revealed: <module 'tempfile'>

reveal_type(importlib.import_module("nonexistent"))  # revealed: ModuleType
reveal_type(importlib.import_module("config", "logging"))  # revealed: ModuleType
```
