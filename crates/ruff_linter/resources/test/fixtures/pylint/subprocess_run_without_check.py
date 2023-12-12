import subprocess

# Errors.
subprocess.run("ls")
subprocess.run("ls", shell=True)
subprocess.run(
    ["ls"],
    shell=False,
)
subprocess.run(["ls"], **kwargs)

# Non-errors.
subprocess.run("ls", check=True)
subprocess.run("ls", check=False)
subprocess.run("ls", shell=True, check=True)
subprocess.run("ls", shell=True, check=False)
foo.run("ls")  # Not a subprocess.run call.
subprocess.bar("ls")  # Not a subprocess.run call.
