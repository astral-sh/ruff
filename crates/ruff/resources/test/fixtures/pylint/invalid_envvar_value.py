import os

os.getenv(1) # [invalid-envvar-value]
os.getenv("a")
os.getenv('test')

os.getenv(["hello"]) # [invalid-envvar-value]

AA = "aa"
os.getenv(AA)