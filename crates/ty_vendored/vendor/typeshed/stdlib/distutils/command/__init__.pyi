import sys

from . import (
    bdist,
    bdist_dumb,
    bdist_rpm,
    build,
    build_clib,
    build_ext,
    build_py,
    build_scripts,
    check,
    clean,
    install,
    install_data,
    install_headers,
    install_lib,
    install_scripts,
    register,
    sdist,
    upload,
)

__all__ = [
    "build",
    "build_py",
    "build_ext",
    "build_clib",
    "build_scripts",
    "clean",
    "install",
    "install_lib",
    "install_headers",
    "install_scripts",
    "install_data",
    "sdist",
    "register",
    "bdist",
    "bdist_dumb",
    "bdist_rpm",
    "check",
    "upload",
]

if sys.version_info < (3, 10):
    from . import bdist_wininst

    __all__ += ["bdist_wininst"]
