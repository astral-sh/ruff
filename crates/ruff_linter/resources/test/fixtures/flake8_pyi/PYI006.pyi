import sys
from sys import version_info as python_version

if sys.version_info < (3, 9): ...  # OK

if sys.version_info >= (3, 9): ...  # OK

if sys.version_info == (3, 9): ...  # OK

if sys.version_info == (3, 9): ...  # Error: PYI006 Use only `<` and `>=` for version info comparisons

if sys.version_info <= (3, 10): ...  # Error: PYI006 Use only `<` and `>=` for version info comparisons

if sys.version_info <= (3, 10): ...  # Error: PYI006 Use only `<` and `>=` for version info comparisons

if sys.version_info > (3, 10): ...  # Error: PYI006 Use only `<` and `>=` for version info comparisons
elif sys.version_info > (3, 11): ...  # Error: PYI006 Use only `<` and `>=` for version info comparisons

if python_version > (3, 10): ...  # Error: PYI006 Use only `<` and `>=` for version info comparisons
elif python_version == (3, 11): ...  # Error: PYI006 Use only `<` and `>=` for version info comparisons
