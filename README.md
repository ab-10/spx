# spx

`spx` publishes standalone HTML files at short, unlisted SPX URLs.

## Quickstart

```bash
spx login
spx report.html
```

Publishing prints a shareable URL like `https://abc123.runspx.dev/`.

## Commands

### `spx <path>`

Uploads a new standalone HTML file and prints its public-by-URL pub URL.

```bash
spx report.html
```

The file must end in `.html` or `.htm` and must be no larger than 10 MB.

### `spx create <path>`

Explicit form of `spx <path>`.

```bash
spx create report.html
```

### `spx update <slug-or-url> <path>`

Replaces an existing pub owned by the current account and keeps the same URL.

```bash
spx update abc123 report-v2.html
spx update https://abc123.runspx.dev/ report-v2.html
```

### `spx delete <slug-or-url>`

Deletes an existing pub owned by the current account.

```bash
spx delete abc123
```

### `spx list`

Lists pubs owned by the current account.

```bash
spx list
```

### `spx login [--code CODE]`

Authenticates the CLI via GitHub OAuth, or redeems a registration code.

```bash
spx login
spx login --code CODE
```

### `spx subscribe`

Starts Stripe Checkout for an SPX subscription and waits for activation.

```bash
spx subscribe
```

### `spx feedback <message|->`

Sends product feedback in a single non-interactive command.

```bash
spx feedback "publish failed for report.html"
printf '%s' "publish output and notes" | spx feedback -
```

## Global Flags

| Flag | What it does |
|---|---|
| `-v, --verbose` | Print verbose debug output |

## Testing

```bash
cargo test
```

## Maintainer Release Rule

When changing `spx`, bump `version` in `Cargo.toml` in the same change.
Unless explicitly specified otherwise, increment the patch version.
