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

AA = "aa"
os.getenv(AA)
