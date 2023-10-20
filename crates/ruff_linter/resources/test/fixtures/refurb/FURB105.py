# Errors.

print("")
print("", sep=",")
print("", end="bar")
print("", sep=",", end="bar")
print(sep="")
print("", sep="")
print("", "", sep="")
print("", "", sep="", end="")
print("", "", sep="", end="bar")
print("", sep="", end="bar")
print(sep="", end="bar")
print("", "foo", sep="")
print("foo", "", sep="")
print("foo", "", "bar", sep="")
print("", *args)
print("", *args, sep="")
print("", **kwargs)
print(sep="\t")

# OK.

print()
print("foo")
print("", "")
print("", "foo")
print("foo", "")
print("", "", sep=",")
print("", "foo", sep=",")
print("foo", "", sep=",")
print("foo", "", "bar", "", sep=",")
print("", "", **kwargs)
print(*args, sep=",")
