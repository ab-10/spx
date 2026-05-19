# spx

`spx` scaffolds Python (FastAPI) projects and deploys them to the SPX production runtime.

## Prerequisites

- [uv](https://docs.astral.sh/uv/) — Python package manager
- [rclone](https://rclone.org/) — file sync (`brew install rclone`)
- Python 3.12+

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

### Global flags

| Flag | What it does |
|---|---|
| `--json` | Output as JSON for scripting and editor integrations |
| `-v, --verbose` | Print verbose debug output |

## Testing

```bash
cargo test
```

Integration tests run the real `spx` binary against temp directories. No network or cloud access required — tests exercise user resolution, state persistence, and the rclone availability probe.
