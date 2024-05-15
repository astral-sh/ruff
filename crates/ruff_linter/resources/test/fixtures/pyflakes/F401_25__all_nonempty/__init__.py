"""__init__.py with nonempty __all__

Unused stdlib and third party imports are unsafe removals

Unused first party imports get added to __all__
"""


# stdlib

import os  # Ok: is used

_ = os


import argparse  # Ok: is exported in __all__


import sys  # F401: remove unused


# first-party


from . import used  # Ok: is used

_ = used


from . import aliased as aliased  # Ok: is redundant alias


from . import exported  # Ok: is exported in __all__


from . import unused  # F401: add to __all__


from . import renamed as bees  # F401: add to __all__


__all__ = ["argparse", "exported"]
