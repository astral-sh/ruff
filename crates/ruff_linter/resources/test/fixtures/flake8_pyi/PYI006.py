import sys
from sys import version_info as python_version

if sys.version_info < (3, 9): ...  # OK

if sys.version_info >= (3, 9): ...  # OK

if sys.version_info == (3, 9): ...  # OK

if sys.version_info <= (3, 10): ...  # OK

if sys.version_info > (3, 10): ...  # OK

if python_version > (3, 10): ...  # OK
