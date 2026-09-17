# sapphire-tally

> Language: **English** | [日本語](README.ja.md)

A stamp tracker with GitHub-style heatmaps built on [sapphire-framework](https://github.com/fluo10/sapphire-framework) — file-based, local-first, made for human-agent collaboration. Stamps are an activity + timestamp + optional one-line comment, each stored as one plain TOML file.

**Under development.** The MVP ships the MCP server only.

## Usage (planned)

```powershell
sapphire-tally-server --data-dir C:\path\to\tally-data
# -> http://127.0.0.1:3174/mcp
```

## Data format

- `<data-dir>/activities/<grain-id>.toml` — an activity (`title` required, `unit` optional: day/week/month)
- `<data-dir>/stamps/<grain-id>.toml` — a stamp (`activity`/`timestamp` required, `comment` optional)
- Filenames are grain-ids (7-char BASE32). Each file also carries an `id` field, but the filename is authoritative.

## MCP tools

`activity_add` / `activity_list` / `stamp_add` / `stamp_list` / `heatmap`
