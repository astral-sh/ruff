# Builtin attribute shadowing (`A003`)

```toml
target-version = "py313"

[lint]
select = ["A003"]
```

## Annotation-only declarations do not create runtime attributes

The reference resolves to builtin `id`, since the annotation does not assign a class attribute.

```py
class Container:
    id: int
    value = id
```

## Annotated assignments create attributes

```py
class Container:
    id: int = 1
    value = id  # error: [builtin-attribute-shadowing]
```

## Methods can shadow builtins

```py
class Container:
    def list(self):
        pass

    value = list  # error: [builtin-attribute-shadowing]
```

## A later annotation leaves an assigned attribute in place

```py
class Container:
    id = 1
    id: int
    value = id  # error: [builtin-attribute-shadowing]
```

## An assignment after an annotation creates an attribute

```py
class Container:
    id: int
    id = 1
    value = id  # error: [builtin-attribute-shadowing]
```

## Stub declarations define attributes

```pyi
class Container:
    id: int
    value = id  # error: [builtin-attribute-shadowing]
```

## Deferred annotations in a nongeneric class

```toml
target-version = "py314"

[lint]
select = ["A003"]
```

```py
class Container:
    id: int

    def get(self) -> id:
        pass
```

## Deferred annotations in a generic class

```toml
target-version = "py314"

[lint]
select = ["A003"]
```

```py
class Container[T]:
    id: int

    def get(self) -> tuple[T, id]:
        pass
```

## Python 3.15 deferred annotations in a generic class

```toml
target-version = "py315"

[lint]
select = ["A003"]
```

```py
class Container[T]:
    id: int

    def get(self) -> tuple[T, id]:
        pass
```
