# Django Type Checking Support

This directory contains tests for Django ORM type checking support in ty.

## Overview

Django is one of the most popular Python web frameworks. Unlike libraries like Pydantic that use
`dataclass_transform`, Django requires custom built-in support for proper type checking due to its
unique metaclass-based Model system and dynamic attribute synthesis.

## Implementation Strategy

Django support will be implemented incrementally across several phases:

### Phase 0: Baseline (Current)

- Basic Model class detection via MRO
- Manual type annotations work
- Stub files provide basic types
- **Status**: ✅ Tests passing

### Phase 1: Generic Types

- Proper generic `QuerySet[Model]` and `Manager[Model]` support
- Type-safe query chaining
- **Status**: ✅ Tests passing with manual annotations

### Phase 2: Model Attribute Synthesis

- Auto-synthesize `.objects` manager on Model classes
- Auto-synthesize `.DoesNotExist` and `.MultipleObjectsReturned` exceptions
- Auto-add `id` field when not explicitly defined
- **Status**: ⏳ Planned

### Phase 3: Field Descriptors

- Implement descriptor protocol for Django Fields
- Type-safe field access (e.g., `user.name` returns `str` for `CharField`)
- Relationship field support (`ForeignKey`, `ManyToManyField`)
- **Status**: ⏳ Planned

### Phase 4: Advanced Features

- QuerySet annotation type inference
- Aggregation function return types
- Custom manager support
- **Status**: ⏳ Planned

## Test Files

- `basic_model.md` - Basic Model class detection and inheritance
- `queryset_manager.md` - QuerySet and Manager generic typing

## References

- GitHub Issue: <https://github.com/astral-sh/ty/issues/291>
- django-stubs: <https://github.com/typeddjango/django-stubs>
- django-types: <https://github.com/sbdchd/django-types>
