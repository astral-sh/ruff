if __name__ == '__main__':
    #EXE002 Checks for executable .py files that do not have a shebang.
    executable = True
    shebang = False
    rule_should = "pass" # Rule is not executed on file systems which do not support the executable bit
    print(f'EXE002 should {rule_should}: Executable: {executable}, shebang present: {shebang}')
