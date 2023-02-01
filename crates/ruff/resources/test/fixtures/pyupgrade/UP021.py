import subprocess
import subprocess as somename
from subprocess import run
from subprocess import run as anothername

subprocess.run(["foo"], universal_newlines=True, check=True)
somename.run(["foo"], universal_newlines=True)

run(["foo"], universal_newlines=True, check=False)
anothername(["foo"], universal_newlines=True)

subprocess.run(["foo"], check=True)
