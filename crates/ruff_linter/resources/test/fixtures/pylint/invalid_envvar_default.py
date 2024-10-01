import os

tempVar = os.getenv("TEST", 12)  # [invalid-envvar-default]
goodVar = os.getenv("TESTING", None)
dictVarBad = os.getenv("AAA", {"a", 7})  # [invalid-envvar-default]
print(os.getenv("TEST", False))  # [invalid-envvar-default]
os.getenv("AA", "GOOD")
os.getenv("AA", f"GOOD")
os.getenv("AA", "GOOD" + "BAR")
os.getenv("AA", "GOOD" + 1)
os.getenv("AA", "GOOD %s" % "BAR")
os.getenv("B", Z)
os.getenv("AA", "GOOD" if Z else "BAR")
os.getenv("AA", 1 if Z else "BAR")  # [invalid-envvar-default]
