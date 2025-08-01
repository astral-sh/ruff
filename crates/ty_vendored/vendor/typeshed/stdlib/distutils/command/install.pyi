import sys
from _typeshed import Incomplete
from collections.abc import Callable
from typing import Any, ClassVar, Final, Literal

from ..cmd import Command

HAS_USER_SITE: Final[bool]

SCHEME_KEYS: Final[tuple[Literal["purelib"], Literal["platlib"], Literal["headers"], Literal["scripts"], Literal["data"]]]
INSTALL_SCHEMES: Final[dict[str, dict[str, str]]]

if sys.version_info < (3, 10):
    WINDOWS_SCHEME: Final[dict[str, str]]

class install(Command):
    description: str
    user_options: ClassVar[list[tuple[str, str | None, str]]]
    boolean_options: ClassVar[list[str]]
    negative_opt: ClassVar[dict[str, str]]
    prefix: str | None
    exec_prefix: Incomplete
    home: str | None
    user: bool
    install_base: Incomplete
    install_platbase: Incomplete
    root: str | None
    install_purelib: Incomplete
    install_platlib: Incomplete
    install_headers: Incomplete
    install_lib: str | None
    install_scripts: Incomplete
    install_data: Incomplete
    install_userbase: Incomplete
    install_usersite: Incomplete
    compile: Incomplete
    optimize: Incomplete
    extra_path: Incomplete
    install_path_file: int
    force: int
    skip_build: int
    warn_dir: int
    build_base: Incomplete
    build_lib: Incomplete
    record: Incomplete
    def initialize_options(self) -> None: ...
    config_vars: Incomplete
    install_libbase: Incomplete
    def finalize_options(self) -> None: ...
    def dump_dirs(self, msg) -> None: ...
    def finalize_unix(self) -> None: ...
    def finalize_other(self) -> None: ...
    def select_scheme(self, name) -> None: ...
    def expand_basedirs(self) -> None: ...
    def expand_dirs(self) -> None: ...
    def convert_paths(self, *names) -> None: ...
    path_file: Incomplete
    extra_dirs: Incomplete
    def handle_extra_path(self) -> None: ...
    def change_roots(self, *names) -> None: ...
    def create_home_path(self) -> None: ...
    def run(self) -> None: ...
    def create_path_file(self) -> None: ...
    def get_outputs(self): ...
    def get_inputs(self): ...
    def has_lib(self): ...
    def has_headers(self): ...
    def has_scripts(self): ...
    def has_data(self): ...
    # Any to work around variance issues
    sub_commands: ClassVar[list[tuple[str, Callable[[Any], bool] | None]]]
