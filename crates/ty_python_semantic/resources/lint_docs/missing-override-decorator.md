## What it does
Checks for methods that override a method or attribute in a superclass but are not decorated with `@override`.

This rule is disabled by default. Enable it to opt in to strict `@override` enforcement for a project.

## Exemptions
Overriding `__init__`, `__new__`, `__init_subclass__`, or `__post_init__` does not require
`@override`, even if the method is explicitly declared by a superclass.

## Why is this bad?
Without an `@override` annotation, refactors can silently change whether a method is an override.
Requiring `@override` on every override lets ty report when an intended override stops overriding
anything, and when a method unexpectedly starts overriding a superclass member.

## Example

```python
from typing import override

class Parent:
    def method(self) -> int:
        return 1

class Child(Parent):
    def method(self) -> int:  # Error raised here when the rule is enabled
        return 2

class ExplicitChild(Parent):
    @override
    def method(self) -> int:  # fine
        return 2
```
