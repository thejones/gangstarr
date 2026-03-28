"""Color themes for gangstarr console output.

Each theme maps semantic roles to ANSI escape sequences. Themes can be
selected via ``PrintingOptions(color_theme='505')`` in the Django app or
``--colours retro`` on the CLI.

Available themes:
    default  — classic ANSI red/yellow/green
    505      — New Mexico chile: yellow, turquoise, tan, red, green
    retro    — Orange Moon (Figma): burnt orange, deep red, teal, peach
    muted    — soft pastel tones
"""

from __future__ import annotations

from dataclasses import dataclass

# ── Helpers ───────────────────────────────────────────────────────────────────

BOLD = "\033[1m"
DIM = "\033[2m"
RESET = "\033[0m"


def _fg256(n: int) -> str:
    """256-color foreground."""
    return f"\033[38;5;{n}m"


def _fg_rgb(r: int, g: int, b: int) -> str:
    """True-color (24-bit) foreground."""
    return f"\033[38;2;{r};{g};{b}m"


# ── Theme dataclass ───────────────────────────────────────────────────────────


@dataclass(frozen=True)
class ColorTheme:
    """Semantic color roles used across gangstarr console output."""

    name: str

    # Report chrome
    bold: str = BOLD
    dim: str = DIM
    reset: str = RESET

    # Severity colors (query report + consolidated findings)
    error: str = ""       # HOT callsites, excessive duration
    warning: str = ""     # N+1, repeated query groups
    ok: str = ""          # healthy / normal rows
    accent: str = ""      # special highlights (GraphQL header, etc.)

    # Discipline (cross-request tracing)
    discipline_error: str = ""    # 4x+ duplicates
    discipline_warning: str = ""  # 2-3x duplicates
    discipline_info: str = ""     # informational


# ── Built-in themes ──────────────────────────────────────────────────────────

DEFAULT = ColorTheme(
    name='default',
    error="\033[31m",            # red
    warning="\033[33m",          # yellow
    ok="\033[32m",               # green
    accent="\033[36m",           # cyan
    discipline_error=_fg256(204),    # hot pink
    discipline_warning=_fg256(210),  # salmon
    discipline_info=_fg256(218),     # light pink
)

THEME_505 = ColorTheme(
    name='505',
    # New Mexico chile palette: yellow, turquoise, tan, red, green
    error=_fg256(160),               # chile red
    warning=_fg256(220),             # golden yellow
    ok=_fg256(34),                   # hatch green chile
    accent=_fg256(80),               # turquoise (NM state gem)
    dim=_fg256(180),                 # desert tan
    discipline_error=_fg256(160),    # chile red
    discipline_warning=_fg256(220),  # golden yellow
    discipline_info=_fg256(80),      # turquoise
)

THEME_RETRO = ColorTheme(
    name='retro',
    # Orange Moon palette (Figma): #ED6B47 #D5422D #085075 #354354 #FCAC92
    error=_fg_rgb(213, 66, 45),       # deep red    #D5422D
    warning=_fg_rgb(237, 107, 71),    # burnt orange #ED6B47
    ok=_fg_rgb(8, 80, 117),           # deep teal   #085075
    accent=_fg_rgb(252, 172, 146),    # peach        #FCAC92
    dim=_fg_rgb(53, 67, 84),          # slate        #354354
    discipline_error=_fg_rgb(213, 66, 45),
    discipline_warning=_fg_rgb(237, 107, 71),
    discipline_info=_fg_rgb(252, 172, 146),
)

THEME_MUTED = ColorTheme(
    name='muted',
    # Soft pastel tones
    error=_fg256(174),               # dusty rose
    warning=_fg256(180),             # soft wheat
    ok=_fg256(151),                  # sage green
    accent=_fg256(146),              # lavender grey
    dim=_fg256(245),                 # soft grey
    discipline_error=_fg256(174),    # dusty rose
    discipline_warning=_fg256(180),  # soft wheat
    discipline_info=_fg256(146),     # lavender grey
)

# ── Registry ─────────────────────────────────────────────────────────────────

THEMES: dict[str, ColorTheme] = {
    'default': DEFAULT,
    '505': THEME_505,
    'retro': THEME_RETRO,
    'muted': THEME_MUTED,
}

THEME_NAMES = list(THEMES.keys())


def get_theme(name: str | None = None) -> ColorTheme:
    """Resolve a theme by name, falling back to default."""
    if not name:
        return DEFAULT
    return THEMES.get(name, DEFAULT)
