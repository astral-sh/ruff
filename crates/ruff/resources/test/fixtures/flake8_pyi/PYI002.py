import sys

if sys.version == 'Python 2.7.10': ...  # Y002 If test must be a simple comparison against sys.platform or sys.version_info
if 'linux' == sys.platform: ...  # Y002 If test must be a simple comparison against sys.platform or sys.version_info
if hasattr(sys, 'maxint'): ...  # Y002 If test must be a simple comparison against sys.platform or sys.version_info
if sys.maxsize == 42: ...  # Y002 If test must be a simple comparison against sys.platform or sys.version_info
