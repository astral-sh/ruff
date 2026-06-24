# SEE PEP 758 FOR MORE DETAILS
# remains unchanged
try:
    pass
except:
    pass

# remains unchanged
try:
    pass
except ValueError:
    pass

try:
    pass
except* ValueError:
    pass

# parenthesis are removed
try:
    pass
except ValueError:
    pass

try:
    pass
except* ValueError:
    pass

# parenthesis are removed
try:
    pass
except ValueError as e:
    pass

try:
    pass
except* ValueError as e:
    pass

# remains unchanged
try:
    pass
except (ValueError,):
    pass

try:
    pass
except* (ValueError,):
    pass

# remains unchanged
try:
    pass
except (ValueError,) as e:
    pass

try:
    pass
except* (ValueError,) as e:
    pass

# parenthesis are not removed
try:
    pass
except (ValueError, TypeError, KeyboardInterrupt):
    pass

try:
    pass
except* (ValueError, TypeError, KeyboardInterrupt):
    pass

# parenthesis are not removed
try:
    pass
except (ValueError, TypeError, KeyboardInterrupt) as e:
    pass

try:
    pass
except* (ValueError, TypeError, KeyboardInterrupt) as e:
    pass

# parenthesis are removed
try:
    pass
except ValueError if True else TypeError:
    pass

try:
    pass
except* ValueError if True else TypeError:
    pass

# parenthesis are not removed
try:
    try:
        pass
    except (TypeError, KeyboardInterrupt):
        pass
except (ValueError,):
    pass

try:
    try:
        pass
    except* (TypeError, KeyboardInterrupt):
        pass
except* (ValueError,):
    pass
