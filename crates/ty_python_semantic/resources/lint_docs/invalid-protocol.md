## What it does

Checks for protocol classes that are invalid at runtime or do not satisfy the typing specification.

## Why is this bad?

An invalidly defined protocol class may lead to the type checker inferring unexpected things or
accepting unsafe operations. Some invalid protocol definitions also raise `TypeError` at runtime.

## Examples

A `Protocol` class cannot inherit from a non-`Protocol` class; this raises a `TypeError` at runtime:

```pycon
>>> from typing import Protocol
>>> class Foo(int, Protocol): ...
Traceback (most recent call last):
  File "<python-input-1>", line 1, in <module>
    class Foo(int, Protocol): ...
TypeError: Protocols can only inherit from other protocols, got <class 'int'>
```

## Rule history

- v0.0.102: Split out `invalid-type-variable-variance`.
