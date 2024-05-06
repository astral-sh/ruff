"""__init__.py with __all__ populated by plus-eq
"""


from . import exported  # Ok: is exported in __all__


from . import unused # F401: add to __all__


from . import renamed as bees # F401: add to __all__


__all__ = []
__all__ += ["exported"]

