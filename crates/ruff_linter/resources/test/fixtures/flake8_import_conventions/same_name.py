def no_alias():
    from django.conf import settings


def conventional_alias():
    from django.conf import settings as settings


def unconventional_alias():
    from django.conf import settings as s
