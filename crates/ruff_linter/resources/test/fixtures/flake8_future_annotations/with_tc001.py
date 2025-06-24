"""The rule should trigger here because adding the __future__ import would
allow TC001 to apply."""

from . import local_module


def func(sized: local_module.Container) -> int:
    return len(sized)
