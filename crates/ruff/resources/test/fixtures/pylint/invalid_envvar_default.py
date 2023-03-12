import os

tempVar = os.getenv("TEST", 12)  # [invalid-envvar-default]
goodVar = os.getenv("TESTING", None)
dictVarBad = os.getenv("AAA", {"a", 7})  # [invalid-envvar-default]
print(os.getenv("TEST", False))  # [invalid-envvar-default]
os.getenv("AA", "GOOD")
os.getenv("B", Z)
