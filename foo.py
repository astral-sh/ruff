from django_stubs_ext import ValuesQuerySet

# This file needs to be different from cache.py because cache.py
# cannot import anything from zerver.models or we'd have an import
# loop
from analytics.models import RealmCount
from zerver.lib.cache import (
    cache_set_many,
    get_remote_cache_requests,
    get_remote_cache_time,
    get_stream_cache_key,
    user_profile_by_api_key_cache_key,
    user_profile_cache_key,
)
