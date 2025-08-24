from typing import TypeVar

# Forward references are okay for both `bound` and `default` in a stub file
_P = TypeVar("_P", bound=X, default=X)

class X: ...
