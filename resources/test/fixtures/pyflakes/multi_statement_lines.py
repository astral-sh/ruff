
if True:
    import foo; x = 1
    import foo;     x = 1

if True:
    import foo; \
x = 1

if True:
    import foo \
        ; x = 1


if True:
    x = 1; import foo


if True:
    x = 1; \
         import foo


if True:
    x = 1 \
        ; import foo


if True:
    x = 1; import foo; x = 1
    x = 1;     import foo;     x = 1

if True:
    x = 1; \
        import foo; \
    x = 1

if True:
    x = 1 \
        ;import foo \
        ;x = 1


# Continuation, but not as the last content in the file.
x = 1; \
import foo

# Continuation, followed by end-of-file. (Removing `import foo` would cause a syntax
# error.)
x = 1; \
import foo