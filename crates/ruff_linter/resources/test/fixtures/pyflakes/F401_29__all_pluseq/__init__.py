"""__init__.py with __all__ populated by plus-eq

multiple __all__ so cannot offer a fix to add to them; offer fixes for redundant-aliases
"""


from . import exported  # Ok: is exported in __all__


from . import unused # F401: convert to redundant-alias (with fix)


from . import renamed as bees # F401: convert to redundant-alias (no fix)


__all__ = []
__all__ += ["exported"]

