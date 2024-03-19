import os
import subprocess

import commands
import popen2

# Check all shell functions.
os.system("true")
os.popen("true")
os.popen2("true")
os.popen3("true")
os.popen4("true")
popen2.popen2("true")
popen2.popen3("true")
popen2.popen4("true")
popen2.Popen3("true")
popen2.Popen4("true")
commands.getoutput("true")
commands.getstatusoutput("true")
subprocess.getoutput("true")
subprocess.getstatusoutput("true")


# Check command argument looks unsafe.
var_string = "true"
os.system(var_string)
os.system([var_string])
os.system([var_string, ""])
