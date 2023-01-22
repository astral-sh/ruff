
if True:
    import foo1; x = 1
    import foo2;     x = 1

if True:
    import foo3; \
x = 1

if True:
    import foo4 \
        ; x = 1


if True:
    x = 1; import foo5


if True:
    x = 1; \
         import foo6


if True:
    x = 1 \
        ; import foo7


if True:
    x = 1; import foo8; x = 1
    x = 1;     import foo9;     x = 1

if True:
    x = 1; \
        import foo10; \
    x = 1

if True:
    x = 1 \
        ;import foo11 \
        ;x = 1


# Continuation, but not as the last content in the file.
x = 1; \
import foo12

# Continuation, followed by end-of-file. (Removing `import foo` would cause a syntax
# error.)
x = 1; \
import foo13
