"""RUF066 - Single-assignment missing Final.

Should NOT warn - Unpacked assignments (e.g., a, b = ...) are excluded from this rule.
"""

a, b = (1, 2)
print(a, b)
