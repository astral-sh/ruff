# These SHOULD be detected
try:
    ...
except CustomException:
    pass
except Exception:
    pass

try:
    ...
except Exception:
    ...

try:
    ...
except Exception:
    Ellipsis

try:
    ...
except (Exception, ValueError):
    Ellipsis

try:
    ...
except (ValueError, IndexError, Exception):
    Ellipsis

# These should NOT be detected
try:
    ...
except Exception:
    print("hello")

try:
    ...
except Exception:
    print("hello")
    pass

try:
    ...
except Exception:
    if "hello":
        pass
