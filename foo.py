# Extra content following on the same line.
if True:
    import foo; x = 1
    import foo;     x = 1

# Extra content following on the next line.
if True:
    import foo; \
x = 1

# Extra content (including the semicolon) following on the next line.
if True:
    import foo \
        ; x = 1


# Extra content preceding on the same line.
if True:
    x = 1; import foo


# Extra content preceding on the same line.
if True:
    x = 1; \
         import foo


# Extra content (including the semicolon) preceding on the same line.
if True:
    x = 1 \
        ; import foo


# Extra content on both sides.
if True:
    x = 1; import foo; x = 1
    x = 1;     import foo;     x = 1

# Extra content following on the next line.
if True:
    x = 1; \
        import foo; \
    x = 1

# Extra content (including the semicolon) following on the next line.
if True:
    x = 1 \
        ;import foo \
        ;x = 1
