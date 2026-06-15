# Recursive loop narrowing convergence

```toml
[environment]
python-version = "3.13"
```

This is minimized from a dd-trace-py ecosystem failure. Applying a truthiness narrowing constraint
to a loop-carried recursive dictionary state used to keep changing the cycle-recovery approximation
and overflow the stack.

```py
def f(items):
    state = {}
    for item in items:
        if item:
            child = state.get(item)
            if not child:
                child = {}
                state[item] = child
            state = child
    if not state.get("end"):
        state["end"] = None
```

This is minimized from a steam.py ecosystem stack overflow. A loop-carried augmented assignment can
feed back into the same variable's equality guard in the rest of the loop body. The loop fixpoint
uses a cycle-recovery approximation for the augmented assignment, and applying the equality
narrowing to that approximation used to re-enter the same predicate.

```py
def decrement_until_zero(condition: bool, limit: int | None = 100) -> None:
    while condition:
        if limit is not None:
            limit -= 1
        if limit == 0:
            return
```

This is minimized from an xarray ecosystem failure. A loop-carried recursive value can be refined
by an identity check on one branch and then reassigned from a method result on another branch. The
loop fixpoint should converge even when equivalent union states are produced in different orders.

```py
from typing import Self

class XarrayTreeNode:
    _parent: Self | None

    @property
    def parent(self) -> Self | None:
        return self._parent

    def other(self) -> Self | None:
        return self._parent

    def lookup(self, parts: list[str]) -> Self:
        current_node = self
        for part in parts:
            if part == "..":
                if current_node.parent is None:
                    raise KeyError
                current_node = current_node.parent
            else:
                child = current_node.other()
                if child is None:
                    raise KeyError
                current_node = child
        return current_node
```

This is minimized from a SymPy ecosystem failure. Exact loop-header reachability should not
re-enter inference of a type-dependent predicate that reads the same loop-carried value, such as
`len(q2)` or `values[b]`.

```py
def sympy_recurrence_vector_like(values):
    q1 = [0]
    q2 = [1]
    b, z = 0, len(values) >> 1
    while len(q2) <= z:
        while values[b] == 0:
            b += 1
            if b == len(values):
                return q2
        scale = 1 / values[b]
        next_values = [scale]
        for k in range(b + 1, len(values)):
            next_values.append(
                -sum(values[j + 1] * next_values[b - j - 1] for j in range(b, k)) * scale
            )
        values, next_values = next_values, [0] * max(len(q2), b + len(q1))
        for k, q in enumerate(q2):
            next_values[k] = scale * q
        for k, q in enumerate(q1):
            next_values[k + b] += q
        while next_values[-1] == 0:
            next_values.pop()
        q1, q2, b = q2, next_values, 1
    return [0]
```

This is minimized from a pywin32 ecosystem failure. A value can be initialized from an implicit
instance attribute, updated through a recursive loop, and then assigned back to the same attribute.
The cycle-recovery union should treat nested cycle markers as transparent representatives so the
loop-carried local converges.

```py
class History:
    def __init__(self):
        self.history_prefix = None
        self.history_pointer = None

    def history_do(self, reverse):
        pointer = self.history_pointer
        prefix = self.history_prefix
        if pointer is None or prefix is None:
            prefix = ""
            if reverse:
                pointer = 0
            else:
                pointer = -1
        while True:
            if reverse:
                pointer = pointer - 1
            else:
                pointer = pointer + 1
            if pointer < 0:
                pointer = prefix = None
                break
            if prefix == "":
                break
        self.history_pointer = pointer
        self.history_prefix = prefix
```
