__all__ = ["A", "B", "C"]

# Errors
__all__.append("D")
__all__.extend(["E", "Foo"])
__all__.remove("A")

# OK
__all__ += ["D"]
foo = ["Hello"]
foo.append("World")
foo.bar.append("World")
