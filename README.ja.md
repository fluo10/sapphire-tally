# sapphire-tally（日本語）

> 言語: [English](README.md) | **日本語**

スタンプ（活動＋日時＋任意の一行コメント）を記録し、日/週/月単位のGitHub草風ヒートマップで頻度を可視化する、ファイルベース（TOML・1スタンプ1ファイル）の記録ツール。

**開発中です。** MVPはMCPサーバーのみを提供します。

## 使い方（予定）

```powershell
sapphire-tally-server --data-dir C:\path\to\tally-data
# -> http://127.0.0.1:3174/mcp
```

## データ形式

- `<data-dir>/activities/<grain-id>.toml` — 種目（title必須、unit任意: day/week/month）
- `<data-dir>/stamps/<grain-id>.toml` — スタンプ（activity/timestamp必須、comment任意）
- ファイル名はgrain-id（7文字BASE32）。ファイル内にもidフィールドを持つが、ファイル名が正

## MCPツール

`activity_add` / `activity_list` / `stamp_add` / `stamp_list` / `heatmap`
