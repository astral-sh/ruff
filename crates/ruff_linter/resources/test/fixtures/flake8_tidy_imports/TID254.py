import os
import subprocess

# These should trigger
os.system("ls")  # TID254
os.popen("ls")  # TID254

# These should not trigger
subprocess.run(["ls"])
os.path.join("a", "b")

# Test with aliased imports
from os import system
system("ls")  # TID254

# Test with renamed imports
from os import system as run_cmd
run_cmd("ls")  # TID254

# Test with module alias
import os as operating_system
operating_system.system("ls")  # TID254

# Test with third-party modules that may not be semantically resolvable
import example
example.function("argument")  # TID254
