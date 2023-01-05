# ok
with open("/abc/tmp", "w") as f:
    f.write("def")

with open("/tmp/abc", "w") as f:
    f.write("def")

with open("/var/tmp/123", "w") as f:
    f.write("def")

with open("/dev/shm/unit/test", "w") as f:
    f.write("def")

# not ok by config
with open("/foo/bar", "w") as f:
    f.write("def")
