import sys

# issue only repros if we check `sys.version_info` here; no other test will do
if sys.version_info >= (3, 10):
    class NoneType: ...
else:
    class NoneType: ...
