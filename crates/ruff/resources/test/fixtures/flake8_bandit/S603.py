from subprocess import Popen, call, check_call, check_output, run

# Different Popen wrappers are checked.
Popen("true", shell=False)
call("true", shell=False)
check_call("true", shell=False)
check_output("true", shell=False)
run("true", shell=False)

# Values that falsey values are treated as false.
Popen("true", shell=0)
Popen("true", shell=[])
Popen("true", shell={})
Popen("true", shell=None)

# Unknown values are treated as falsey.
Popen("true", shell=True if True else False)

# No value is also caught.
Popen("true")
