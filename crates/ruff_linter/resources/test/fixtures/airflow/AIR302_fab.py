from __future__ import annotations

from airflow.api.auth.backend.basic_auth import (
    CLIENT_AUTH,
    auth_current_user,
    init_app,
    requires_authentication,
)

CLIENT_AUTH
init_app()
auth_current_user()
requires_authentication()

from airflow.api.auth.backend.kerberos_auth import (
    CLIENT_AUTH,
    find_user,
    init_app,
    log,
    requires_authentication,
)

log()
CLIENT_AUTH
find_user()
init_app()
requires_authentication()

from airflow.auth.managers.fab.api.auth.backend.kerberos_auth import (
    CLIENT_AUTH,
    find_user,
    init_app,
    log,
    requires_authentication,
)

log()
CLIENT_AUTH
find_user()
init_app()
requires_authentication()

from airflow.auth.managers.fab.fab_auth_manager import FabAuthManager
from airflow.auth.managers.fab.security_manager.override import (
    MAX_NUM_DATABASE_USER_SESSIONS,
    FabAirflowSecurityManagerOverride,
)

FabAuthManager()
MAX_NUM_DATABASE_USER_SESSIONS
FabAirflowSecurityManagerOverride()

from airflow.www.security import FabAirflowSecurityManagerOverride

FabAirflowSecurityManagerOverride()
