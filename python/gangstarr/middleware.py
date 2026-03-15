from __future__ import annotations

import json
import os
import re
import sys
import threading
from pathlib import Path

from django.conf import settings

from .context_manager import full_clip

# Default paths to exclude from profiling
DEFAULT_EXCLUDE_PATHS: list[str] = [
    '/static/',
    '/favicon.ico',
    '/media/',
    '/__debug__/',
]

# Simple regex to extract the operation type and name from a GraphQL query string.
# Matches: query MyQuery { ... } or mutation CreateArtist { ... }
_GQL_OP_RE = re.compile(
    r'\b(query|mutation|subscription)\s+([A-Za-z_]\w*)',
)


class MomentOfTruthMiddleware:
    """Moment of Truth — automatic request-level SQL profiling.

    Only active when DEBUG is True (or GANGSTARR_ALWAYS_ON is set).

    Settings:
        GANGSTARR_EXCLUDE_PATHS: list[str]
            Path prefixes to skip profiling. Default includes /static/,
            /favicon.ico, /media/, /__debug__/.
        GANGSTARR_ALWAYS_ON: bool
            If True, profile even when DEBUG is False.
    """

    def __init__(self, get_response):
        self.get_response = get_response
        self._exclude_paths: list[str] = getattr(
            settings, 'GANGSTARR_EXCLUDE_PATHS', DEFAULT_EXCLUDE_PATHS
        )

    def __call__(self, request):
        if not self._should_profile():
            return self.get_response(request)

        if self._is_excluded(request.path):
            return self.get_response(request)

        view_name = self._resolve_view_name(request)
        operation_name, operation_type = self._extract_graphql_info(request)

        meta_data = dict(
            url=request.path,
            method=request.method,
            view_name=view_name,
            operation_name=operation_name,
            operation_type=operation_type,
        )

        with full_clip(meta_data=meta_data) as fc:
            response = self.get_response(request)
            fc._premier.request_context.status_code = response.status_code
            fc._premier.request_context.view_name = view_name
            fc._premier.request_context.operation_name = operation_name
            fc._premier.request_context.operation_type = operation_type

        return response

    def _is_excluded(self, path: str) -> bool:
        return any(path.startswith(prefix) for prefix in self._exclude_paths)

    @staticmethod
    def _should_profile() -> bool:
        if getattr(settings, 'GANGSTARR_ALWAYS_ON', False):
            return True
        return getattr(settings, 'DEBUG', False)

    @staticmethod
    def _resolve_view_name(request) -> str:
        try:
            from django.urls import resolve
            match = resolve(request.path)
            if match.func:
                func = match.func
                if hasattr(func, 'view_class'):
                    return func.view_class.__name__
                return getattr(func, '__name__', str(func))
        except Exception:
            pass
        return ''

    @staticmethod
    def _extract_graphql_info(request) -> tuple[str, str]:
        """Extract operation name and type from a GraphQL request.

        Handles both JSON body (standard) and form-encoded (GraphiQL) payloads.
        Returns (operation_name, operation_type) or ('', '') if not GraphQL.
        """
        if request.method != 'POST':
            return '', ''

        content_type = request.content_type or ''
        body_data: dict = {}

        try:
            if 'application/json' in content_type:
                body_data = json.loads(request.body)
            elif 'application/x-www-form-urlencoded' in content_type:
                body_data = {
                    'query': request.POST.get('query', ''),
                    'operationName': request.POST.get('operationName', ''),
                }
            elif 'multipart/form-data' in content_type:
                body_data = {
                    'query': request.POST.get('query', ''),
                    'operationName': request.POST.get('operationName', ''),
                }
            else:
                return '', ''
        except (json.JSONDecodeError, ValueError, UnicodeDecodeError):
            return '', ''

        if not isinstance(body_data, dict):
            return '', ''

        # 1. Check the explicit operationName field
        op_name = body_data.get('operationName') or ''

        # 2. Parse the query string for operation type and name
        query_str = body_data.get('query', '')
        op_type = ''
        if query_str:
            match = _GQL_OP_RE.search(query_str)
            if match:
                op_type = match.group(1)  # query | mutation | subscription
                if not op_name:
                    op_name = match.group(2)

        # If we found a name but no type, default to 'query'
        if op_name and not op_type:
            op_type = 'query'

        return op_name, op_type
