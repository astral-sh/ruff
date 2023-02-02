import sys

if True:
    if sys.version_info < (3, 3):
        cmd = [sys.executable, "-m", "test.regrtest"]


if True:
    if foo:
        pass
    elif sys.version_info < (3, 3):
        cmd = [sys.executable, "-m", "test.regrtest"]

if True:
    if foo:
        pass
    elif sys.version_info < (3, 3):
        cmd = [sys.executable, "-m", "test.regrtest"]
    elif foo:
        cmd = [sys.executable, "-m", "test", "-j0"]

    if foo:
        pass
    elif sys.version_info < (3, 3):
        cmd = [sys.executable, "-m", "test.regrtest"]

    if sys.version_info < (3, 3):
        cmd = [sys.executable, "-m", "test.regrtest"]

    if foo:
        pass
    elif sys.version_info < (3, 3):
        cmd = [sys.executable, "-m", "test.regrtest"]
    else:
        cmd = [sys.executable, "-m", "test", "-j0"]

    if sys.version_info < (3, 3):
        cmd = [sys.executable, "-m", "test.regrtest"]
    else:
        cmd = [sys.executable, "-m", "test", "-j0"]

    if sys.version_info < (3, 3):
        cmd = [sys.executable, "-m", "test.regrtest"]
    elif foo:
        cmd = [sys.executable, "-m", "test", "-j0"]
