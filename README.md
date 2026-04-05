# spx

`spx` sets up Docker container with an opinionated NextJS webapp setup.
This allows you to build with Claude Code (w/ `--dangerously-skip-permissions`) out of the box, with good feedback loops.


## Quickstart


**Installation:**
1. Get the latest pre-built binary [here](https://github.com/ab-10/spx/releases)
2. Put it on your path (e.g. `~/.local/bin`)

**Creating a project:**

```bash
spx new <project-name>
cd <project-name>
spx claude
```

This just did the following:
1. Created a docker sandbox for your project
2. Initialized a NextJS project
3. Ran Claude Code (in the sandbox) w/ `--dangerously-skip-permissions`


Now you can give Claude Code detailed instructions for executing

**Previewing your project:**

Ask Claude Code to run a development server, it should tell you the right port.

If that fails:
1. Message me, so I can fix it.
2. Run `spx shell`.
3. Inside of the shell run `npm run dev`
4. Find the (host) port in `spx.config.json` and open `localhost:{port}` in the browser.
    (The docker container maps port 3000 to 3000 or next available port on your machine).


## Stack

Agent: Claude Code
(broader agent support WIP, PRs welcome!)

We're opinionated about the decisions that don't affect your user experience.

| Layer | Tool |
|---|---|
| Framework | Next.js 14 (App Router, TypeScript, Tailwind) |
| Hosting | Vercel |
| Database | Vercel Postgres (Neon) |
| Auth | (WIP) Stack Auth |
| Local runtime | Docker |
| Browser / Testing | Playwright (headless Chromium) |

## Usage

### `spx new [project-name]`

```
spx new [project-name | defaults to dir name]
```

1. Pulls spx base Docker image (Next.js 14, Node 20, Playwright pre-installed).
2. Scaffolds Next.js app with TypeScript, Tailwind, App Router — no prompts.
3. Drops you into the running container.
4. Pulls spx base Docker image.
5. Scaffolds Next.js app.
6. Creates a local git repo
7. Creates an initial commit
8. Starts the dev server and exits.

**Port discovery:**
1. Defaults to `3000`
2. Keep incrementing to find the right port (up to `40000`)
3. Display appropriate connection URL for port found


### `spx link`

1. Provisions Vercel Postgres and injects connection strings into `.env.local`.
2. Syncs all env vars to Vercel (preview + prod).
3. Creates GitHub repo, pushes initial commit, links to Vercel for auto-deploys on `main`.
4. Sets up CD

> **WIP:** System-level authentication config shared across projects.
> For now, `spx link` authenticates per-project.


### `spx claude`

Launches an interactive Claude Code session inside the container in dangerous/auto-approve mode.

The agent has access to:
- `localhost:3000` — Next.js dev server with hot reload.
- `npm test` — pre-configured Playwright suite; tests live in `/tests`.
- Project's filesystem, git, and Vercel CLI access.

## Command Reference

| Command | What it does |
|---|---|
| `spx new [name]` | Local scaffold only — cloud wiring deferred to first deploy |
| `spx link` | Wire project to GitHub + Vercel for continuous deployment |
| `spx claude` | Interactive Claude Code session inside the container |
| `spx shell` | Interactive shell inside the container |

### Global flags

| Flag | What it does |
|---|---|
| `--json` | Output as JSON for scripting and editor integrations |
| `-v, --verbose` | Print verbose debug output (useful when a command hangs) |

## Design Principles

1. Always show a "next step" after each command — reduces cognitive load.
2. Fail loudly and specifically — if the container crashes, surface the exact log line, not a generic error.
3. Every URL should be clickable in modern terminals (OSC 8 hyperlinks).
4. The agent's actions should stream in real time — hiding them in a spinner creates anxiety; showing them builds trust.
5. `--json` flag on everything for scripting and editor integrations.

## Testing

Tests should give the same confidence as running the real command manually. That means:

- Integration tests run the actual `spx` binary against real Docker — no mocks, no duplicate Dockerfiles.
- Docker must be running. Tests fail fast with a clear message if it isn't.
- We verify side effects of the real flow (config files on disk, container state, bind mounts, user permissions) rather than unit-testing internal functions against synthetic fixtures.
- Network dependencies (npm registry, etc.) are accepted. A flaky network is a real failure mode worth knowing about.
- Tests are slow (~1-2 min for npm scaffolding) and that's fine — they reproduce the actual user experience.

```bash
cargo test --test new_local    # requires Docker
```

## Reflect on

- [ ] What are the limitations of using Vercel for deployment?
    At which stage do I need a direct interface with GCP?
- [ ] `spx new` vs `spx new --local` flags.
    What's the cost of connecting deployment at `init`?
    Implement both ways and A/B test.
- [ ] What's a good mechanism for implementing agent tools inside of the spx environment?
