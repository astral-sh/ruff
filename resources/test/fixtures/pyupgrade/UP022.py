import subprocess

output = subprocess.run(["foo"], stdout=subprocess.PIPE, stderr=subprocess.PIPE)

output = subprocess.run(
    ["foo"], stdout=subprocess.PIPE, check=True, stderr=subprocess.PIPE
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
