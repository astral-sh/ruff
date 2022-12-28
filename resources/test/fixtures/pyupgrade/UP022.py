from subprocess import run
import subprocess

output = run(["foo"], stdout=subprocess.PIPE, stderr=subprocess.PIPE)

output = subprocess.run(["foo"], stdout=subprocess.PIPE, stderr=subprocess.PIPE)

output = subprocess.run(stdout=subprocess.PIPE, args=["foo"],  stderr=subprocess.PIPE)

output = subprocess.run(
    ["foo"], stdout=subprocess.PIPE, check=True, stderr=subprocess.PIPE
)

output = subprocess.run(
    ["foo"], stderr=subprocess.PIPE, check=True, stdout=subprocess.PIPE
)

output = subprocess.run(
    ["foo"],
    stdout=subprocess.PIPE,
    check=True,
    stderr=subprocess.PIPE,
    text=True,
    encoding="utf-8",
    close_fds=True,
)

if output:
    output = subprocess.run(
        ["foo"],
        stdout=subprocess.PIPE,
        check=True,
        stderr=subprocess.PIPE,
        text=True,
        encoding="utf-8",
    )


# Examples that should NOT trigger the rule
from foo import PIPE
subprocess.run(["foo"], stdout=PIPE, stderr=PIPE)
run(["foo"], stdout=None, stderr=PIPE)
