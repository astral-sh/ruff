from subprocess import Popen, call, check_call, check_output, run

# Different Popen wrappers are checked.
a = input()
Popen(a, shell=False)
call(a, shell=False)
check_call(a, shell=False)
check_output(a, shell=False)
run(a, shell=False)

# Values that falsey values are treated as false.
Popen(a, shell=0)
Popen(a, shell=[])
Popen(a, shell={})
Popen(a, shell=None)

# Unknown values are treated as falsey.
Popen(a, shell=True if True else False)

# No value is also caught.
Popen(a)

# Literals are fine, they're trusted.
run("true")
Popen(["true"])

# Fine through assignments as well.
run(c := "true")
cmd = ["true"]
run(cmd)

# Imperfect literals cannot be trusted.
cmd = ["echo", input()]
run(cmd)
