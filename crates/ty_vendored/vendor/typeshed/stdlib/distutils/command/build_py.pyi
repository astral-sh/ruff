"""distutils.command.build_py

Implements the Distutils 'build_py' command.
"""

from _typeshed import Incomplete
from typing import ClassVar, Literal

from ..cmd import Command
from ..util import Mixin2to3 as Mixin2to3

class build_py(Command):
    description: str
    user_options: ClassVar[list[tuple[str, str | None, str]]]
    boolean_options: ClassVar[list[str]]
    negative_opt: ClassVar[dict[str, str]]
    build_lib: Incomplete
    py_modules: Incomplete
    package: Incomplete
    package_data: Incomplete
    package_dir: Incomplete
    compile: int
    optimize: int
    force: Incomplete
    def initialize_options(self) -> None: ...
    packages: Incomplete
    data_files: Incomplete
    def finalize_options(self) -> None: ...
    def run(self) -> None: ...
    def get_data_files(self):
        """Generate list of '(package,src_dir,build_dir,filenames)' tuples"""

    def find_data_files(self, package, src_dir):
        """Return filenames for package's data files in 'src_dir'"""

    def build_package_data(self) -> None:
        """Copy data files into build directory"""

    def get_package_dir(self, package):
        """Return the directory, relative to the top of the source
        distribution, where package 'package' should be found
        (at least according to the 'package_dir' option, if any).
        """

    def check_package(self, package, package_dir): ...
    def check_module(self, module, module_file): ...
    def find_package_modules(self, package, package_dir): ...
    def find_modules(self):
        """Finds individually-specified Python modules, ie. those listed by
        module name in 'self.py_modules'.  Returns a list of tuples (package,
        module_base, filename): 'package' is a tuple of the path through
        package-space to the module; 'module_base' is the bare (no
        packages, no dots) module name, and 'filename' is the path to the
        ".py" file (relative to the distribution root) that implements the
        module.
        """

    def find_all_modules(self):
        """Compute the list of all modules that will be built, whether
        they are specified one-module-at-a-time ('self.py_modules') or
        by whole packages ('self.packages').  Return a list of tuples
        (package, module, module_file), just like 'find_modules()' and
        'find_package_modules()' do.
        """

    def get_source_files(self): ...
    def get_module_outfile(self, build_dir, package, module): ...
    def get_outputs(self, include_bytecode: bool | Literal[0, 1] = 1) -> list[str]: ...
    def build_module(self, module, module_file, package): ...
    def build_modules(self) -> None: ...
    def build_packages(self) -> None: ...
    def byte_compile(self, files) -> None: ...

class build_py_2to3(build_py, Mixin2to3):
    updated_files: Incomplete
    def run(self) -> None: ...
    def build_module(self, module, module_file, package): ...
