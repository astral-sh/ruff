"""__init__.py without __all__

Unused stdlib and third party imports are unsafe removals

Unused first party imports get changed to redundant aliases
"""


# stdlib

import os  # Ok: is used

_ = os


import argparse as argparse  # Ok: is redundant alias


import sys  # F401: remove unused


# first-party


from . import used  # Ok: is used

_ = used


from . import aliased as aliased  # Ok: is redundant alias


from . import unused  # F401: change to redundant alias


from . import renamed as bees  # F401: no fix
