import os
import subprocess

os.popen("chmod +w foo*")
subprocess.Popen("/bin/chown root: *", shell=True)
subprocess.Popen(["/usr/local/bin/rsync", "*", "some_where:"], shell=True)
subprocess.Popen("/usr/local/bin/rsync * no_injection_here:")
os.system("tar cf foo.tar bar/*")

subprocess.Popen(["chmod", "+w", "*.py"], shell={**{}})
subprocess.Popen(["chmod", "+w", "*.py"], shell={**{**{}}})

# Additional truthiness cases for generator, lambda, and f-strings
subprocess.Popen("chmod +w foo*", shell=(i for i in ()))
subprocess.Popen("chmod +w foo*", shell=lambda: 0)
subprocess.Popen("chmod +w foo*", shell=f"{b''}")
x = 1
subprocess.Popen("chmod +w foo*", shell=f"{x=}")
