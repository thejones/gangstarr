"""Resolver index — cached static analysis for GraphQL query attribution.

Named after Gang Starr's "Index" — cataloging every resolver in the codebase
so we can point N+1 queries at the actual source, not middleware.py:67.

Usage::

    from gangstarr.resolver_index import get_index

    loc = get_index().lookup("ArtistType.albums")
    if loc:
        print(f"{loc['file']}:{loc['line']}")  # testapp/schema.py:10
"""

from __future__ import annotations

import json
import os
from dataclasses import dataclass
from pathlib import Path
from typing import Any

from gangstarr.gangstarr import camel_to_snake, scan_resolvers


@dataclass(frozen=True)
class ResolvedLocation:
    """A source location for a GraphQL resolver."""

    file: str
    line: int
    source: str
    kind: str  # "explicit" or "implicit"


class ResolverIndex:
    """Cached index mapping GraphQL TypeName.fieldName → source locations.

    Scans Python files under ``base_dir`` for GraphQL type classes
    (DjangoObjectType, ObjectType) and their resolver methods / field
    declarations.  Results are cached by ``(file_path, mtime)`` so only
    changed files are re-scanned.
    """

    def __init__(self, base_dir: str):
        self._base_dir = base_dir
        # Cache: path → (mtime, entries)
        self._file_cache: dict[str, tuple[float, list[dict]]] = {}
        # The merged index: "TypeName.fieldName" → ResolvedLocation
        self._index: dict[str, ResolvedLocation] = {}
        self._dirty = True

    def lookup(self, resolver_path: str) -> ResolvedLocation | None:
        """Look up a resolver path like 'ArtistType.albums'.

        Handles camelCase → snake_case matching automatically.
        For example, 'Query.artistsWithAlbumsAndTracks' will match
        'Query.artists_with_albums_and_tracks' in the index.
        """
        if self._dirty:
            self._rebuild()

        # Direct match first
        if resolver_path in self._index:
            return self._index[resolver_path]

        # Try camelCase → snake_case conversion on the field name
        parts = resolver_path.split('.', 1)
        if len(parts) == 2:
            type_name, field_name = parts
            snake_field = camel_to_snake(field_name)
            snake_key = f'{type_name}.{snake_field}'
            if snake_key in self._index:
                return self._index[snake_key]

        return None

    def invalidate(self) -> None:
        """Force a re-scan on next lookup."""
        self._dirty = True

    def _rebuild(self) -> None:
        """Scan application files and rebuild the index."""
        files_to_scan: list[dict[str, str]] = []

        for py_path in Path(self._base_dir).rglob('*.py'):
            path_str = str(py_path)

            # Skip common non-application directories
            if any(
                part in path_str
                for part in ('__pycache__', 'migrations', '.venv', 'node_modules')
            ):
                continue

            try:
                mtime = py_path.stat().st_mtime
            except OSError:
                continue

            # Check cache: skip unchanged files
            cached = self._file_cache.get(path_str)
            if cached and cached[0] == mtime:
                continue

            try:
                content = py_path.read_text(encoding='utf-8', errors='ignore')
            except OSError:
                continue

            # Quick filter: only scan files that mention ObjectType or DjangoObjectType
            if 'ObjectType' not in content:
                # Cache as empty so we don't re-read next time
                self._file_cache[path_str] = (mtime, [])
                continue

            rel_path = str(os.path.relpath(path_str, self._base_dir))
            files_to_scan.append({'path': rel_path, 'content': content})
            # Mark for cache update after scan
            self._file_cache[path_str] = (mtime, [])  # placeholder

        if files_to_scan:
            # Delegate heavy scanning to Rust
            result_json = scan_resolvers(json.dumps(files_to_scan))
            raw_index: dict[str, Any] = json.loads(result_json)

            # Merge into the main index
            for key, loc_dict in raw_index.items():
                self._index[key] = ResolvedLocation(
                    file=loc_dict['file'],
                    line=loc_dict['line'],
                    source=loc_dict['source'],
                    kind=loc_dict['kind'],
                )

        self._dirty = False


# Module-level singleton — built lazily on first use.
_singleton: ResolverIndex | None = None


def get_index() -> ResolverIndex:
    """Get the global ResolverIndex singleton.

    Lazily initialized using ``GANGSTAR_BASE_DIR`` from Django settings.
    """
    global _singleton
    if _singleton is None:
        from django.conf import settings

        _singleton = ResolverIndex(settings.GANGSTAR_BASE_DIR)
    return _singleton
