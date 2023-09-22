import os
import subprocess

os.popen("chmod +w foo*")
subprocess.Popen("/bin/chown root: *", shell=True)
subprocess.Popen(["/usr/local/bin/rsync", "*", "some_where:"], shell=True)
subprocess.Popen("/usr/local/bin/rsync * no_injection_here:")
os.system("tar cf foo.tar bar/*")
