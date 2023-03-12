import os

os.getenv(1)  # [invalid-envvar-value]
os.getenv("a")
os.getenv("test")
os.getenv(key="testingAgain")
os.getenv(key=11)  # [invalid-envvar-value]
os.getenv(["hello"])  # [invalid-envvar-value]
os.getenv(key="foo", default="bar")

AA = "aa"
os.getenv(AA)
