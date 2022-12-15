
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
