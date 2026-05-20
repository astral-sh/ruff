# This py314+ module provides annotations for `sys._jit`.
# It's named `sys.__jit` in typeshed,
# because trying to import `sys._jit` will fail at runtime!
# At runtime, `sys._jit` has the unusual status
# of being a `types.ModuleType` instance that cannot be directly imported,
# (same as sys.monitoring)
# and exists in the `sys`-module namespace despite `sys` not being a package.

def is_available() -> bool: ...
def is_enabled() -> bool: ...
def is_active() -> bool: ...
