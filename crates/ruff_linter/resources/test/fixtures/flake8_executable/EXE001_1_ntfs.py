#!/usr/bin/python

if __name__ == '__main__':
    #EXE001 Checks for a shebang directive in a file that is not executable.
    executable = False
    shebang = True
    rule_should = "pass" # Rule is not executed on file systems which do not support the executable bit
    print(f'EXE001 should {rule_should}: Executable: {executable}, shebang present: {shebang}')
