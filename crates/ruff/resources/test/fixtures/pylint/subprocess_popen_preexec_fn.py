import subprocess


def foo():
    pass


# Errors.
subprocess.Popen(preexec_fn=foo)

# Non-errors.
subprocess.Popen()
subprocess.Popen(preexec_fn=None)  # None is the default.
