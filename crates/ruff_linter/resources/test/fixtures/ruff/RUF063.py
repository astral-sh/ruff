# RUF063
# Cases that should trigger the violation

foo.__dict__.get("__annotations__")  # RUF063
foo.__dict__.get("__annotations__", None)  # RUF063
foo.__dict__.get("__annotations__", {})  # RUF063
foo.__dict__["__annotations__"]  # RUF063

# Cases that should NOT trigger the violation

foo.__dict__.get("not__annotations__")
foo.__dict__.get("not__annotations__", None)
foo.__dict__.get("not__annotations__", {})
foo.__dict__["not__annotations__"]
foo.__annotations__
foo.get("__annotations__")
foo.get("__annotations__", None)
foo.get("__annotations__", {})
