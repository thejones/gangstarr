# AI-Powered Analysis via Kiro — `gangstarr --steeze`

> The goal: run `gangstarr check`, then `gangstarr --steeze --kiro` and walk away.
> Kiro reads the SQLite DB, reviews findings, cross-references runtime data, and delivers results — a terminal report, a branch with fixes, or both.

---

## Why This Matters

Gangstarr already captures static findings and runtime evidence in `.gangstarr/gangstarr.db`. But interpreting those findings, prioritizing them, and writing the actual fixes is still manual. An AI agent that can read the DB, understand the codebase, and act on findings turns gangstarr from a reporting tool into an automated performance remediation pipeline.

---

## Two Kiro Integration Paths

### Path 1: Kiro CLI (Local, Interactive or Scripted)

The Kiro CLI runs locally in your terminal. It supports custom agents, steering files, hooks, and can read/write files + execute shell commands. This is the primary integration path.

- Docs: https://kiro.dev/docs/cli/
- Custom agents: https://kiro.dev/docs/cli/custom-agents/
- Steering: https://kiro.dev/docs/cli/steering/
- Hooks: https://kiro.dev/docs/cli/hooks/

### Path 2: Kiro Autonomous Agent (Remote, Async via GitHub)

The autonomous agent runs in a cloud sandbox, clones your repo, and opens PRs. It's triggered from `app.kiro.dev/agent` or GitHub issues. Good for CI-triggered remediation.

- Docs: https://kiro.dev/docs/autonomous-agent/
- Requires: GitHub app installed, Kiro Pro/Pro+/Power plan
- Currently in preview, up to 10 concurrent tasks

---

## Path 1: Kiro CLI Custom Agent (Primary)

### What We Ship in the Repo

```
.kiro/
├── steering/
│   └── rules.md                  # Already exists — project conventions
├── agents/
│   └── steeze.json               # The gangstarr analysis agent
└── steering/
    └── steeze-prompt.md          # Detailed prompt for the agent
```

Plus the `AGENTS.md` at repo root (already exists) — Kiro always loads this.

### The Custom Agent: `.kiro/agents/steeze.json`

```json
{
  "name": "steeze",
  "description": "Gangstarr AI analysis agent — reads findings from .gangstarr/gangstarr.db, reviews source code, and produces fixes or a prioritized report.",
  "prompt": "file://../../.kiro/steering/steeze-prompt.md",
  "tools": ["read", "write", "shell", "grep", "glob", "code"],
  "allowedTools": ["read", "shell", "grep", "glob", "code"],
  "resources": [
    "file://AGENTS.md",
    "file://README.md",
    "file://.kiro/steering/**/*.md"
  ],
  "model": "claude-sonnet-4",
  "welcomeMessage": "Steeze mode activated. Point me at your gangstarr findings."
}
```

Key decisions:
- `write` is in `tools` but NOT in `allowedTools` — the agent must ask before modifying files (safety net)
- `shell` is allowed so it can run `sqlite3` queries against the DB without prompting
- All steering files are loaded as resources so the agent knows project conventions

### The Prompt: `.kiro/steering/steeze-prompt.md`

This is the brain of the operation. It should instruct the agent to:

```markdown
# Steeze — Gangstarr AI Analysis Agent

You are the "steeze" agent for the gangstarr project. Your job is to analyze
Django ORM performance findings and produce actionable fixes.

## Your Workflow

1. **Read the database** — Run `sqlite3 .gangstarr/gangstarr.db` to query:
   - `SELECT * FROM runs ORDER BY created_at DESC LIMIT 5;` — recent runs
   - `SELECT * FROM static_findings WHERE run_id = '<latest>';` — static findings
   - `SELECT * FROM runtime_findings WHERE run_id = '<latest>';` — runtime evidence
   - `SELECT * FROM correlations WHERE run_id = '<latest>';` — cross-referenced hits

2. **Prioritize** — Focus on findings that have BOTH static and runtime evidence.
   These are confirmed problems, not theoretical. Sort by:
   - Correlated findings first (static + runtime match)
   - High query count / high duration runtime findings
   - Then remaining static-only findings

3. **Read the source** — For each finding, read the referenced file and line.
   Understand the Django model relationships, queryset usage, and context.

4. **Produce fixes** — For each actionable finding:
   - Explain the problem in one sentence
   - Show the fix (select_related, prefetch_related, .only(), .count(), etc.)
   - If the fix requires model changes, note that

5. **Output options** (based on user request):
   - **Report mode** (default): Print a structured terminal report
   - **Branch mode** (`--branch`): Create a git branch, apply fixes, commit

## Rules
- Never modify test files
- Follow the Gang Starr naming convention for any new code
- Keep fixes minimal — one concern per change
- If a finding is ambiguous, flag it for human review instead of guessing
```

### How the User Runs It

**Interactive (simplest):**
```bash
# First, run the analysis
gangstarr check path/to/myproject/

# Then launch Kiro with the steeze agent
kiro-cli --agent steeze
# Agent auto-loads, reads DB, starts working
```

