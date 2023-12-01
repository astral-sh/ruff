import subprocess


def foo():
    pass


# Errors.
subprocess.Popen(preexec_fn=foo)
subprocess.Popen(["ls"], preexec_fn=foo)
subprocess.Popen(preexec_fn=lambda: print("Hello, world!"))
subprocess.Popen(["ls"], preexec_fn=lambda: print("Hello, world!"))

# Non-errors.
subprocess.Popen()
subprocess.Popen(["ls"])
subprocess.Popen(preexec_fn=None)  # None is the default.
subprocess.Popen(["ls"], preexec_fn=None)  # None is the default.
