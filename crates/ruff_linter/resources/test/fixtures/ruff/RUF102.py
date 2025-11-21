# Invalid code
import os  # noqa: INVALID123
# External code
import re  # noqa: V123
# Valid noqa
import sys  # noqa: E402
from functools import cache  # Preceeding comment # noqa: F401, INVALID456
from itertools import product  # Preceeding comment # noqa: INVALID789
# Succeeding comment
import math # noqa: INVALID000 # Succeeding comment
# Mixed valid and invalid
from typing import List  # noqa: F401, INVALID123
# Test for multiple invalid
from collections import defaultdict  # noqa: INVALID100, INVALID200, F401
# Test for preserving valid codes when fixing
from itertools import chain  # noqa: E402, INVALID300, F401
# Test for mixed code types
import json  # noqa: E402, INVALID400, V100
# Test for rule redirects
import pandas as pd  # noqa: TCH002
