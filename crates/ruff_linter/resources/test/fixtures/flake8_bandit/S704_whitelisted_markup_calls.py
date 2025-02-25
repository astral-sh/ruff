from bleach import clean
from markupsafe import Markup

content = "<script>alert('Hello, world!')</script>"
Markup(clean(content))

# indirect assignments are currently not supported
cleaned = clean(content)
Markup(cleaned)  # S704
