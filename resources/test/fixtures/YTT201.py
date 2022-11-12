import sys
from sys import version_info

print("{}.{}".format(*sys.version_info))
PY3 = sys.version_info[0] >= 3

PY3 = sys.version_info[0] == 3
PY3 = version_info[0] == 3
PY2 = sys.version_info[0] != 3
PY2 = version_info[0] != 3
