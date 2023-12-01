import os

os.getenv(1)  # [invalid-envvar-value]
os.getenv("a")
os.getenv("test")
os.getenv(key="testingAgain")
os.getenv(key=11)  # [invalid-envvar-value]
os.getenv(["hello"])  # [invalid-envvar-value]
os.getenv(key="foo", default="bar")
os.getenv(key=f"foo", default="bar")
os.getenv(key="foo" + "bar", default=1)
os.getenv(key=1 + "bar", default=1)  # [invalid-envvar-value]
os.getenv("PATH_TEST" if using_clear_path else "PATH_ORIG")
os.getenv(1 if using_clear_path else "PATH_ORIG")

AA = "aa"
os.getenv(AA)
