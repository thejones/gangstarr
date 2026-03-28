from __future__ import annotations

import json
import re
import time

from django.conf import settings

from .context_manager import full_clip
from .discipline import (
    Discipline,
    RequestTrace,
    compute_request_fingerprint,
    hash_graphql_variables,
    resolve_client_fingerprint,
)

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
        operation_name, operation_type, body_data = self._extract_graphql_info(request)

        # Discipline: compute fingerprints before the request executes.
        client_fp, client_fp_source = resolve_client_fingerprint(request)
        variables_hash = hash_graphql_variables(body_data)
        request_fp = compute_request_fingerprint(
            method=request.method,
            path=request.path,
            operation_name=operation_name,
            variables_hash=variables_hash,
        )

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
            fc._premier.request_context.client_fingerprint = client_fp
            fc._premier.request_context.client_fp_source = client_fp_source
            fc._premier.request_context.request_fingerprint = request_fp

        # Discipline: register the trace and check for cross-request duplicates.
        sql_fps = []
        analysis = fc.reporter._run_analysis() if hasattr(fc.reporter, '_run_analysis') else None
        if analysis:
            sql_fps = [g['fingerprint'] for g in analysis.get('groups', [])]

        trace = RequestTrace(
            timestamp=time.monotonic(),
            client_fingerprint=client_fp,
            client_fp_source=client_fp_source,
            request_fingerprint=request_fp,
            request_id=fc._premier.request_context.request_id,
            method=request.method,
            path=request.path,
            operation_name=operation_name,
            operation_type=operation_type,
            sql_fingerprints=sql_fps,
            total_queries=analysis['summary']['total_queries'] if analysis else 0,
            total_duration_ms=analysis['summary']['total_duration_ms'] if analysis else 0.0,
        )
        discipline_findings = Discipline.register(trace)

        if discipline_findings:
            self._report_discipline_findings(discipline_findings)

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
    def _report_discipline_findings(findings) -> None:
        """Print cross-request duplicate findings to the console."""
        from gangstarr.themes import get_theme

        theme_name = getattr(settings, 'GANGSTARR_COLOR_THEME', None)
        # Also check the reporting options if set
        if not theme_name:
            opts = getattr(settings, 'GANGSTAR_REPORTING_OPTIONS', None)
            if opts and hasattr(opts, 'color_theme'):
                theme_name = opts.color_theme
        theme = get_theme(theme_name)

        for f in findings:
            if f.severity == 'error':
                color = theme.discipline_error
            elif f.severity == 'warning':
                color = theme.discipline_warning
            else:
                color = theme.discipline_info
            print(
                f"{color}{theme.bold}[{f.code}] {f.title}{theme.reset}  "
                f"{color}{f.message}{theme.reset}"
            )

    @staticmethod
    def _extract_graphql_info(request) -> tuple[str, str, dict | None]:
        """Extract operation name and type from a GraphQL request.

        Handles both JSON body (standard) and form-encoded (GraphiQL) payloads.
        Returns (operation_name, operation_type, body_data) where body_data is
        the parsed body dict (or None if not GraphQL). The body_data is used
        downstream to hash GraphQL variables for request fingerprinting.
        """
        if request.method != 'POST':
            return '', '', None

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
                return '', '', None
        except (json.JSONDecodeError, ValueError, UnicodeDecodeError):
            return '', '', None

        if not isinstance(body_data, dict):
            return '', '', None

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

        return op_name, op_type, body_data
