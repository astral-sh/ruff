"""RUF066 - Single-assignment missing Final.

Should warn - A name assigned exactly once at module scope should be annotated
with Final so readers and tools can treat it as immutable.
"""

X = 1
print(X)
