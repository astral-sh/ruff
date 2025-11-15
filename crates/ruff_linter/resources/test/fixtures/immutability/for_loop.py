"""RUF066 - Single-assignment missing Final.

Should NOT warn - variable is reassigned in the loop, so RUF066 does not apply.
"""

TOTAL = 0
for i in range(3):
    TOTAL += i
print(TOTAL)
