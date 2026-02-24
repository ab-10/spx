# spawn

`spawn` is a CLI command that fully sets up a project for agentic development.
This includes:
1. Initializing local sandboxed dev environment with strong agent feedback loops.
2. Deployment environment that's consistent with the development environment.

## Goals

1. Have a realistic preview env for the agent to use to debug.
2. 1 line setup of an agent-ready coding environment for a production NextJS agentic app.
3. 0 dev time spent on tasks that don't directly contribute to product development (dev env setup, prod deployment, observability, analytics setup).

## Stack

| Layer | Tool |
|---|---|
| Framework | Next.js 14 (App Router, TypeScript, Tailwind) |
| Hosting | Vercel |
| Database | Vercel Postgres (Neon) |
| Auth | Stack Auth |
| Local runtime | Docker |
| Agent | Claude Code (dangerous mode) |
| Browser / Testing | Playwright (headless Chromium) |

## Usage

### `spawn init [project-name]`

```
spawn init [project-name | default to dir name]           # full setup, cloud-connected (default)
spawn init [project-name | defaults to dir name] --local   # local only, no cloud wiring
```

**Default — cloud-connected:**

1. Pulls spawn base Docker image (Next.js 14, Node 20, Playwright pre-installed).
2. Scaffolds Next.js app with TypeScript, Tailwind, App Router — no prompts.
3. Provisions Vercel Postgres and injects connection strings into `.env.local`.
4. Creates Stack Auth project via API and runs installer in no-browser mode:
   ```
   npx @stackframe/init-stack --no-browser
   ```
   Wires `StackProvider` into `layout.tsx`, creates `stack/server.ts`, registers `/handler/signup`, `/handler/signin`, `/handler/account-settings`.
5. Syncs all env vars to Vercel (preview + prod).
6. Creates GitHub repo, pushes initial commit, links to Vercel for auto-deploys on `main`.
7. Drops you into the running container.

**`--local` — scaffold only (mirrors `create-next-app`):**

1. Pulls spawn base Docker image.
2. Scaffolds Next.js app.
3. Installs Stack Auth in no-browser mode — auth pages work locally, env vars are placeholders.
4. Drops you into the running container.

No database, no Vercel project, no GitHub repo — yet.
Cloud wiring runs automatically on the first `spawn deploy` or `spawn preview` call.

Port discovery:
1. Defaults to `3000`
2. Keep incrementing to find the right port (up to `40000`)
3. Display appropriate connection URL for port found

### `spawn run claude`

Launches an interactive Claude Code session inside the container in dangerous/auto-approve mode.

The agent has access to:
- `localhost:3000` — Next.js dev server with hot reload.
- `npm test` — pre-configured Playwright suite; tests live in `/tests`.
- Full filesystem, git, and Vercel CLI access.
- Stack Auth already wired — `stackServerApp.getUser()` works immediately.

When the session ends, spawn does not auto-commit.
You handle git.

### `spawn preview`

Deploys a shareable URL for the current state of your code — not prod, but real Vercel infrastructure.

```
spawn preview
spawn preview --close
```

1. Pushes current state to a `preview/*` branch.
2. Triggers Vercel preview deployment against an isolated preview database.
3. Returns a URL like `myapp-git-preview-xyz.vercel.app` and copies it to clipboard.

If initialized with `--local`: detects missing cloud wiring, prompts once, connects, then proceeds.

### `spawn deploy`

Promotes `main` to production.

```
spawn deploy
spawn deploy --force   # skip test gate
```

1. Runs `npm test` — blocks deploy if Playwright suite fails.
2. Pushes to `main`.
3. Vercel auto-deploys via GitHub integration.
4. Prints the production URL.

If initialized with `--local`: detects missing cloud wiring via `spawn.config.json`, prompts once ("This project isn't connected to the cloud yet. Connect now? [Y/n]"), provisions Vercel Postgres + Stack Auth + GitHub + Vercel, then proceeds with deploy.
This is the only time spawn asks a question after init.

## Command Reference

| Command | What it does |
|---|---|
| `spawn init [name]` | Full setup — container, DB, auth, GitHub, Vercel |
| `spawn init [name] --local` | Local scaffold only — cloud wiring deferred to first deploy |
| `spawn run claude` | Interactive Claude Code session inside the container |
| `spawn preview` | Shareable Vercel preview URL from current working state |
| `spawn preview --close` | Tears down the preview deployment |
| `spawn deploy` | Test-gated push to main → production |
| `spawn deploy --force` | Deploy, skipping the test gate |

## Design Principles

1. Always show a "next step" after each command — reduces cognitive load.
2. Fail loudly and specifically — if the container crashes, surface the exact log line, not a generic error.
3. Every URL should be clickable in modern terminals (OSC 8 hyperlinks).
4. The agent's actions should stream in real time — hiding them in a spinner creates anxiety; showing them builds trust.
5. `--json` flag on everything for scripting and editor integrations.

## Testing

Tests should give the same confidence as running the real command manually. That means:

- Integration tests run the actual `spawn` binary against real Docker — no mocks, no duplicate Dockerfiles.
- Docker must be running. Tests fail fast with a clear message if it isn't.
- We verify side effects of the real flow (config files on disk, container state, bind mounts, user permissions) rather than unit-testing internal functions against synthetic fixtures.
- Network dependencies (npm registry, etc.) are accepted. A flaky network is a real failure mode worth knowing about.
- Tests are slow (~1-2 min for npm scaffolding) and that's fine — they reproduce the actual user experience.

```bash
cargo test --test init_local    # requires Docker
```

## Reflect on

- [ ] What are the limitations of using Vercel for deployment?
    At which stage do I need a direct interface with GCP?
- [ ] `spawn init` vs `spawn init --local` flags.
    What's the cost of connecting deployment at `init`?
    Implement both ways and A/B test.
- [ ] What's a good mechanism for implementing agent tools inside of the spawn environment?
