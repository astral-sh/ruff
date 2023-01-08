import os
import sys
import sysconfig

ruff = os.path.join(sysconfig.get_path("scripts"), "ruff")
sys.exit(os.spawnv(os.P_WAIT, ruff, [ruff, *sys.argv[1:]]))
