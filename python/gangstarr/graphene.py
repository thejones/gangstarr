"""DWYCK — Graphene middleware for resolver-level query attribution.

Named after the Gang Starr track "DWYCK" (feat. Nice & Smooth) — because
Gangstarr and Graphene need to work together nice and smooth.

Install in your Graphene schema or Django settings::

    GRAPHENE = {
        'MIDDLEWARE': ['gangstarr.graphene.DWYCKMiddleware'],
    }

Or pass directly when creating the schema view::

    GraphQLView.as_view(schema=schema, middleware=[DWYCKMiddleware()])

When active, every SQL query captured by Premier will include the resolver
path that triggered it, e.g. ``Query.artistsWithAlbumsAndTracks`` or
``ArtistType.albums``.
"""

from __future__ import annotations

import threading

# Thread-local storage for the current resolver context.
# Premier reads this when capturing query events.
_resolver_context = threading.local()


def get_resolver_path() -> str:
    """Return the current resolver path, or '' if not inside a resolver."""
    return getattr(_resolver_context, 'path', '')


def _set_resolver_path(path: str) -> None:
    _resolver_context.path = path


def _clear_resolver_path() -> None:
    _resolver_context.path = ''


class DWYCKMiddleware:
    """Graphene middleware that tracks the active resolver for query attribution.

    Wraps every field resolver and stashes the ``ParentType.field_name`` path
    in a thread-local so that Premier can attribute SQL queries to the specific
    GraphQL resolver that triggered them.

    The path is intentionally NOT restored after the resolver returns.  This is
    because Django ORM querysets are lazy — Graphene evaluates them *after* the
    resolver middleware exits (during result coercion).  Keeping the path set
    ensures that the SQL queries triggered by lazy evaluation are still
    attributed to the correct resolver.
    """

    def resolve(self, next, root, info, **args):
        parent_type = info.parent_type.name if info.parent_type else 'Unknown'
        field_name = info.field_name or 'unknown'
        _set_resolver_path(f'{parent_type}.{field_name}')
        return next(root, info, **args)
