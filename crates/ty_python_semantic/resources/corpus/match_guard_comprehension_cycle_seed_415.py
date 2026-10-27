# Regression test for a cycle found with py-fuzzer seed 415.

from typing import Any

constants: Any = object

match lambda: captured:
    case 0 if {0: 0 for _ in [lambda: module]}:
        pass
    case constants.value if (0 for _ in []):
        pass
    case {**captured}:
        pass
    case 0:
        import sys as module
