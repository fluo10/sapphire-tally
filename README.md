# sapphire-tally

> Language: **English** | [日本語](README.ja.md)

A file-based (TOML, one file per stamp) tracking tool that records stamps (an activity + timestamp + optional one-line comment) and visualizes frequency with day/week/month GitHub-style heatmaps.

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
