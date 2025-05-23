from markupsafe import Markup
from webhelpers.html import literal

content = "<script>alert('Hello, world!')</script>"
Markup(f"unsafe {content}")  # S704
literal(f"unsafe {content}")  # S704
