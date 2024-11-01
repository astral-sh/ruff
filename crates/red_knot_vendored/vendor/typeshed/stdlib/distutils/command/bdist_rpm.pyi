from _typeshed import Incomplete
from typing import ClassVar

from ..cmd import Command

class bdist_rpm(Command):
    description: str
    user_options: ClassVar[list[tuple[str, str | None, str]]]
    boolean_options: ClassVar[list[str]]
    negative_opt: ClassVar[dict[str, str]]
    bdist_base: Incomplete
    rpm_base: Incomplete
    dist_dir: Incomplete
    python: Incomplete
    fix_python: Incomplete
    spec_only: Incomplete
    binary_only: Incomplete
    source_only: Incomplete
    use_bzip2: Incomplete
    distribution_name: Incomplete
    group: Incomplete
    release: Incomplete
    serial: Incomplete
    vendor: Incomplete
    packager: Incomplete
    doc_files: Incomplete
    changelog: Incomplete
    icon: Incomplete
    prep_script: Incomplete
    build_script: Incomplete
    install_script: Incomplete
    clean_script: Incomplete
    verify_script: Incomplete
    pre_install: Incomplete
    post_install: Incomplete
    pre_uninstall: Incomplete
    post_uninstall: Incomplete
    prep: Incomplete
    provides: Incomplete
    requires: Incomplete
    conflicts: Incomplete
    build_requires: Incomplete
    obsoletes: Incomplete
    keep_temp: int
    use_rpm_opt_flags: int
    rpm3_mode: int
    no_autoreq: int
    force_arch: Incomplete
    quiet: int
    def initialize_options(self) -> None: ...
    def finalize_options(self) -> None: ...
    def finalize_package_data(self) -> None: ...
    def run(self) -> None: ...
