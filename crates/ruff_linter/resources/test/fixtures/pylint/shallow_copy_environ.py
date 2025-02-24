import copy
import os

copied_env = copy.copy(os.environ)  # [shallow-copy-environ]


# Test case where the proposed fix is wrong, i.e., unsafe fix
# Ref: https://github.com/astral-sh/ruff/issues/16274#event-16423475135

os.environ["X"] = "0"
env = copy.copy(os.environ)
os.environ["X"] = "1"
