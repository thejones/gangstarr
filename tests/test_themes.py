"""Tests for gangstarr.themes — color theme system."""

from gangstarr.themes import (
    THEME_NAMES,
    THEMES,
    ColorTheme,
    get_theme,
)


class TestThemeRegistry:
    def test_all_themes_registered(self):
        assert 'default' in THEMES
        assert '505' in THEMES
        assert 'retro' in THEMES
        assert 'muted' in THEMES

    def test_theme_names_match_keys(self):
        for name, theme in THEMES.items():
            assert theme.name == name

    def test_get_theme_default(self):
        theme = get_theme()
        assert theme.name == 'default'

    def test_get_theme_by_name(self):
        for name in THEME_NAMES:
            theme = get_theme(name)
            assert theme.name == name

    def test_get_theme_unknown_falls_back(self):
        theme = get_theme('nonexistent')
        assert theme.name == 'default'

    def test_get_theme_none_returns_default(self):
        assert get_theme(None).name == 'default'
        assert get_theme('').name == 'default'


class TestThemeCompleteness:
    """Every theme must have all semantic color roles populated."""

    REQUIRED_ROLES = [
        'error', 'warning', 'ok', 'bold', 'dim', 'reset',
        'discipline_error', 'discipline_warning', 'discipline_info',
    ]

    def test_all_themes_have_all_roles(self):
        for name, theme in THEMES.items():
            for role in self.REQUIRED_ROLES:
                value = getattr(theme, role)
                assert value, f"Theme '{name}' missing color for role '{role}'"

    def test_themes_are_frozen(self):
        """Themes should be immutable."""
        theme = get_theme('default')
        assert isinstance(theme, ColorTheme)
        try:
            theme.error = 'bad'  # type: ignore[misc]
            assert False, "Should not be able to mutate frozen dataclass"
        except AttributeError:
            pass


class TestThemeInFormatReport:
    """Themes integrate into _format_report without errors."""

    ANALYSIS = {
        'summary': {
            'total_queries': 5,
            'unique_queries': 2,
            'total_duration_ms': 10.0,
            'duplicate_groups': 1,
            'reads': 4,
            'writes': 1,
        },
        'groups': [
            {
                'fingerprint': 'abc',
                'normalized_sql': 'SELECT ...',
                'count': 3,
                'total_duration_ms': 6.0,
                'avg_duration_ms': 2.0,
                'p50_duration_ms': 1.5,
                'max_duration_ms': 3.0,
                'callsites': [{'file': 'views.py', 'line': 10, 'function': 'index', 'resolver_path': ''}],
            },
        ],
        'findings': [],
        'consolidated': [],
    }

    def test_format_report_each_theme(self):
        from gangstarr.reporting import _format_report
        from gangstarr.schemas import RequestContext

        ctx = RequestContext(method='GET', path='/api/test/')

        for name in THEME_NAMES:
            output = _format_report(self.ANALYSIS, ctx, color_theme=name)
            assert 'QUERY REPORT' in output
            assert '5 queries' in output

    def test_printing_options_color_theme(self):
        from gangstarr.reporting import PrintingOptions

        opts = PrintingOptions(color_theme='505')
        assert opts.color_theme == '505'

        opts_default = PrintingOptions()
        assert opts_default.color_theme == 'default'
