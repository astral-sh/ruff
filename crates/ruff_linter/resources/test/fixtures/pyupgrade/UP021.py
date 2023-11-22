import subprocess
from subprocess import run

# Errors
subprocess.run(["foo"], universal_newlines=True, check=True)
subprocess.run(["foo"], universal_newlines=True, text=True)
run(["foo"], universal_newlines=True, check=False)

# OK
subprocess.run(["foo"], check=True)
