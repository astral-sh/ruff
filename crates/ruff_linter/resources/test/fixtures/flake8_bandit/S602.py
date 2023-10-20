from subprocess import Popen, call, check_call, check_output, run

# Check different Popen wrappers are checked.
Popen("true", shell=True)
call("true", shell=True)
check_call("true", shell=True)
check_output("true", shell=True)
run("true", shell=True)

# Check values that truthy values are treated as true.
Popen("true", shell=1)
Popen("true", shell=[1])
Popen("true", shell={1: 1})
Popen("true", shell=(1,))

# Check command argument looks unsafe.
var_string = "true"
Popen(var_string, shell=True)
Popen([var_string], shell=True)
Popen([var_string, ""], shell=True)
