import contextlib

# SIM115
f = open("foo.txt")
data = f.read()
f.close()

# OK
with open("foo.txt") as f:
    data = f.read()

# OK
with contextlib.ExitStack() as exit_stack:
    f = exit_stack.enter_context(open("filename"))

# OK
with contextlib.ExitStack() as stack:
    files = [stack.enter_context(open(fname)) for fname in filenames]
    close_files = stack.pop_all().close

# OK
with contextlib.AsyncExitStack() as exit_stack:
    f = await exit_stack.enter_async_context(open("filename"))

# OK (false negative)
with contextlib.ExitStack():
    f = exit_stack.enter_context(open("filename"))

# SIM115
with contextlib.ExitStack():
    f = open("filename")

# OK
with contextlib.ExitStack() as exit_stack:
    exit_stack_ = exit_stack
    f = exit_stack_.enter_context(open("filename"))
