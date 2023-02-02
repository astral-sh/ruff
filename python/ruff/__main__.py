import os
import sys
import sysconfig
from pathlib import Path

if __name__ == "__main__":
    ruff = Path(sysconfig.get_path("scripts")) / "ruff"
    sys.exit(os.spawnv(os.P_WAIT, ruff, [ruff, *sys.argv[1:]]))