**Scripted (hands-off):**
```bash
gangstarr check path/to/myproject/ && \
  echo "Analyze the latest gangstarr run. Report mode." | kiro-cli --agent steeze
```

> Note: As of current Kiro CLI docs, there isn't a documented `--prompt` or stdin-pipe mode for fully non-interactive execution. The scripted approach above may need adjustment based on future Kiro CLI features. Check `kiro-cli --help` for the latest.

### Future: `gangstarr --steeze --kiro` as a Native Subcommand

Once the agent config is proven, we add a thin wrapper in `src/cli.rs`:

```rust
"steeze" => {
    // Run check first if DB is stale
    // Then exec kiro-cli with the steeze agent
    let status = std::process::Command::new("kiro-cli")
        .args(["--agent", "steeze"])
        .status();
    match status {
        Ok(s) => s.code().unwrap_or(1),
        Err(e) => {
            eprintln!("error: could not launch kiro-cli: {}", e);
            eprintln!("Install: curl -fsSL https://cli.kiro.dev/install | bash");
            2
        }
    }
}
```

Usage becomes:
```bash
gangstarr check src/ && gangstarr steeze
# or eventually:
gangstarr --steeze --kiro
```

---

## Path 2: Kiro Autonomous Agent (CI / Async)

For teams that want this in CI or as a background process:

1. Install the Kiro Agent GitHub app on the repo
2. Go to `app.kiro.dev/agent`, select the gangstarr repo
3. Create a task:
   > "Run `gangstarr check python/` and analyze the findings in `.gangstarr/gangstarr.db`. For each confirmed issue (correlated static + runtime), create a fix. Open a PR with the changes."

The autonomous agent will:
- Clone the repo into a sandbox
- Read `AGENTS.md` and `.kiro/steering/` for context
- Execute the analysis
- Open a PR with fixes

### CI Integration (GitHub Actions)

```yaml
# .github/workflows/steeze.yml
name: Gangstarr Steeze
on:
  schedule:
    - cron: '0 6 * * 1'  # Weekly Monday 6am
  workflow_dispatch:

jobs:
  steeze:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - name: Run gangstarr check
        run: |
          pip install gangstarr
          gangstarr check src/
      - name: Trigger Kiro autonomous agent
        # Use Kiro's GitHub integration — the agent picks up
        # issues labeled 'kiro' or can be triggered via the web UI.
        # Alternatively, commit the .gangstarr/ DB and let the
        # autonomous agent analyze it on the next task.
        run: echo "Findings stored in .gangstarr/gangstarr.db"
```

---

## What Makes This Work: Steering Files

The existing `.kiro/steering/rules.md` and `AGENTS.md` already give Kiro the project context it needs. The steeze agent adds a focused prompt on top. This is the key insight — **if the steering files are good, swapping the AI tool is just swapping the agent config format.**

### Adapting for Other AI Tools (Post-Launch)

| Tool | Config Format | Steering Equivalent | Notes |
|------|--------------|---------------------|-------|
| Kiro CLI | `.kiro/agents/steeze.json` | `.kiro/steering/*.md` | Primary. Ships in repo. |
| Kiro Autonomous | Web UI task + `AGENTS.md` | `AGENTS.md` + steering | Async, opens PRs |
| GitHub Copilot | `.github/copilot-instructions.md` | Same content, different file | Reuse steeze-prompt.md |
| Cursor | `.cursor/rules/*.md` | Same content, different dir | Reuse steeze-prompt.md |
| Aider | `.aider.conf.yml` + conventions | Inline or file-based | Reuse prompt content |

The prompt content (`steeze-prompt.md`) is tool-agnostic. Only the config wrapper changes.

---

## Implementation Checklist

- [ ] Create `.kiro/agents/steeze.json`
- [ ] Create `.kiro/steering/steeze-prompt.md`
- [ ] Test interactively: `kiro-cli --agent steeze`
- [ ] Verify the agent can query `.gangstarr/gangstarr.db` via `sqlite3`
- [ ] Verify the agent reads source files referenced in findings
- [ ] Add `steeze` subcommand to `src/cli.rs` (thin kiro-cli wrapper)
- [ ] Document in README under a "AI-Assisted Analysis" section
- [ ] Test with Kiro autonomous agent via GitHub
- [ ] Add CI workflow for scheduled steeze runs

---

## Open Questions

1. **Kiro CLI non-interactive mode** — Does `kiro-cli` support piping a prompt via stdin or a `--prompt` flag for fully headless execution? If not, the `gangstarr steeze` wrapper would launch an interactive session. Worth checking `kiro-cli --help` or requesting the feature.

2. **Token limits** — Large codebases with many findings could exceed context windows. The steeze prompt should instruct the agent to batch findings and work in chunks.

3. **Branch mode safety** — When the agent creates a branch and commits, it should always use a `steeze/` prefix (e.g., `steeze/fix-n-plus-1-books`) and never push to `main`.

4. **API key distribution** — Kiro CLI uses your Kiro account (no separate API key for the CLI itself). For CI, the autonomous agent uses the GitHub app integration. No keys to manage in the repo.
