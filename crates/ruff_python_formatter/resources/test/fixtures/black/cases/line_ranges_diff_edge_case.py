# flags: --line-ranges=10-11
# NOTE: If you need to modify this file, pay special attention to the --line-ranges=
# flag above as it's formatting specifically these lines.

# Reproducible example for https://github.com/psf/black/issues/4033.
# This can be fixed in the future if we use a better diffing algorithm, or make Black
# perform formatting in a single pass.

print ( "format me" )
print ( "format me" )
print ( "format me" )
print ( "format me" )
print ( "format me" )
