# RUF061
# Cases that should trigger the violation

foo.__dict__.get("__annotations__")  # RUF061
foo.__dict__.get("__annotations__", None)  # RUF061
foo.__dict__.get("__annotations__", {})  # RUF061

# Cases that should NOT trigger the violation

foo.__dict__.get("not__annotations__")  # RUF061
foo.__dict__.get("not__annotations__", None)  # RUF061
foo.__dict__.get("not__annotations__", {})  # RUF061
foo.__annotations__  # RUF061
foo.get("__annotations__")  # RUF061
foo.get("__annotations__", None)  # RUF061
foo.get("__annotations__", {})  # RUF061
