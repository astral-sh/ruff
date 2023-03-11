import socket

from kombu import Connection, exceptions

try:
    conn = Connection(settings.CELERY_BROKER_URL)
    conn.ensure_connection(max_retries=2)
    conn._close()
except (socket.error, exceptions.OperationalError):
    return HttpResponseServerError("cache: cannot connect to broker.")
