from webhelpers.html import literal

# NOTE: This test case exists to make sure our optimization doesn't cause
#       additional markup names to be skipped if we don't import either
#       markupsafe or flask first.
content = "<script>alert('Hello, world!')</script>"
literal(f"unsafe {content}")  # S704
