__all__ = ["A", "B", "C"]

__all__.append("D")
__all__.extend(["E", "Foo"])
__all__.remove("A")

# these are ok
__all__ += ["D"]
l = ["Hello"]
l.append("World")
