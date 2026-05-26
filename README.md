# spx

`spx` scaffolds Python (FastAPI) projects and deploys them to the SPX production runtime.

## Prerequisites

- [rclone](https://rclone.org/) — file sync (`brew install rclone`)

## Quickstart

```bash
spx new my-app
cd my-app
```

This scaffolds a FastAPI project, installs dependencies with `uv sync`, deploys it to SPX, and prints a live project URL.

To re-deploy after making changes:

```bash
spx run main.py
```

## Deployment Lifetime

An SPX deployment is a remote service, not a process tied to your local terminal. Closing the terminal, ending the local CLI process, or putting your laptop to sleep does not intentionally stop the remote service.

Each project has a stable `.runspx.com` URL. Running `spx run <file>` again for the same project replaces the running service behind that project URL.

Use `spx kill <deployment-slug>` to stop the running remote service and remove its active routing while stopped. `spx kill` does not delete your local project files or saved project identity.

## Commands

### `spx new <name>`

1. Scaffolds a FastAPI project (`pyproject.toml`, `main.py`, `.gitignore`).
2. Initializes a git repo and installs dependencies (`uv sync`).
3. Deploys the project to the SPX production runtime.
4. Prints the stable project URL and deployment slug.

### `spx run <file>`

1. Packages the current directory.
2. Deploys the selected Python entry file to the SPX production runtime.
3. Replaces the running service for the same project URL.

The project identity and deployment slug are persisted to `.spx/state.json`.

`spx run` exits after deploy so you can continue chained shell commands.

Use `spx env` to manage deploy environment values before running `spx run`.

### `spx uv`

Manage deployment dependencies:

```bash
spx uv list
spx uv add httpx
spx uv add "uvicorn[standard]>=0.34"
spx uv remove httpx
```

Dependency changes apply on the next deploy with `spx run <file>`.
You do not need local `python`, `pip`, or `uv` installed to deploy.

#### Dependency management decisions

- `pyproject.toml` and `uv.lock` are the dependency source of truth.
- `spx uv add/remove/list` is the preferred interface for agent-safe, non-interactive dependency changes.
- Dependencies are project/deployment state in SPX; users should not need a separate "server" mental model.
- Dependency edits are explicit and reviewable in git (lockfile and manifest diffs), then applied at deploy time.
- Runtime install behavior is deterministic (`uv sync --frozen`), so lockfile drift is surfaced instead of silently resolved.

### `spx env`

Manage persisted project-scoped env values:

```bash
spx env set DATABASE_URL=postgres://...
spx env set DATABASE_URL --from-env
printf %s "$DATABASE_URL" | spx env set DATABASE_URL --from-stdin
spx env load .env
spx env list
spx env unset DATABASE_URL
```

`spx env` is non-interactive by default. Bare `spx env set KEY` is invalid and fails with a clear error.

### `spx logs`

Query runtime logs for the current project:

```bash
spx logs
spx logs --limit 100
spx logs --severity error
spx logs --from 2026-05-19T10:00:00Z --to 2026-05-19T10:05:00Z
```

- Output is JSON and is designed for deterministic agent/tool parsing.
- Default query window is the last five minutes.
- `--severity` supports `info` and `error`.
- `--from` and `--to` accept ISO 8601 timestamps.
- `--limit` caps returned entries (default `500`).

#### Logging interface decisions

- Logging UX prioritizes coding-agent workflows: structured output over human-only formatting.
- Querying is window/filter based (`from`/`to`/`severity`/`limit`) instead of cursor-based in v1.
- v1 intentionally excludes `--follow`; repeated bounded queries are the expected workflow.

### `spx feedback`

Send product feedback in a single non-interactive command:

```bash
spx feedback "deploy failed with glibc error"
echo "deploy output and notes" | spx feedback -
```

- No prompts, editor, or interactive login.
- Uses existing auth token when available; otherwise submits anonymous feedback.
- Auto-attaches context: CLI version, OS/arch, and `~/.spx/last.log` when present.
- Includes an agent note to attach chat logs/transcripts in the feedback message.
- Enforces a 1MB submission limit.

### Global flags

| Flag | What it does |
|---|---|
| `-v, --verbose` | Print verbose debug output |

## Testing

```bash
cargo test
```

Integration tests run the real `spx` binary against temp directories. No network or cloud access required — tests exercise user resolution, state persistence, and the rclone availability probe.

## Maintainer release rule

When changing `spx`, bump `version` in `Cargo.toml` in the same change.
Unless explicitly specified otherwise, increment the patch version.
