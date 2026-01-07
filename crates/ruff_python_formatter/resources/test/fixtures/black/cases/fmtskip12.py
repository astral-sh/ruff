with open("file.txt") as f:    content = f.read() # fmt: skip

# Ideally, only the last line would be ignored
# But ignoring only part of the asexpr_test causes a parse error
# Same with ignoring the asexpr_test without also ignoring the entire with_stmt
with open   (
    "file.txt"   ,
) as f:    content = f.read() # fmt: skip
