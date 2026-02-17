# Constructor (PEP 695)

```toml
[environment]
python-version = "3.13"
```

Constructor type parameters should remain independent from same-named type parameters on `self`:

```py
class Pair[K, V]:
    def __init__(self, key: K, value: V) -> None:
        self.key: K = key
        self.value: V = value

    def swap(self) -> "Pair[V, K]":
        swapped_pair = Pair(self.value, self.key)
        reveal_type(swapped_pair)  # revealed: Pair[V@Pair, K@Pair]
        return swapped_pair

pair = Pair("foo", 123)
reveal_type(pair.swap())  # revealed: Pair[int, str]
```

Constructor inference should avoid "chasing" inferred type-parameter-to-type-parameter assignments
in a way that leaks the inferred type for one type parameter into another:

```py
class Pair2[K, V]:
    def __init__(self, key: K, value: V) -> None:
        self.key: K = key
        self.value: V = value

    def with_int_value(self) -> "Pair2[V, int]":
        pair = Pair2(self.value, 1)
        reveal_type(pair)  # revealed: Pair2[V@Pair2, int]
        return pair

pair = Pair2("foo", 123)
reveal_type(pair.with_int_value())  # revealed: Pair2[int, int]
```

Constructor inference should not transitively chase type parameters when doing so would conflate the
enclosing instance's type parameters with the constructed instance's type parameters:

```py
class Pair4[K, V]:
    def __init__(self, key: K, value: V) -> None:
        self.key: K = key
        self.value: V = value

    def rekey(self, key: V) -> "Pair4[V, int]":
        pair = Pair4(key, 1)
        reveal_type(pair)  # revealed: Pair4[V@Pair4, int]
        return pair

pair = Pair4("foo", 123)
rekeyed = pair.rekey(123)
reveal_type(rekeyed)  # revealed: Pair4[int, int]
```

Constructor specializations should compose in nested calls:

```py
class Pair3[K, V]:
    def __init__(self, key: K, value: V) -> None:
        self.key: K = key
        self.value: V = value

    def swap(self) -> "Pair3[V, K]":
        return Pair3(self.value, self.key)

    def swap_twice(self) -> "Pair3[K, V]":
        swapped = self.swap()
        swapped_twice = swapped.swap()
        reveal_type(swapped_twice)  # revealed: Pair3[K@Pair3, V@Pair3]
        return swapped_twice

pair = Pair3("foo", 123)
reveal_type(pair.swap_twice())  # revealed: Pair3[str, int]
```
