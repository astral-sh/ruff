try:  # OK
    print()
except Exception:
    raise


try:  # OK
    print()
    print()
    print()
    print()
    print()
except Exception:
    raise

try:  # OK
    print()
except Exception:
    print()
    print()
    print()
    print()
    print()
    print()
    print()
    print()
    raise

try:  # OK
    print()
finally:
    print()
    print()
    print()
    print()
    print()
    print()
    print()
    print()

try:  # OK
    print()
except Exception:
    print()
    print()
    print()
    print()
    print()
    print()
    print()
    print()
    raise
finally:
    print()
    print()
    print()
    print()
    print()
    print()
    print()
    print()

try:  # Too many try statements (6/5)
    print()
    print()
    print()
    print()
    print()
    print()
except Exception:
    raise
