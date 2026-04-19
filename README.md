# spx

`spx` scaffolds Python (FastAPI) projects and deploys them to a cloud preview environment on GCP Cloud Run.

## Prerequisites

- [uv](https://docs.astral.sh/uv/) — Python package manager
- [rclone](https://rclone.org/) — file sync (`brew install rclone`)
- Python 3.12+

## Quickstart

```bash
spx new my-app --user alice
cd my-app
```

This scaffolds a FastAPI project, installs dependencies with `uv sync`, syncs to GCS, and provisions a Cloud Run preview environment. You get a live URL at the end.

To re-deploy after making changes:

```bash
spx run
```

## Commands

### `spx new <name> --user <user>`

1. Scaffolds a FastAPI project (`pyproject.toml`, `main.py`, `.gitignore`).
2. Initializes a git repo and installs dependencies (`uv sync`).
3. Syncs the project to `gs://spx-<user>/app/` via rclone.
4. Requests a run on the preview environment (provisions on first use).

### `spx run`

1. Syncs the current directory to GCS via rclone.
2. Requests a run on the preview environment.

Use `--user <name>` on first run (or to change user). The identity is persisted to `.spx/state.json`.

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
