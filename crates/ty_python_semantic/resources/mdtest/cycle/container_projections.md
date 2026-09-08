# Recursive container projections

## Reconstructing a tuple from an element

Reading the first element and constructing another pair preserves the first element's type. The
second element remains a string throughout the recursive assignments, including when the whole tuple
is copied by slicing.

```py
class Pair:
    def __init__(self):
        self.value = (0, "start")

    def update(self, other: "Pair"):
        self.value = (other.value[0], "next")

    def copy_from(self, other: "Pair"):
        self.value = other.value[:]

reveal_type(Pair().value)  # revealed: tuple[int, str]
reveal_type(Pair().value[0])  # revealed: int
reveal_type(Pair().value[1])  # revealed: str
```

## Indexing through two tuple levels

Reading the element inside both tuples and rebuilding both levels preserves the nested tuple's type.
Each indexing operation removes one level of nesting.

```py
class Container:
    def __init__(self):
        self.value = ((0,),)

    def update(self):
        self.value = ((self.value[0][0],),)

reveal_type(Container().value)  # revealed: tuple[tuple[int]]
```

## Nesting a projected element

Nesting just one element produces a recursive element type. It does not make the other element
recursive or change the length of the outer tuple.

```py
class Nested:
    def __init__(self):
        self.value = (0, "start")

    def update(self, other: "Nested"):
        self.value = (other.value[0], (other.value[1],))

reveal_type(Nested().value[0])  # revealed: int
reveal_type(Nested().value[1])  # revealed: μa0. tuple[a0] | str
```

## Checking subscripts in a recursive assignment

Recursive assignments do not permit invalid indices or a slice with a zero step.

```py
class Checked:
    def __init__(self):
        self.value = (0, "start")

    def update(self, other: "Checked"):
        self.value = (other.value[0], "next")
        other.value[2]  # error: [index-out-of-bounds]
        other.value[::0]  # error: [zero-stepsize-in-slice]
        other.value["key"]  # error: [invalid-argument-type]

reveal_type(Checked().value)  # revealed: tuple[int, str]
```
