# Move mutable arguments below imports and docstrings
# https://github.com/astral-sh/ruff/issues/7616


def import_module_wrong(value: dict[str, str] = {}):
    import os


def import_module_with_values_wrong(value: dict[str, str] = {}):
    import os

    return 2


def import_modules_wrong(value: dict[str, str] = {}):
    import os
    import sys
    import itertools


def from_import_module_wrong(value: dict[str, str] = {}):
    from os import path


def from_imports_module_wrong(value: dict[str, str] = {}):
    from os import path
    from sys import version_info


def import_and_from_imports_module_wrong(value: dict[str, str] = {}):
    import os
    from sys import version_info


def import_docstring_module_wrong(value: dict[str, str] = {}):
    """Docstring"""
    import os


def import_module_wrong(value: dict[str, str] = {}):
    """Docstring"""
    import os; import sys


def import_module_wrong(value: dict[str, str] = {}):
    """Docstring"""
    import os; import sys; x = 1


def import_module_wrong(value: dict[str, str] = {}):
    """Docstring"""
    import os; import sys


def import_module_wrong(value: dict[str, str] = {}):
    import os; import sys


def import_module_wrong(value: dict[str, str] = {}):
    import os; import sys; x = 1


def import_module_wrong(value: dict[str, str] = {}):
    import os; import sys


def import_module_wrong(value: dict[str, str] = {}): import os


def import_module_wrong(value: dict[str, str] = {}): import os; import sys


def import_module_wrong(value: dict[str, str] = {}): \
    import os
