class BazMeta(type):
    _private_count = 1

    def __new__(mcs, name, bases, attrs):
        if mcs._private_count <= 5:
            mcs.some_method()

        return super().__new__(mcs, name, bases, attrs)

    def some_method():
        pass


class Bar:
    _private = True

    @classmethod
    def is_private(cls):
        return cls._private


class Foo(metaclass=BazMeta):

    def __init__(self):
        self.public_thing = "foo"
        self._private_thing = "bar"
        self.__really_private_thing = "baz"
        self.bar = Bar()

    def __str__(self):
        return "foo"

    def get_bar():
        if self.bar._private:  # SLF001
            return None
        if self.bar()._private:  # SLF001
            return None
        if Bar._private_thing:  # SLF001
            return None
        if Foo._private_thing:
            return None
        Foo = Bar()
        if Foo._private_thing:  # SLF001
            return None
        return self.bar

    def public_func(self):
        super().public_func()

    def _private_func(self):
        super()._private_func()

    def __really_private_func(self, arg):
        super().__really_private_func(arg)

    def __eq__(self, other):
        return self._private_thing == other._private_thing


foo = Foo()

print(foo._private_thing)  # SLF001
print(foo.__really_private_thing)  # SLF001
print(foo._private_func())  # SLF001
print(foo.__really_private_func(1))  # SLF001
print(foo.bar._private)  # SLF001
print(foo()._private_thing)  # SLF001
print(foo()._private_thing__)  # SLF001

print(foo.public_thing)
print(foo.public_func())
print(foo.__dict__)
print(foo.__str__())
print(foo().__class__)
print(foo._asdict())

import os

os._exit()

import os as operating_system

operating_system._exit(1)


from enum import Enum

Enum._missing_(1)  # OK

# Underscore-prefixed standard library members are public despite their leading
# underscores.
import __future__

__future__._Feature  # OK

import asyncio

asyncio._enter_task  # OK
asyncio._leave_task  # OK
asyncio._register_task  # OK
asyncio._unregister_task  # OK

import ctypes

ctypes._CFuncPtr  # OK
ctypes._CData  # OK
ctypes._Pointer  # OK
ctypes._SimpleCData  # OK
ctypes.CDLL._handle  # OK
ctypes.CDLL._name  # OK

import sys

sys._emscripten_info  # OK
sys._enablelegacywindowsfsencoding  # OK
sys._clear_internal_caches  # OK
sys._clear_type_cache  # OK
sys._current_exceptions  # OK
sys._current_frames  # OK
sys._stats_clear  # OK
sys._stats_dump  # OK
sys._stats_off  # OK
sys._stats_on  # OK
sys._debugmallocstats  # OK
sys._getframe  # OK
sys._getframemodulename  # OK
sys._is_gil_enabled  # OK
sys._is_immortal  # OK
sys._is_interned  # OK
sys._jit  # OK
sys._xoptions  # OK
sys._base_executable  # OK
sys.implementation._multiarch  # OK

import sysconfig

sysconfig._get_preferred_schemes  # OK

import importlib.util

importlib.util._incompatible_extension_module_restrictions  # OK

import ssl

ssl._create_unverified_context  # OK

import subprocess

subprocess._USE_POSIX_SPAWN  # OK
subprocess._USE_VFORK  # OK

import logging

logging._defaultFormatter  # OK

from collections import abc

abc.Set._hash  # OK

import ast

ast.AST._field_types  # OK

import gettext

gettext.NullTranslations._parse  # OK
gettext.NullTranslations._charset  # OK
gettext.NullTranslations._fallback  # OK
gettext.NullTranslations._info  # OK

from multiprocessing import managers

managers.BaseProxy._callmethod  # OK
managers.BaseProxy._getvalue  # OK

import unittest

unittest.TestSuite._removeTestAtIndex  # OK

import zipfile

zipfile.ZipInfo._for_archive  # OK

# SLF001: underscore-prefixed members that are not documented remain private.
import sys

sys._unknown_private_member  # SLF001
