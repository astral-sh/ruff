import sys
from sys import platform, version_info

if sys.version_info[0.0] == 2: ...  # PYI003
if sys.version_info[0j] == 2: ...  # PYI003
if sys.version_info[0] == (2, 7): ...  # PYI003
if sys.version_info[0] == '2': ...  # PYI003
if sys.version_info[:2] == "lol": ...  # PYI003
if sys.version_info[:2.0] >= (3, 9): ...  # PYI003
if sys.version_info[:2j] >= (3, 9): ...  # PYI003
if sys.version_info[:, :] >= (2, 7): ...  # PYI003
if sys.version_info < ('3', '0'): ...  # PYI003
if sys.version_info < [3, 0]: ...  # PYI003
if sys.version_info[1:] >= (7, 11): ...  # PYI003
if sys.version_info[0:2] >= (7, 11): ...  # PYI003
if sys.version_info[::-1] < (11, 7): ...  # PYI003
if sys.version_info[:3] >= (2, 7): ...  # PYI003



if sys.version_info[0] == 2: ...
if version_info[0] == 2: ...
if sys.version_info < (3, 5): ...
if version_info >= (3, 5): ...
if sys.version_info[:2] == (2, 7): ...
if sys.version_info[:1] == (2,): ...
if platform == 'linux': ...
