import os
import sys
import sysconfig
from pathlib import Path


def find_ruff_bin() -> Path:
    """Return the ruff binary path."""
    ruff_path = Path(sysconfig.get_path("scripts")) / "ruff"
    if ruff_path.is_file():
        return ruff_path

    if sys.version_info >= (3, 10):
        ruff_path = (
            Path(
                sysconfig.get_path(
                    "scripts",
                    scheme=sysconfig.get_preferred_scheme("user"),
                ),
            )
            / "ruff"
        )
        if ruff_path.is_file():
            return ruff_path

    raise FileNotFoundError(ruff_path)


if __name__ == "__main__":
    ruff = find_ruff_bin()
    sys.exit(os.spawnv(os.P_WAIT, ruff, [ruff, *sys.argv[1:]]))
