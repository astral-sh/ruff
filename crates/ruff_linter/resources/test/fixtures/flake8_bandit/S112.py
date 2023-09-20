try:
    pass
except Exception:
    continue

try:
    pass
except:
    continue

try:
    pass
except (Exception,):
    continue

try:
    pass
except (Exception, ValueError):
    continue

try:
    pass
except ValueError:
    continue

try:
    pass
except (ValueError,):
    continue
