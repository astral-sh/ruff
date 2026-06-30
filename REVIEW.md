# Review findings

## Later comprehension targets cannot be modeled precisely by the current loop headers

The branch now carries later generator targets through enclosing comprehension loop headers. This
fixes the type of a target left behind by a completed inner loop:

```py
first = True
[(first := False) for _ in [0, 1] if ((value := "" if first else y) or True) for y in [1]]
```

On the second outer iteration, `y` has the value from the preceding inner iteration. Omitting `y`
from the outer header therefore loses its `int` type.

However, the same header also makes a later target appear available before its first assignment:

```py
[None for a in [0, 1] if (flag := True) if c for c in [1]]
```

Python raises `UnboundLocalError` when the first outer iteration reads `c`. The base branch reports
`unresolved-reference`, but carrying `c` through the outer header reduces this to
`possibly-unresolved-reference`. Because that rule is disabled by default, the branch emits no
diagnostic in the default configuration.

### Root cause

The use-def map represents a loop header as one merged state. Adding a later target to that state
preserves the value from prior iterations, but the map does not separately represent the first
iteration, when that target is still unbound. It also does not retain the correlation needed to
prove that the valid example reads `y` only after an earlier inner iteration assigned it.

I tried recovering the first-iteration state while reporting the diagnostic by evaluating the
available narrowing constraints without loop-header definitions. That made the invalid example
definitely unbound, but it also made the valid example definitely unbound: the use-def data has
already discarded the iteration and branch correlation needed to distinguish them. The experiment
was backed out.

### Recommendation

Do not expand this branch into first-iteration-aware or relational loop analysis. That would require
a broader change to the use-def representation and fixed-point semantics, with equivalent explicit
nested-loop behavior to consider.

The conservative scoped resolution is to stop carrying later generator targets through enclosing
headers. This restores the definite unresolved-reference diagnostic for reads before the first
assignment, at the cost of retaining the existing loss of precision for values left behind by prior
inner iterations. Precise support for both cases should be handled separately as a larger control-
flow project.
