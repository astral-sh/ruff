# Reproduces the scenario from issue #9472:
# F401 removes unused imports leaving empty `if` blocks,
# RUF050 removes those blocks, then F401 cleans up the
# now-unused guard imports on subsequent fix iterations.

import os
import sys

# F401 removes the unused `ExceptionGroup` import, leaving `pass`.
# Then RUF050 removes the empty `if`, and F401 removes `sys`.
if sys.version_info < (3, 11):
    from exceptiongroup import ExceptionGroup

# Already-empty block handled in a single pass by RUF050
if sys.version_info < (3, 11):
    pass

print(os.getcwd())
