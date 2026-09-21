# Iterators

## Yield must be iterable

```py
class NotIterable: ...

class Iterator:
    def __next__(self) -> int:
        return 42

class Iterable:
    def __iter__(self) -> Iterator:
        return Iterator()

def generator_function():
    yield from Iterable()
    yield from NotIterable()  # error: "Object of type `NotIterable` is not iterable"
```
