"""Regression test for #14531.

RUF101 should trigger here because the TCH rules have been recoded to TC.
"""
# ruff: noqa: TCH002

from __future__ import annotations

import local_module


def func(sized: local_module.Container) -> int:
    return len(sized)
