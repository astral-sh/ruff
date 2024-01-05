# Supported
x =    1  # fmt: skip
x =    1  # fmt: skip # reason
x =    1  # reason # fmt: skip

# Unsupported
x =    1  # fmt: skip reason
x =    1  # fmt: skip - reason
x =    1  # fmt: skip; noqa
