## In preview, references to the `exec` builtin are flagged, not just calls.
## https://github.com/astral-sh/ruff/issues/28011

# Error (call)
exec('x = 2')

# Error (reference passed to map)
list(map(exec, ['x = 2']))

# Error (bare reference)
x = exec

# Error (bare load, no call)
exec

# No error: a same-named attribute on another object is not the builtin
foo.exec

# No error: a locally bound name shadows the builtin
def fn():
    from builtins import exec as exec
    exec('')  # Error (still resolves to the builtin import)
