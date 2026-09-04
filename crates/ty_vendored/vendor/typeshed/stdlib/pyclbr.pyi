from collections.abc import Mapping, Sequence

__all__ = ["readmodule", "readmodule_ex", "Class", "Function"]

class _Object:
    module: str
    name: str
    file: int
    lineno: int
    end_lineno: int | None
    parent: _Object | None

    # This is a dict at runtime, but we're typing it as Mapping to
    # avoid variance issues in the subclasses
    children: Mapping[str, _Object]

    def __init__(
        self, module: str, name: str, file: str, lineno: int, end_lineno: int | None, parent: _Object | None
    ) -> None: ...

class Function(_Object):
    is_async: bool
    parent: Function | Class | None
    children: dict[str, Class | Function]

    def __init__(
        self,
        module: str,
        name: str,
        file: str,
        lineno: int,
        parent: Function | Class | None = None,
        is_async: bool = False,
        *,
        end_lineno: int | None = None,
    ) -> None: ...

class Class(_Object):
    super: list[Class | str] | None
    methods: dict[str, int]
    parent: Class | None
    children: dict[str, Class | Function]

    def __init__(
        self,
        module: str,
        name: str,
        super_: list[Class | str] | None,
        file: str,
        lineno: int,
        parent: Class | None = None,
        *,
        end_lineno: int | None = None,
    ) -> None: ...

def readmodule(module: str, path: Sequence[str] | None = None) -> dict[str, Class]: ...
def readmodule_ex(module: str, path: Sequence[str] | None = None) -> dict[str, Class | Function | list[str]]: ...
