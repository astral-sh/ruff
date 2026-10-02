# Regression test for a cycle found with py-fuzzer seed 77.

from typing import Any

source: Any = []
guard_source: Any = []
pattern: Any = object
type Alias = compute

match {0: lambda: compute for _ in source}:
    case pattern() if {Alias: 0 for _ in guard_source}:
        pass
    case 0:
        pass
    case Alias:
        async def compute() -> Alias:
            pass
