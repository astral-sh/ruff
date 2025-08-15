import sys
from collections.abc import Container, Iterable, Sequence
from types import ModuleType
from typing import Any, Literal

if sys.platform == "win32":
    from _msi import *
    from _msi import _Database

    AMD64: bool
    Win64: bool

    datasizemask: Literal[0x00FF]
    type_valid: Literal[0x0100]
    type_localizable: Literal[0x0200]
    typemask: Literal[0x0C00]
    type_long: Literal[0x0000]
    type_short: Literal[0x0400]
    type_string: Literal[0x0C00]
    type_binary: Literal[0x0800]
    type_nullable: Literal[0x1000]
    type_key: Literal[0x2000]
    knownbits: Literal[0x3FFF]

    class Table:
        name: str
        fields: list[tuple[int, str, int]]
        def __init__(self, name: str) -> None: ...
        def add_field(self, index: int, name: str, type: int) -> None: ...
        def sql(self) -> str: ...
        def create(self, db: _Database) -> None: ...

    class _Unspecified: ...

    def change_sequence(
        seq: Sequence[tuple[str, str | None, int]],
        action: str,
        seqno: int | type[_Unspecified] = ...,
        cond: str | type[_Unspecified] = ...,
    ) -> None:
        """Change the sequence number of an action in a sequence list"""

    def add_data(db: _Database, table: str, values: Iterable[tuple[Any, ...]]) -> None: ...
    def add_stream(db: _Database, name: str, path: str) -> None: ...
    def init_database(
        name: str, schema: ModuleType, ProductName: str, ProductCode: str, ProductVersion: str, Manufacturer: str
    ) -> _Database: ...
    def add_tables(db: _Database, module: ModuleType) -> None: ...
    def make_id(str: str) -> str: ...
    def gen_uuid() -> str: ...

    class CAB:
        name: str
        files: list[tuple[str, str]]
        filenames: set[str]
        index: int
        def __init__(self, name: str) -> None: ...
        def gen_id(self, file: str) -> str: ...
        def append(self, full: str, file: str, logical: str) -> tuple[int, str]: ...
        def commit(self, db: _Database) -> None: ...

    _directories: set[str]

    class Directory:
        db: _Database
        cab: CAB
        basedir: str
        physical: str
        logical: str
        component: str | None
        short_names: set[str]
        ids: set[str]
        keyfiles: dict[str, str]
        componentflags: int | None
        absolute: str
        def __init__(
            self,
            db: _Database,
            cab: CAB,
            basedir: str,
            physical: str,
            _logical: str,
            default: str,
            componentflags: int | None = None,
        ) -> None:
            """Create a new directory in the Directory table. There is a current component
            at each point in time for the directory, which is either explicitly created
            through start_component, or implicitly when files are added for the first
            time. Files are added into the current component, and into the cab file.
            To create a directory, a base directory object needs to be specified (can be
            None), the path to the physical directory, and a logical directory name.
            Default specifies the DefaultDir slot in the directory table. componentflags
            specifies the default flags that new components get.
            """

        def start_component(
            self,
            component: str | None = None,
            feature: Feature | None = None,
            flags: int | None = None,
            keyfile: str | None = None,
            uuid: str | None = None,
        ) -> None:
            """Add an entry to the Component table, and make this component the current for this
            directory. If no component name is given, the directory name is used. If no feature
            is given, the current feature is used. If no flags are given, the directory's default
            flags are used. If no keyfile is given, the KeyPath is left null in the Component
            table.
            """

        def make_short(self, file: str) -> str: ...
        def add_file(self, file: str, src: str | None = None, version: str | None = None, language: str | None = None) -> str:
            """Add a file to the current component of the directory, starting a new one
            if there is no current component. By default, the file name in the source
            and the file table will be identical. If the src file is specified, it is
            interpreted relative to the current directory. Optionally, a version and a
            language can be specified for the entry in the File table.
            """

        def glob(self, pattern: str, exclude: Container[str] | None = None) -> list[str]:
            """Add a list of files to the current component as specified in the
            glob pattern. Individual files can be excluded in the exclude list.
            """

        def remove_pyc(self) -> None:
            """Remove .pyc files on uninstall"""

    class Binary:
        name: str
        def __init__(self, fname: str) -> None: ...

    class Feature:
        id: str
        def __init__(
            self,
            db: _Database,
            id: str,
            title: str,
            desc: str,
            display: int,
            level: int = 1,
            parent: Feature | None = None,
            directory: str | None = None,
            attributes: int = 0,
        ) -> None: ...
        def set_current(self) -> None: ...

    class Control:
        dlg: Dialog
        name: str
        def __init__(self, dlg: Dialog, name: str) -> None: ...
        def event(self, event: str, argument: str, condition: str = "1", ordering: int | None = None) -> None: ...
        def mapping(self, event: str, attribute: str) -> None: ...
        def condition(self, action: str, condition: str) -> None: ...

    class RadioButtonGroup(Control):
        property: str
        index: int
        def __init__(self, dlg: Dialog, name: str, property: str) -> None: ...
        def add(self, name: str, x: int, y: int, w: int, h: int, text: str, value: str | None = None) -> None: ...

    class Dialog:
        db: _Database
        name: str
        x: int
        y: int
        w: int
        h: int
        def __init__(
            self,
            db: _Database,
            name: str,
            x: int,
            y: int,
            w: int,
            h: int,
            attr: int,
            title: str,
            first: str,
            default: str,
            cancel: str,
        ) -> None: ...
        def control(
            self,
            name: str,
            type: str,
            x: int,
            y: int,
            w: int,
            h: int,
            attr: int,
            prop: str | None,
            text: str | None,
            next: str | None,
            help: str | None,
        ) -> Control: ...
        def text(self, name: str, x: int, y: int, w: int, h: int, attr: int, text: str | None) -> Control: ...
        def bitmap(self, name: str, x: int, y: int, w: int, h: int, text: str | None) -> Control: ...
        def line(self, name: str, x: int, y: int, w: int, h: int) -> Control: ...
        def pushbutton(
            self, name: str, x: int, y: int, w: int, h: int, attr: int, text: str | None, next: str | None
        ) -> Control: ...
        def radiogroup(
            self, name: str, x: int, y: int, w: int, h: int, attr: int, prop: str | None, text: str | None, next: str | None
        ) -> RadioButtonGroup: ...
        def checkbox(
            self, name: str, x: int, y: int, w: int, h: int, attr: int, prop: str | None, text: str | None, next: str | None
        ) -> Control: ...
