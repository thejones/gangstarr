"""
JazzThing — Django settings reader for `gangstarr pg-royalty`.

Discovers the Postgres connection URL from Django's DATABASES['default']
setting so the user doesn't need to supply --db-url manually when running
from inside a Django project directory.

All logic here is read-only; we never touch the database.
"""
from __future__ import annotations

import re
import sys
from pathlib import Path
from typing import Optional
from urllib.parse import quote_plus


def _find_django_settings_module() -> Optional[str]:
    """
    Search upward from cwd for a manage.py to locate the Django project root,
    then look for DJANGO_SETTINGS_MODULE in the environment or fall back to
    common defaults.

    Search order:
    1. DJANGO_SETTINGS_MODULE environment variable
    2. manage.py source code
    3. .env file(s) next to manage.py
    4. wsgi.py / asgi.py in the project package
    5. pyproject.toml [tool.pytest.ini_options]
    6. pytest.ini / setup.cfg
    """
    import os

    # Honour an explicit env var first.
    env_module = os.environ.get("DJANGO_SETTINGS_MODULE")
    if env_module:
        return env_module

    # Walk upward looking for manage.py.
    cwd = Path.cwd()
    project_root: Optional[Path] = None
    for directory in [cwd, *cwd.parents]:
        manage = directory / "manage.py"
        if not manage.exists():
            continue

        project_root = directory

        # Try to extract DJANGO_SETTINGS_MODULE from manage.py source.
        found = _extract_settings_module(manage)
        if found:
            _ensure_on_sys_path(directory)
            return found

        break

    if project_root is None:
        return None

    _ensure_on_sys_path(project_root)

    # Check .env files next to manage.py.
    for env_name in (".env", ".env.local", ".env.dev", ".env.development"):
        env_file = project_root / env_name
        found = _extract_settings_from_env_file(env_file)
        if found:
            return found

    # Check wsgi.py / asgi.py in any immediate sub-package.
    for child in sorted(project_root.iterdir()):
        if not child.is_dir():
            continue
        for entry_name in ("wsgi.py", "asgi.py"):
            entry = child / entry_name
            found = _extract_settings_module(entry)
            if found:
                return found

    # Check pyproject.toml, pytest.ini, setup.cfg for DJANGO_SETTINGS_MODULE.
    for cfg_name in ("pyproject.toml", "pytest.ini", "setup.cfg"):
        cfg_path = project_root / cfg_name
        found = _extract_settings_from_config(cfg_path)
        if found:
            return found

    return None


def _ensure_on_sys_path(directory: Path) -> None:
    """Add *directory* to sys.path if it isn't already there."""
    root_str = str(directory)
    if root_str not in sys.path:
        sys.path.insert(0, root_str)


_SETTINGS_RE = re.compile(
    r"DJANGO_SETTINGS_MODULE.*?['\"]([A-Za-z_][\w]*(?:\.[A-Za-z_][\w]*)+)['\"]"
)


def _extract_settings_module(path: Path) -> Optional[str]:
    """Extract DJANGO_SETTINGS_MODULE from a Python source file."""
    if not path.is_file():
        return None
    try:
        content = path.read_text(encoding="utf-8", errors="ignore")
        for line in content.splitlines():
            if "DJANGO_SETTINGS_MODULE" not in line:
                continue
            m = _SETTINGS_RE.search(line)
            if m:
                return m.group(1)
    except OSError:
        pass
    return None


def _extract_settings_from_env_file(path: Path) -> Optional[str]:
    """Extract DJANGO_SETTINGS_MODULE from a .env file (KEY=value format)."""
    if not path.is_file():
        return None
    try:
        for line in path.read_text(encoding="utf-8", errors="ignore").splitlines():
            line = line.strip()
            if line.startswith("#") or "=" not in line:
                continue
            key, _, value = line.partition("=")
            if key.strip() == "DJANGO_SETTINGS_MODULE":
                value = value.strip().strip("'\"")
                if value:
                    return value
    except OSError:
        pass
    return None


def _extract_settings_from_config(path: Path) -> Optional[str]:
    """Extract DJANGO_SETTINGS_MODULE from pyproject.toml, pytest.ini, or setup.cfg."""
    if not path.is_file():
        return None
    try:
        content = path.read_text(encoding="utf-8", errors="ignore")
        for line in content.splitlines():
            if "DJANGO_SETTINGS_MODULE" not in line:
                continue
            m = _SETTINGS_RE.search(line)
            if m:
                return m.group(1)
    except OSError:
        pass
    return None


def discover_db_url() -> str:
    """
    Return a Postgres DSN derived from Django's DATABASES['default'] setting,
    or an empty string if the setting is unavailable or the engine is not Postgres.

    This function is called by the Rust CLI before dispatching pg-royalty so
    the user doesn't need to pass --db-url explicitly in Django projects.
    """
    import os

    settings_module = _find_django_settings_module()
    if settings_module:
        os.environ.setdefault("DJANGO_SETTINGS_MODULE", settings_module)

    dsm = os.environ.get("DJANGO_SETTINGS_MODULE")
    if not dsm:
        print(
            "warning: could not discover DJANGO_SETTINGS_MODULE — "
            "set it in the environment, manage.py, or a .env file, "
            "or pass --db-url explicitly",
            file=sys.stderr,
        )
        return ""

    try:
        import django
        # Only call setup() if Django isn't already configured.
        if not django.conf.settings.configured:
            django.setup()

        from django.conf import settings  # noqa: E402
        db = settings.DATABASES.get("default", {})
    except Exception as exc:
        print(
            f"warning: could not load Django settings ({dsm}): {exc}",
            file=sys.stderr,
        )
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
