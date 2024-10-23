# Check for `flake8-commas` violation for a file containing syntax errors.
(
    *args
)

def foo[(param1='test', param2='test',):
    pass

