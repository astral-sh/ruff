from mytools import path as mypath
from . import views

# Test that custom path functions are also checked
urlpatterns_custom = [
    mypath("help", views.help_view),  # DJ014
    mypath("about", views.about_view),  # DJ014
]

# OK - custom path with trailing slash
urlpatterns_custom_ok = [
    mypath("help/", views.help_view),
    mypath("about/", views.about_view),
]

# Test that default django.urls.path still works
urlpatterns_default = [
    path("contact", views.contact_view),  # DJ014
    path("contact/", views.contact_ok),  # OK
]
