import os
import sys
import sysconfig
from pathlib import Path

RUFF_PATHS = [
    Path(sysconfig.get_config_var("userbase")) / "bin" / "ruff", 
    Path(sysconfig.get_path("scripts")) / "ruff"
]

def get_bin_ruff():
    """Return the ruff binary path."""
    for ruff_path in RUFF_PATHS:
        if ruff_path.is_file():
            return ruff_path
    raise FileNotFoundError(ruff_path)

if __name__ == "__main__":
    try:
        ruff = get_bin_ruff()
    except FileNotFoundError as e:
        raise FileNotFoundError(e) from e
    sys.exit(os.spawnv(os.P_WAIT, ruff, [ruff, *sys.argv[1:]]))

