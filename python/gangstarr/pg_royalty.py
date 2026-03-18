"""
JazzThing — Django settings reader for `gangstarr pg-royalty`.

Discovers the Postgres connection URL from Django's DATABASES['default']
setting so the user doesn't need to supply --db-url manually when running
from inside a Django project directory.

All logic here is read-only; we never touch the database.
"""
from __future__ import annotations

import sys
from pathlib import Path
from typing import Optional
from urllib.parse import quote_plus


def _find_django_settings_module() -> Optional[str]:
    """
    Search upward from cwd for a manage.py to locate the Django project root,
    then look for DJANGO_SETTINGS_MODULE in the environment or fall back to
    common defaults.
    """
    import os

    # Honour an explicit env var first.
    env_module = os.environ.get("DJANGO_SETTINGS_MODULE")
    if env_module:
        return env_module

    # Walk upward looking for manage.py.
    cwd = Path.cwd()
    for directory in [cwd, *cwd.parents]:
        manage = directory / "manage.py"
        if not manage.exists():
            continue

        # Try to extract DJANGO_SETTINGS_MODULE from manage.py source.
        try:
            content = manage.read_text(encoding="utf-8", errors="ignore")
            for line in content.splitlines():
                if "DJANGO_SETTINGS_MODULE" in line and ("'" in line or '"' in line):
                    # Extract the quoted module name.
                    for delim in ('"', "'"):
                        parts = line.split(delim)
                        for p in parts:
                            if "." in p and not p.startswith("-"):
                                return p
        except OSError:
            pass

        # Ensure the project root is on sys.path so Django can import settings.
        root_str = str(directory)
        if root_str not in sys.path:
            sys.path.insert(0, root_str)

        break

    return None


def discover_db_url() -> str:
    """
    Return a Postgres DSN derived from Django's DATABASES['default'] setting,
    or an empty string if the setting is unavailable or the engine is not Postgres.

    This function is called by the Rust CLI before dispatching pg-royalty so
    the user doesn't need to pass --db-url explicitly in Django projects.
    """
    settings_module = _find_django_settings_module()
    if settings_module:
        import os
        os.environ.setdefault("DJANGO_SETTINGS_MODULE", settings_module)

    try:
        import django
        # Only call setup() if Django isn't already configured.
        if not django.conf.settings.configured:
            django.setup()

        from django.conf import settings  # noqa: E402
        db = settings.DATABASES.get("default", {})
    except Exception:
        return ""

    engine: str = db.get("ENGINE", "")
    if "postgresql" not in engine and "postgis" not in engine:
        # Not a Postgres backend — nothing to inject.
        return ""

    host = db.get("HOST", "localhost") or "localhost"
    port = db.get("PORT", "5432") or "5432"
    name = db.get("NAME", "")
    user = db.get("USER", "")
    password = db.get("PASSWORD", "")

    if not name:
        return ""

    # Build a safe DSN — encode special characters in user/password.
    if user and password:
        auth = f"{quote_plus(str(user))}:{quote_plus(str(password))}@"
    elif user:
        auth = f"{quote_plus(str(user))}@"
    else:
        auth = ""

    return f"postgresql://{auth}{host}:{port}/{name}"
