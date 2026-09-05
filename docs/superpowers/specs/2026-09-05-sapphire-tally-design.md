# sapphire-tally MVP 設計書

- 日付: 2026-09-05
- 状態: 承認済み（実装開始可）
- 関係プロジェクト: grain-id（ID方式）、sapphire-journal / sapphire-ledger（思想・前例）

## 概要

「達成スタンプ」を記録し、GitHubのコントリビューショングラフ（草）風のヒートマップで
可視化するツール。目標管理やリマインドは持たず、**カウントと可視化に特化**する。
ポジティブな習慣（練習・筋トレ）にもネガティブな監視（摂取量の削減）にも等しく使える。

- データはむき出しのテキストファイル（TOML）のみ。DBは持たない（journal/ledgerと同じ思想）
- MVPはMCPサーバーのみ。remote workspace（git同期）対応は後続
- キャッシュ（redb等）は後回し。まずディレクトリスキャンで動作させる

## ゴール / 非ゴール

ゴール:

- 種目（activity）とスタンプ（stamp）の記録・一覧・集計をMCPツールとして提供する
- 1日複数回のスタンプを許容し、1日の回数で濃度が変るヒートマップ用データを提供する
- 複数端末からの並列追加に対応できるデータ形式（1スタンプ1ファイル）

非ゴール（MVPでは作らない）:

- 目標（goal）設定・リマインド・習慣の継続日数計算
- remote workspace（sapphire-framework remote-server / registry）対応
- キャッシュ層、CLI、GUIクライアント

## データモデル

データディレクトリはサーバー起動時に `--data-dir`（または環境変数）で指定する。

### 配置

- `<data-dir>/stamps/`     … スタンプ。平坦配置
- `<data-dir>/activities/` … 種目。平坦配置
- ファイル名はいずれも **grain-id のみ**（日付階層・種目別ディレクトリは使わない）
- **ファイル内にも `id` フィールドとしてgrain-idを書く**（ファイル名と重複してよい）。
  ファイル単体で自己完結させるため。読み込み時はファイル名由来のIDを正とし、
  ファイル内IDと食い違った場合は警告ログを出してファイル名を正とする

### 種目ファイル（`activities/<grain-id>.toml`）

```toml
id = "012atvw"                       # ファイル名と同一（自己完結のための重複）
title = "タピオカミルクティー"
unit = "week"   # 表示粒度: day(既定) / week / month。任意フィールド
```

- `id`: 必須。ファイル名と同一のgrain-id
- `title`: 必須。種目名。スタンプは種目をgrain-idでのみ参照するため、改名の影響を受けない
- `unit`: 任意。ヒートマップの表示粒度。`day` 既定

### スタンプファイル（`stamps/<grain-id>.toml`）

```toml
id = "012atvw0"                      # ファイル名と同一（自己完結のための重複）
activity = "012atvw"                 # 種目のgrain-id
timestamp = "2026-09-05T09:12:00+09:00"  # 任意。省略時はサーバー側で現在時刻
comment = "横浜で買った"              # 任意。一行程度
```

- `id`: 必須。ファイル名と同一のgrain-id
- `activity`: 必須。存在しない種目を参照する孤立スタンプはスキャン時にスキップして警告
- `timestamp`: 任意。**引用符付きRFC3339（タイムゾーン付き）文字列をフィールドとして持つ**。
  grain-id自体にもタイムスタンプは埋め込めるが、秒精度で約1090年・週単位でwrap-aroundする
  ため復元用途には使わず、必ずフィールド側を正とする
- `comment`: 任意

## クレート構成

sapphire-agentと同じ新構成（virtual workspace + ルートにバイナリ、`crates/`にライブラリ）:

```
sapphire-tally/            … virtual workspace、default-members = ["server"]
├─ server/                 … バイナリ。MCPサーバー（rmcp + streamable HTTP、axum）。デフォルトポート 3174
└─ crates/
   └─ sapphire-tally-core/ … Stamp / Activity モデル、TOML読み書き、スキャン・集計
```

- MCPは別クレートに分けず `server` に一体化する（ツール数が少ないため）
- 依存は姉妹プロジェクトと統一: `rmcp`（server, transport-streamable-http-server）、
  `axum`（http1, tokio）、`tokio`、`tokio-util`、`anyhow`、`chrono`（serde）、`serde`、
  `serde_json`、`thiserror`、`toml`、`grain-id`（serde/schemars）、`tracing(-subscriber)`
- dev-dependencies: `tempfile`、`tower`（util）、`http-body-util`、`tokio`（macros, rt-multi-thread）

## MCPツール面（たたき台）

- `activity_add(title, unit?)` — 種目を追加し、grain-idを返す
- `activity_list()` — 種目一覧
- `stamp_add(activity, timestamp?, comment?)` — スタンプ追加。timestamp省略時は現在時刻
- `stamp_list(activity?, from?, to?)` — スタンプ一覧（期間・種目フィルタ）
- `heatmap(activity?, from?, to?)` — 種目のunitに応じた日/週/月カウントの集計を返す。
  可視化の描画はクライアント側（アシスタント）の役割。サーバーは集計までを提供

## エラー処理

- 不正なTOML・孤立スタンプ（存在しないactivity参照）はスキャン時に**スキップして警告ログ**。
  一部ファイルの破損で全体が読めるのを防ぐ（破損ファイルのみスキップし、全体は読める）
- ファイル内IDとファイル名IDの不一致は警告してファイル名を正とする
- grain-id衝突（同一IDのファイルが複数）は事実上起きないが、起きたら上書きせずエラー

## テスト

- core層にはunitテストをTDDで先行させる
- tempfileによる一時ディレクトリの読み書きを統合テスト（journalと同じ流儀）

## 将来の拡張（MVP範囲外）

- キャッシュ（redb等）によるスキャン高速化
- remote workspace（git同期）対応
- CLI / GUIクライアント
- 継続日数・累計などの集計拡張
