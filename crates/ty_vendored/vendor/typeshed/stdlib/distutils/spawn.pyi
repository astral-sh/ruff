from collections.abc import Iterable
from typing import Literal

def spawn(
    cmd: Iterable[str],
    search_path: bool | Literal[0, 1] = 1,
    verbose: bool | Literal[0, 1] = 0,
    dry_run: bool | Literal[0, 1] = 0,
) -> None: ...
def find_executable(executable: str, path: str | None = None) -> str | None: ...
