import flask
from markupsafe import Markup, escape

content = "<script>alert('Hello, world!')</script>"
Markup(f"unsafe {content}")  # MS001
flask.Markup("unsafe {}".format(content))  # MS001
Markup("safe {}").format(content)
flask.Markup(b"safe {}", encoding='utf-8').format(content)
escape(content)
Markup(content)  # MS001
