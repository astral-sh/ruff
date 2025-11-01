"""Minimal Django settings for type checking tests."""

SECRET_KEY = "test-secret-key"
INSTALLED_APPS = []
DATABASES = {
    "default": {
        "ENGINE": "django.db.backends.sqlite3",
        "NAME": ":memory:",
    }
}
