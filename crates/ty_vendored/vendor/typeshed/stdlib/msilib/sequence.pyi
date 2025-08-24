import sys
from typing import Final
from typing_extensions import TypeAlias

if sys.platform == "win32":
    _SequenceType: TypeAlias = list[tuple[str, str | None, int]]

    AdminExecuteSequence: Final[_SequenceType]
    AdminUISequence: Final[_SequenceType]
    AdvtExecuteSequence: Final[_SequenceType]
    InstallExecuteSequence: Final[_SequenceType]
    InstallUISequence: Final[_SequenceType]

    tables: Final[list[str]]
