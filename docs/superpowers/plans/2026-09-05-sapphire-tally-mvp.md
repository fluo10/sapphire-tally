# sapphire-tally MVP Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** MCPサーバー経由でスタンプ（活動＋日時＋任意の一行コメント）を記録し、日/週/月単位のヒートマップ集計を返す、ファイルベース（TOML・1スタンプ1ファイル）の記録ツールを作る。

**Architecture:** 2クレート構成。`crates/sapphire-tally-core` がデータモデル（Activity/Stamp）とTOMLファイルの読み書き・集計を持つライブラリ。ルート直下の `server` が rmcp の streamable HTTP（axum）でMCPツール5本を提供するバイナリ。キャッシュ・git同期・CLI・GUIはMVP範囲外（spec参照）。

**Tech Stack:** Rust edition 2024（resolver 3）、rmcp 1.5（server, transport-streamable-http-server）、axum 0.8、tokio、clap 4、chrono（serde feature、DateTime<FixedOffset>）、serde、toml 1.1、grain-id 0.16（serde feature）、thiserror 2、tracing(-subscriber)、dev-deps: tempfile、tower、http-body-util。

**Spec:** `docs/superpowers/specs/2026-09-05-sapphire-tally-design.md`

## Global Constraints

- edition 2024、workspace resolver = "3"、依存のバージョンは姉妹プロジェクト（sapphire-ledger）と同一系列で統一
- MCPツールの戻り値はすべて JSON 文字列（非asyncの `fn` で `Result<String, String>`、journal-mcpと同じ流儀。エラーは `e.to_string()`）。ツールメソッド名はツール名そのもの（`activity_add` など、journalの `entry_list` と同じ流儀）
- ツール引数の `Parameters<T>` 入力スキーマは `type = "object"` を宣言すること（journalで過去バグになった点。テストで担保する）
- データディレクトリ: `<data-dir>/stamps/` と `<data-dir>/activities/`、どちらも平坦配置・TOML・ファイル名は grain-id（7文字BASE32）のみ + `.toml`
- タイムスタンプはファイル内フィールド（引用符付きRFC3339文字列、例 `timestamp = "2026-09-05T09:12:00+09:00"`）。ファイル名から時刻は復元しない
- 不正TOML・孤立スタンプ（存在しないactivity参照）はスキャン時にスキップして `tracing::warn!`。全体エラーにしない
- grain-id衝突時は上書きせずエラー
- デフォルトポート 3174、bind デフォルト `127.0.0.1`、Host allowlist はループバック常に許可＋`--allowed-host` 追加（journalのhttp.rsと同じ方針）
- ドキュメントコメント・コミットメッセージは日本語可（姉妹プロジェクトと同じ风格。コミットは conventional commits）

---

### Task 1: ワークスペーススキャフォールドとcoreのモデル・読み書き

**Files:**
- Create: `Cargo.toml`（workspaceルート）
- Create: `.gitignore`（sapphire-ledgerと同一内容で可: target, debug, *.rs.bk, .DS_Store, .superpowers/ 等）
- Create: `LICENSE-APACHE`, `LICENSE-MIT`（姉妹プロジェクトと同じMIT/Apache-2.0デュアルライセンス。テキストは crates.io-standard のもの）
- Create: `README.md`（プロジェクト概要と「開発中」の一文だけでよい）
- Create: `crates/sapphire-tally-core/Cargo.toml`
- Create: `crates/sapphire-tally-core/src/lib.rs`
- Create: `crates/sapphire-tally-core/src/error.rs`
- Create: `crates/sapphire-tally-core/src/model.rs`
- Test: `crates/sapphire-tally-core/src/model.rs` 内 `#[cfg(test)] mod tests`

**Interfaces:**
- Consumes: なし（最初のタスク）
- Produces:
  - `model::{Activity, Stamp, Unit, HeatmapBucket}` — Task 2以降が使うモデル
  - `store::{read_activity, read_stamp, write_activity, write_stamp}` — Task 2が使うファイルI/O
  - `error::{Error, Result}` — Task 2以降が使うエラー型

- [ ] **Step 1: workspaceルートのCargo.tomlを作る**

```toml
[workspace]
members = ["server", "crates/sapphire-tally-core"]
default-members = ["server"]
resolver = "3"

[workspace.package]
description = "Stamp counter with GitHub-style heatmaps - keeps your data alive as plain text"
license = "MIT OR Apache-2.0"
repository = "https://github.com/fluo10/sapphire-tally"
edition = "2024"

[workspace.dependencies]
anyhow = "1"
axum = { version = "0.8", default-features = false, features = ["http1", "tokio"] }
chrono = { version = "0.4", features = ["serde"] }
clap = { version = "4", features = ["derive", "env"] }
grain-id = { version = "0.16", features = ["serde"] }
rmcp = { version = "1.5", features = ["server", "transport-streamable-http-server"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
tempfile = "3"
thiserror = "2"
tokio = { version = "1" }
tokio-util = "0.7"
toml = "1.1"
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }

[profile.dev.package."*"]
opt-level = 1
```

- [ ] **Step 2: coreクレートのCargo.tomlを作る**

```toml
[package]
name = "sapphire-tally-core"
edition.workspace = true
version = "0.1.0"
description.workspace = true
license.workspace = true
repository.workspace = true

[dependencies]
chrono.workspace = true
grain-id.workspace = true
serde.workspace = true
thiserror.workspace = true
toml.workspace = true
tracing.workspace = true

[dev-dependencies]
tempfile.workspace = true
```

- [ ] **Step 3: 失敗するテストを書く（モデルの読み書きラウンドトリップ）**

`crates/sapphire-tally-core/src/model.rs` のテストモジュール:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn activity_roundtrips_through_toml() {
        let toml = "title = \"タピオカミルクティー\"\nunit = \"week\"\n";
        let a: Activity = toml::from_str(toml).unwrap();
        assert_eq!(a.title, "タピオカミルクティー");
        assert_eq!(a.unit, Some(Unit::Week));
    }

    #[test]
    fn activity_without_unit_deserializes_to_none() {
        let a: Activity = toml::from_str("title = \"筋トレ\"\n").unwrap();
        assert_eq!(a.unit, None);
    }

    #[test]
    fn stamp_roundtrips_through_toml() {
        let toml = "activity = \"012atvw\"\ntimestamp = \"2026-09-05T09:12:00+09:00\"\ncomment = \"横浜で買った\"\n";
        let s: Stamp = toml::from_str(toml).unwrap();
        assert_eq!(s.timestamp.to_rfc3339(), "2026-09-05T09:12:00+09:00");
        assert_eq!(s.comment.as_deref(), Some("横浜で買った"));
    }

    #[test]
    fn stamp_without_comment_deserializes_and_serializes_compactly() {
        let s: Stamp = toml::from_str(
            "activity = \"012atvw\"\ntimestamp = \"2026-09-05T09:12:00+09:00\"\n",
        )
        .unwrap();
        assert!(s.comment.is_none());
        // comment: None は出力しない（skip_serializing_if）
        let out = toml::to_string(&s).unwrap();
        assert!(!out.contains("comment"), "{out}");
    }

    #[test]
    fn invalid_unit_is_rejected_by_from_str() {
        let e = "fortnight".parse::<Unit>().unwrap_err();
        assert!(e.contains("fortnight"), "{e}");
        assert_eq!("week".parse::<Unit>().unwrap(), Unit::Week);
    }
}
```

- [ ] **Step 4: テストが失敗することを確認**

Run: `cargo test -p sapphire-tally-core`（初回は型が存在せずコンパイル失敗＝失敗扱いでOK）

- [ ] **Step 5: モデルとエラーを実装する**

`src/error.rs`:

```rust
use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("failed to parse {path}: {message}")]
    Parse { path: String, message: String },

    #[error("invalid grain-id `{0}`")]
    InvalidId(String),

    #[error("activity `{0}` not found")]
    ActivityNotFound(String),

    #[error("`{0}` already exists")]
    AlreadyExists(String),
}

pub type Result<T> = std::result::Result<T, Error>;
```

`src/model.rs`（テストの上側）:

```rust
use chrono::{DateTime, FixedOffset};
use grain_id::GrainId;
use serde::{Deserialize, Serialize};

/// 表示粒度。活動ごとの表示設定。既定は Day。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Unit {
    Day,
    Week,
    Month,
}

impl Unit {
    /// "day"/"week"/"month"（大小文字・前後空白不問）からパースする。
    /// 不正値は `Err(String)`（メッセージに不正値を含む。サーバー側のtool errorにそのまま流す）。
    pub fn parse_unit(s: &str) -> Result<Self, String> {
        match s.trim().to_ascii_lowercase().as_str() {
            "day" => Ok(Unit::Day),
            "week" => Ok(Unit::Week),
            "month" => Ok(Unit::Month),
            other => Err(format!("unit must be one of day/week/month, got `{other}`")),
        }
    }
}

impl Default for Unit {
    fn default() -> Self {
        Unit::Day
    }
}

/// 活動（種目）。ファイル名が id（`<id>.toml`）なので id フィールドは TOML には書かない。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Activity {
    #[serde(skip)]
    pub id: GrainId,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unit: Option<Unit>,
}

/// 1スタンプ = 1ファイル。ファイル名は `<activity の id とは別の grain-id>.toml`。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Stamp {
    #[serde(skip)]
    pub id: GrainId,
    pub activity: GrainId,
    pub timestamp: DateTime<FixedOffset>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,
}

/// ヒートマップの1マス分の集計結果。period は "2026-09-05" / "2026-W36" / "2026-09" 形式。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HeatmapBucket {
    pub activity: GrainId,
    pub period: String,
    pub count: u64,
}
```

- [ ] **Step 6: ストア（読み書き）を実装する**

`src/store.rs`:

```rust
use std::path::Path;

use grain_id::GrainId;

use crate::error::{Error, Result};
use crate::model::{Activity, Stamp};

pub fn activity_path(root: &Path, id: GrainId) -> std::path::PathBuf {
    root.join("activities").join(format!("{id}.toml"))
}

pub fn stamp_path(root: &Path, id: GrainId) -> std::path::PathBuf {
    root.join("stamps").join(format!("{id}.toml"))
}

pub fn read_activity(root: &Path, id: GrainId) -> Result<Activity> {
    let path = activity_path(root, id);
    let text = std::fs::read_to_string(&path)?;
    let mut a: Activity = toml::from_str(&text).map_err(|e| Error::Parse {
        path: path.display().to_string(),
        message: e.to_string(),
    })?;
    a.id = id;
    Ok(a)
}

pub fn read_stamp(root: &Path, id: GrainId) -> Result<Stamp> {
    let path = stamp_path(root, id);
    let text = std::fs::read_to_string(&path)?;
    let mut s: Stamp = toml::from_str(&text).map_err(|e| Error::Parse {
        path: path.display().to_string(),
        message: e.to_string(),
    })?;
    s.id = id;
    Ok(s)
}

/// 新規ファイルのみ書く。既存があれば AlreadyExists。
pub fn write_activity(root: &Path, a: &Activity) -> Result<()> {
    let path = activity_path(root, a.id);
    if path.exists() {
        return Err(Error::AlreadyExists(path.display().to_string()));
    }
    std::fs::write(&path, toml::to_string_pretty(a).unwrap() + "\n")?;
    Ok(())
}

pub fn write_stamp(root: &Path, s: &Stamp) -> Result<()> {
    let path = stamp_path(root, s.id);
    if path.exists() {
        return Err(Error::AlreadyExists(path.display().to_string()));
    }
    std::fs::write(&path, toml::to_string_pretty(s).unwrap() + "\n")?;
    Ok(())
}
```

`src/lib.rs`:

```rust
pub mod error;
pub mod model;
pub mod store;

pub use error::{Error, Result};
pub use model::{Activity, HeatmapBucket, Stamp, Unit};
```

- [ ] **Step 7: テストを通す**

Run: `cargo test -p sapphire-tally-core` → Expected: PASS（5 tests）

- [ ] **Step 8: Commit**

```bash
git add Cargo.toml .gitignore LICENSE-APACHE LICENSE-MIT README.md crates/
git commit -m "feat: workspace scaffold with core models and TOML store"
```

---

### Task 2: Tally（データディレクトリ open/create/list）

**Files:**
- Create: `crates/sapphire-tally-core/src/tally.rs`
- Modify: `crates/sapphire-tally-core/src/lib.rs`（`pub mod tally; pub use tally::Tally;` を追加）
- Test: `tally.rs` 内 `#[cfg(test)] mod tests`（tempfile使用）

**Interfaces:**
- Consumes: Task 1 の `store::*`, `model::*`, `error::*`
- Produces: `tally::Tally` — Task 4 のサーバーが使う:
  - `Tally::open(root: PathBuf) -> Result<Tally>`（`stamps/` `activities/` が無ければ作成）
  - `create_activity(&self, title: &str, unit: Option<Unit>) -> Result<Activity>`
  - `list_activities(&self) -> Result<Vec<Activity>>`（title の Unicode コードポイント順ソート）
  - `add_stamp(&self, activity: GrainId, timestamp: Option<DateTime<FixedOffset>>, comment: Option<String>) -> Result<Stamp>`（timestamp省略時 `chrono::Local::now().fixed_offset()`）
  - `list_stamps(&self, activity: Option<GrainId>, from: Option<NaiveDate>, to: Option<NaiveDate>) -> Result<Vec<Stamp>>`（timestamp昇順）

- [ ] **Step 1: 失敗するテストを書く**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn tally() -> (tempfile::TempDir, Tally) {
        let dir = tempfile::tempdir().unwrap();
        let t = Tally::open(dir.path().to_path_buf()).unwrap();
        (dir, t)
    }

    fn ts(s: &str) -> chrono::DateTime<chrono::FixedOffset> {
        chrono::DateTime::parse_from_rfc3339(s).unwrap()
    }

    #[test]
    fn open_creates_both_directories() {
        let (dir, _) = tally();
        assert!(dir.path().join("stamps").is_dir());
        assert!(dir.path().join("activities").is_dir());
    }

    #[test]
    fn create_then_list_activities_sorted_by_title() {
        let (_dir, t) = tally();
        let b = t.create_activity("筋トレ", None).unwrap();
        let a = t.create_activity("タピオカミルクティー", Some(Unit::Week)).unwrap();
        let list = t.list_activities().unwrap();
        // "タピオカ..."(U+30BF…) < "筋トレ"(U+7B4B…) なので codepoint 順でタピオカが先
        assert_eq!(list[0].title, "タピオカミルクティー");
        assert_eq!(list[0].id, a.id);
        assert_eq!(list[0].unit, Some(Unit::Week));
        assert_eq!(list[1].title, "筋トレ");
        assert_eq!(list[1].id, b.id);
    }

    #[test]
    fn add_stamp_without_timestamp_uses_now_and_roundtrips() {
        let (_dir, t) = tally();
        let a = t.create_activity("筋トレ", None).unwrap();
        let s = t.add_stamp(a.id, None, Some("30回".into())).unwrap();
        assert_eq!(s.activity, a.id);
        assert!(s.timestamp.timestamp() > 0);
        let listed = t.list_stamps(None, None, None).unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].id, s.id);
        assert_eq!(listed[0].comment.as_deref(), Some("30回"));
    }

    #[test]
    fn stamp_on_unknown_activity_fails() {
        let (_dir, t) = tally();
        let id: grain_id::GrainId = "012atvw".parse().unwrap();
        let e = t.add_stamp(id, Some(ts("2026-09-05T09:12:00+09:00")), None).unwrap_err();
        assert!(matches!(e, Error::ActivityNotFound(_)));
    }

    #[test]
    fn orphan_and_broken_stamps_are_skipped_with_warn() {
        let (dir, t) = tally();
        // 存在しない activity を参照する孤立スタンプ
        std::fs::write(
            dir.path().join("stamps/0000001.toml"),
            "activity = \"zzzzzzz\"\ntimestamp = \"2026-09-05T09:12:00+09:00\"\n",
        )
        .unwrap();
        // 壊れた TOML
        std::fs::write(dir.path().join("stamps/0000002.toml"), "not toml [[[")
            .unwrap();
        assert!(t.list_stamps(None, None, None).unwrap().is_empty());
    }

    #[test]
    fn stamps_filter_by_activity_and_date_range_and_sort_by_timestamp() {
        let (_dir, t) = tally();
        let a = t.create_activity("タピオカ", None).unwrap();
        let b = t.create_activity("筋トレ", None).unwrap();
        t.add_stamp(a.id, Some(ts("2026-09-02T10:00:00+09:00")), None).unwrap();
        t.add_stamp(a.id, Some(ts("2026-09-04T10:00:00+09:00")), None).unwrap();
        t.add_stamp(b.id, Some(ts("2026-09-03T10:00:00+09:00")), None).unwrap();

        let all = t.list_stamps(None, None, None).unwrap();
        assert_eq!(all.len(), 3);
        assert!(all[0].timestamp < all[1].timestamp && all[1].timestamp < all[2].timestamp);

        let only_a = t.list_stamps(Some(a.id), None, None).unwrap();
        assert_eq!(only_a.len(), 2);

        let (from, to) = (
            NaiveDate::from_ymd_opt(2026, 9, 3).unwrap(),
            NaiveDate::from_ymd_opt(2026, 9, 5).unwrap(),
        );
        let windowed = t.list_stamps(None, Some(from), Some(to)).unwrap();
        assert_eq!(windowed.len(), 2);
    }
}
```

- [ ] **Step 2: 失敗を確認** — Run: `cargo test -p sapphire-tally-core` → Expected: FAIL（Tally未定義でコンパイル失敗）

- [ ] **Step 3: 実装する**

`src/tally.rs`:

```rust
use std::path::PathBuf;

use chrono::{DateTime, FixedOffset, Local, NaiveDate};
use grain_id::GrainId;

use crate::error::{Error, Result};
use crate::model::{Activity, Stamp, Unit};
use crate::store;

/// データディレクトリのルート。`stamps/` と `activities/` の平坦配置を仮定する。
#[derive(Debug, Clone)]
pub struct Tally {
    pub root: PathBuf,
}

impl Tally {
    /// データディレクトリを開く。`stamps/` `activities/` が無ければ作成する。
    pub fn open(root: PathBuf) -> Result<Self> {
        std::fs::create_dir_all(root.join("stamps"))?;
        std::fs::create_dir_all(root.join("activities"))?;
        Ok(Self { root })
    }

    /// 新種目を作成して grain-id を割り当てる。
    pub fn create_activity(&self, title: &str, unit: Option<Unit>) -> Result<Activity> {
        let activity = Activity { id: GrainId::random(), title: title.to_string(), unit };
        store::write_activity(&self.root, &activity)?;
        Ok(activity)
    }

    /// 全種目を title 昇順（Unicodeコードポイント順）で返す。壊れたファイルは警告してスキップ。
    pub fn list_activities(&self) -> Result<Vec<Activity>> {
        let mut out = Vec::new();
        for entry in std::fs::read_dir(self.root.join("activities"))? {
            let path = entry?.path();
            if path.extension().and_then(|e| e.to_str()) != Some("toml") {
                continue;
            }
            let stem = match path.file_stem().and_then(|s| s.to_str()) {
                Some(s) => s,
                None => continue,
            };
            let id = match stem.parse::<GrainId>() {
                Ok(id) => id,
                Err(_) => {
                    tracing::warn!(path = %path.display(), "skipping file with invalid grain-id name");
                    continue;
                }
            };
            match store::read_activity(&self.root, id) {
                Ok(a) => out.push(a),
                Err(e) => tracing::warn!(path = %path.display(), "skipping unreadable activity: {e}"),
            }
        }
        out.sort_by(|a, b| a.title.cmp(&b.title));
        Ok(out)
    }

    /// スタンプ追加。timestamp 省略時はサーバーの現在時刻を使う。
    pub fn add_stamp(
        &self,
        activity: GrainId,
        timestamp: Option<DateTime<FixedOffset>>,
        comment: Option<String>,
    ) -> Result<Stamp> {
        if !store::activity_path(&self.root, activity).exists() {
            return Err(Error::ActivityNotFound(activity.to_string()));
        }
        let stamp = Stamp {
            id: GrainId::random(),
            activity,
            timestamp: timestamp.unwrap_or_else(|| Local::now().fixed_offset()),
            comment: comment.filter(|c| !c.trim().is_empty()),
        };
        store::write_stamp(&self.root, &stamp)?;
        Ok(stamp)
    }

    /// スタンプ一覧。activity・日付範囲（両端含む、日付判定はスタンプ自身のオフセット基準）
    /// で絞り込み、timestamp 昇順。
    pub fn list_stamps(
        &self,
        activity: Option<GrainId>,
        from: Option<NaiveDate>,
        to: Option<NaiveDate>,
    ) -> Result<Vec<Stamp>> {
        let mut out = Vec::new();
        for entry in std::fs::read_dir(self.root.join("stamps"))? {
            let path = entry?.path();
            if path.extension().and_then(|e| e.to_str()) != Some("toml") {
                continue;
            }
            let stem = match path.file_stem().and_then(|s| s.to_str()) {
                Some(s) => s,
                None => continue,
            };
            let id = match stem.parse::<GrainId>() {
                Ok(id) => id,
                Err(_) => {
                    tracing::warn!(path = %path.display(), "skipping file with invalid grain-id name");
                    continue;
                }
            };
            let stamp = match store::read_stamp(&self.root, id) {
                Ok(s) => s,
                Err(e) => {
                    tracing::warn!(path = %path.display(), "skipping unreadable stamp: {e}");
                    continue;
                }
            };
            if !self.exists_activity(stamp.activity) {
                tracing::warn!(stamp = %stamp.id, activity = %stamp.activity, "orphan stamp skipped");
                continue;
            }
            if let Some(a) = activity {
                if stamp.activity != a {
                    continue;
                }
            }
            let day = stamp.timestamp.date_naive();
            if from.is_some_and(|f| day < f) || to.is_some_and(|t| day > t) {
                continue;
            }
            out.push(stamp);
        }
        out.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));
        Ok(out)
    }

    fn exists_activity(&self, id: GrainId) -> bool {
        store::activity_path(&self.root, id).exists()
    }
}
```

- [ ] **Step 4: テストを通す** — Run: `cargo test -p sapphire-tally-core` → Expected: PASS（Task 1–3 全テスト）

- [ ] **Step 5: Commit**

```bash
git add crates/ && git commit -m "feat: Tally store — open/create/list/add with orphan-skip semantics"
```

---

### Task 3: ヒートマップ集計

**Files:**
- Modify: `crates/sapphire-tally-core/src/tally.rs`
- Test: 同 `#[cfg(test)] mod tests` に追加

**Interfaces:**
- Consumes: Task 2 の `Tally::list_stamps`, `list_activities`, `model::HeatmapBucket`
- Produces: `Tally::heatmap(&self, activity: Option<GrainId>, from: Option<NaiveDate>, to: Option<NaiveDate>) -> Result<Vec<HeatmapBucket>>` — Task 4 の heatmap ツールが使用。period 形式は day=`2026-09-05`、week=ISO週 `2026-W36`、month=`2026-09`

- [ ] **Step 1: 失敗するテストを書く**

```rust
#[test]
fn heatmap_counts_per_day_by_default() {
    let (_dir, t) = tally();
    let a = t.create_activity("タピオカ", None).unwrap();
    t.add_stamp(a.id, Some(ts("2026-09-02T09:00:00+09:00")), None).unwrap();
    t.add_stamp(a.id, Some(ts("2026-09-02T21:00:00+09:00")), None).unwrap();
    t.add_stamp(a.id, Some(ts("2026-09-03T09:00:00+09:00")), None).unwrap();
    let buckets = t.heatmap(None, None, None).unwrap();
    assert_eq!(buckets.len(), 2);
    assert_eq!(buckets[0].period, "2026-09-02");
    assert_eq!(buckets[0].count, 2);
    assert_eq!(buckets[1].period, "2026-09-03");
}

#[test]
fn heatmap_groups_by_week_and_month_per_activity_unit() {
    let (_dir, t) = tally();
    let a = t.create_activity("週一習慣", Some(Unit::Week)).unwrap();
    // 2026-W36（2026-08-31〜09-06）にまたがる2件
    t.add_stamp(a.id, Some(ts("2026-09-01T09:00:00+09:00")), None).unwrap();
    t.add_stamp(a.id, Some(ts("2026-09-05T09:00:00+09:00")), None).unwrap();
    let m = t.create_activity("月一習慣", Some(Unit::Month)).unwrap();
    t.add_stamp(m.id, Some(ts("2026-09-15T09:00:00+09:00")), None).unwrap();

    let buckets = t.heatmap(None, None, None).unwrap();
    let week = buckets.iter().find(|b| b.activity == a.id).unwrap();
    assert_eq!(week.period, "2026-W36");
    assert_eq!(week.count, 2);
    let month = buckets.iter().find(|b| b.activity == m.id).unwrap();
    assert_eq!(month.period, "2026-09");
    assert_eq!(month.count, 1);
}
```

- [ ] **Step 2: 失敗を確認** — `cargo test -p sapphire-tally-core`

- [ ] **Step 3: 実装する**（`tally.rs` の `impl Tally` にメソッド追加。`use crate::model::HeatmapBucket` を import）

```rust
/// 種目の unit に応じた粒度で、期間内のスタンプ数をバケット化する。
/// 空バケットは作らない（カウント0の日は含めない）。
pub fn heatmap(
    &self,
    activity: Option<GrainId>,
    from: Option<NaiveDate>,
    to: Option<NaiveDate>,
) -> Result<Vec<HeatmapBucket>> {
    use chrono::Datelike as _;
    use chrono::iso_week::ISOWeek as _;
    use std::collections::BTreeMap;

    let units: Vec<(GrainId, Unit)> = self
        .list_activities()?
        .into_iter()
        .filter(|a| activity.is_none_or(|id| a.id == id))
        .map(|a| (a.id, a.unit.unwrap_or_default()))
        .collect();

    let mut buckets: BTreeMap<(String, String), u64> = BTreeMap::new();
    for s in self.list_stamps(activity, from, to)? {
        let day = s.timestamp.date_naive();
        let unit = units.iter().find(|(id, _)| *id == s.activity).map(|(_, u)| *u);
        let period = match unit.unwrap_or_default() {
            Unit::Day => day.format("%Y-%m-%d").to_string(),
            Unit::Week => format!("{}-W{:02}", day.iso_week().year(), day.iso_week().week()),
            Unit::Month => day.format("%Y-%m").to_string(),
        };
        *buckets.entry((s.activity.to_string(), period)).or_insert(0) += 1;
    }

    let mut out: Vec<HeatmapBucket> = buckets
        .into_iter()
        .map(|((activity, period), count)| HeatmapBucket {
            activity: activity.parse().expect("bucket key round-trips"),
            period,
            count,
        })
        .collect();
    out.sort_by(|a, b| a.activity.cmp(&b.activity).then(a.period.cmp(&b.period)));
    Ok(out)
}
```

- [ ] **Step 4: テストを通す** — `cargo test -p sapphire-tally-core` → Expected: PASS（Task 1–3 全テスト）

- [ ] **Step 5: Commit**

```bash
git add crates/ && git commit -m "feat: heatmap aggregation with per-activity day/week/month granularity"
```

---

### Task 4: serverクレート（MCPツール5本＋HTTPサーブ）

**Files:**
- Create: `server/Cargo.toml`
- Create: `server/src/lib.rs`
- Create: `server/src/server.rs`
- Create: `server/src/http.rs`
- Create: `server/src/main.rs`
- Test: `server/src/server.rs` 内テスト

**Interfaces:**
- Consumes: Task 1–3 の `sapphire_tally_core::{Tally, Activity, Stamp, HeatmapBucket, Unit}`
- Produces:
  - `server::TallyServer`（`Tally` を `Arc<Mutex<Tally>>` で共有する rmcp サーバー。journal-mcpの `SapphireJournalServer` と同型）
  - MCPツール5本（メソッド名＝ツール名）: `activity_add(title, unit?)`, `activity_list()`, `stamp_add(activity, timestamp?, comment?)`, `stamp_list(activity?, from?, to?)`, `heatmap(activity?, from?, to?)` — 全て非async `fn` で `Result<String, String>`（JSON文字列）
  - `http::{mcp_router, serve_http}`、`main.rs` は clap CLI（`--data-dir`/`SAPPHIRE_TALLY_DIR` 必須、`--addr` 既定 127.0.0.1、`--port` 既定 3174、`--allowed-host` 複数可）

- [ ] **Step 1: server/Cargo.toml**

```toml
[package]
name = "sapphire-tally-server"
edition.workspace = true
version = "0.1.0"
description.workspace = true
license.workspace = true
repository.workspace = true
categories = ["command-line-utilities", "web-programming::http-server"]
keywords = ["mcp", "tracker", "heatmap", "server", "self-hosted"]
publish = false

[[bin]]
name = "sapphire-tally-server"
path = "src/main.rs"

[dependencies]
sapphire-tally-core = { path = "../crates/sapphire-tally-core", version = "0.1.0" }
anyhow.workspace = true
axum.workspace = true
chrono.workspace = true
clap.workspace = true
grain-id.workspace = true
rmcp.workspace = true
serde.workspace = true
serde_json.workspace = true
tokio = { workspace = true, features = ["rt-multi-thread", "macros", "signal"] }
tokio-util.workspace = true
tracing.workspace = true
tracing-subscriber = { workspace = true }

[dev-dependencies]
http-body-util = "0.1"
tempfile.workspace = true
tokio = { workspace = true, features = ["macros", "rt-multi-thread"] }
tower = { version = "0.5", features = ["util"] }
```

- [ ] **Step 2: 失敗するテストを書く**（`server/src/server.rs` 末尾。ツールメソッドは非asyncの `fn` で定義され、戻り値は `Result<String, String>`。ゼロ引数ツール `activity_list` は journal と同じく `Parameters<EmptyObject>` を受け取る）

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use sapphire_tally_core::Unit;

    fn server() -> (tempfile::TempDir, TallyServer) {
        let dir = tempfile::tempdir().unwrap();
        let tally = Tally::open(dir.path().to_path_buf()).unwrap();
        (dir, TallyServer::new(tally))
    }

    #[test]
    fn all_tool_input_schemas_declare_object_type() {
        for tool in TallyServer::tool_router().list_all() {
            let schema = &tool.input_schema;
            assert_eq!(
                schema.get("type").and_then(|t| t.as_str()),
                Some("object"),
                "tool `{}` input_schema must declare type=object, got: {schema:?}",
                tool.name
            );
        }
    }

    #[test]
    fn add_activity_then_stamp_then_heatmap_roundtrip() {
        let (_dir, server) = server();
        let created = server
            .activity_add(ActivityAddParams {
                title: "タピオカミルクティー".into(),
                unit: Some("week".into()),
            })
            .unwrap();
        let created: serde_json::Value = serde_json::from_str(&created).unwrap();
        let id = created["id"].as_str().unwrap().to_string();
        assert_eq!(created["unit"], "week");

        server
            .stamp_add(StampAddParams {
                activity: id.clone(),
                timestamp: Some("2026-09-05T09:12:00+09:00".into()),
                comment: Some("おいしい".into()),
            })
            .unwrap();

        let list = server
            .stamp_list(QueryParams { activity: Some(id.clone()), from: None, to: None })
            .unwrap();
        let list: serde_json::Value = serde_json::from_str(&list).unwrap();
        assert_eq!(list.as_array().unwrap().len(), 1);
        assert_eq!(list[0]["comment"], "おいしい");

        let heat = server
            .heatmap(QueryParams { activity: Some(id.clone()), from: None, to: None })
            .unwrap();
        let heat: serde_json::Value = serde_json::from_str(&heat).unwrap();
        assert_eq!(heat.as_array().unwrap()[0]["count"], 1);
        assert_eq!(heat.as_array().unwrap()[0]["period"], "2026-W36");
    }

    #[test]
    fn stamp_on_unknown_activity_returns_tool_error() {
        let (_dir, server) = server();
        let err = server
            .stamp_add(StampAddParams {
                activity: "012atvw".into(),
                timestamp: Some("2026-09-05T09:12:00+09:00".into()),
                comment: None,
            })
            .unwrap_err();
        assert!(err.contains("not found"), "{err}");
    }

    #[test]
    fn invalid_unit_returns_tool_error() {
        let (_dir, server) = server();
        let err = server
            .activity_add(ActivityAddParams { title: "x".into(), unit: Some("fortnight".into()) })
            .unwrap_err();
        assert!(err.contains("fortnight"), "{err}");
    }
}
```

- [ ] **Step 3: 失敗を確認**（`cargo test -p sapphire-tally-server`、コンパイル失敗でFAIL）

- [ ] **Step 4: server.rs を実装する**

骨格（journal-mcpの `server.rs` と同型。stateは `Arc<Mutex<Tally>>`、poisoned mutexはjournalと同様に recover して `tracing::warn!`）:

```rust
use std::path::Path;
use std::sync::{Arc, Mutex};

use anyhow::Context as _;
use rmcp::{
    ServerHandler, ServiceExt,
    handler::server::{router::ToolRouter, wrapper::Parameters},
    model::*,
    schemars,
    tool, tool_router,
};
use sapphire_tally_core::{Activity, Stamp, Tally, Unit};
use serde::Deserialize;

#[derive(Clone)]
pub struct TallyServer {
    state: Arc<Mutex<Tally>>,
    tool_router: ToolRouter<Self>,
}

impl TallyServer {
    pub fn new(tally: Tally) -> Self {
        Self::from_shared(Arc::new(Mutex::new(tally)))
    }
    pub fn from_shared(state: Arc<Mutex<Tally>>) -> Self {
        Self { state, tool_router: Self::tool_router() }
    }
    /// journal と同じく poison を panic にせず recover する。
    fn with_state<R>(&self, f: impl FnOnce(&Tally) -> anyhow::Result<R>) -> anyhow::Result<R> {
        let state = if self.state.is_poisoned() {
            tracing::warn!("tally state mutex was poisoned by an earlier panic; recovering");
        }
        self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        f(&state)
    }
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ActivityAddParams {
    pub title: String,
    /// 表示粒度: day/week/month（省略時 day）
    #[serde(default)]
    pub unit: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct StampAddParams {
    pub activity: String,
    /// ISO 8601 時刻（タイムゾーン付き）。省略時はサーバーの現在時刻。
    #[serde(default)]
    pub timestamp: Option<String>,
    #[serde(default)]
    pub comment: Option<String>,
}

/// stamp_list と heatmap で共有するパラメータ。
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct QueryParams {
    #[serde(default)]
    pub activity: Option<String>,
    /// 期間開始（YYYY-MM-DD、両端含む）
    #[serde(default)]
    pub from: Option<String>,
    #[serde(default)]
    pub to: Option<String>,
}

#[tool_router]
impl TallyServer {
    #[tool(description = "Create a new activity (tracker). `unit` is an optional display granularity: day/week/month (default day). Returns the created activity JSON including its id.")]
    fn activity_add(&self, Parameters(p): Parameters<ActivityAddParams>) -> Result<String, String> {
        (|| -> anyhow::Result<String> {
            let unit = match p.unit.as_deref() {
                Some(s) => Some(Unit::parse_unit(s).map_err(anyhow::Error::msg)?),
                None => None,
            };
            let a = self.with_state(|t| t.create_activity(&p.title, unit).map_err(Into::into))?;
            Ok(serde_json::to_string_pretty(&a)?)
        })().map_err(|e: anyhow::Error| e.to_string())
    }

    #[tool(description = "List all activities as a JSON array, each with id, title, and unit.")]
    fn activity_list(&self, _: Parameters<serde_json::Value>) -> Result<String, String> {
        (|| -> anyhow::Result<String> {
            let list = self.with_state(|t| t.list_activities().map_err(Into::into))?;
            Ok(serde_json::to_string_pretty(&list)?)
        })().map_err(|e: anyhow::Error| e.to_string())
    }

    #[tool(description = "Add a stamp. `activity` is the activity's grain-id. `timestamp` is optional ISO 8601 with timezone (server time is used when omitted). `comment` is an optional one-line note.")]
    fn stamp_add(&self, Parameters(p): Parameters<StampAddParams>) -> Result<String, String> {
        (|| -> anyhow::Result<String> {
            let activity = p.activity.parse()
                .map_err(|_| anyhow::anyhow!("invalid grain-id `{}`", p.activity))?;
            let ts = match p.timestamp.as_deref() {
                Some(s) => Some(chrono::DateTime::parse_from_rfc3339(s)
                    .map_err(|e| anyhow::anyhow!("invalid timestamp: {e}"))?),
                None => None,
            };
            let s = self.with_state(|t| {
                t.add_stamp(activity, ts, p.comment.clone()).map_err(Into::into)
            })?;
            Ok(serde_json::to_string_pretty(&s)?)
        })().map_err(|e: anyhow::Error| e.to_string())
    }

    #[tool(description = "List stamps as a JSON array, sorted by timestamp ascending. Optional filters: activity (grain-id), from/to (YYYY-MM-DD, inclusive).")]
    fn stamp_list(&self, Parameters(p): Parameters<QueryParams>) -> Result<String, String> {
        (|| -> anyhow::Result<String> {
            let (activity, from, to) = p.into_filters()?;
            let list = self.with_state(|t| t.list_stamps(activity, from, to).map_err(Into::into))?;
            Ok(serde_json::to_string_pretty(&list)?)
        })().map_err(|e: anyhow::Error| e.to_string())
    }

    #[tool(description = "Heatmap buckets as a JSON array. Buckets use the activity's unit (day/week/month); period format is 2026-09-05 / 2026-W36 / 2026-09. Same filters as stamp_list. Zero-count periods are omitted.")]
    fn heatmap(&self, Parameters(p): Parameters<QueryParams>) -> Result<String, String> {
        (|| -> anyhow::Result<String> {
            let (activity, from, to) = p.into_filters()?;
            let buckets = self.with_state(|t| t.heatmap(activity, from, to).map_err(Into::into))?;
            Ok(serde_json::to_string_pretty(&buckets)?)
        })().map_err(|e: anyhow::Error| e.to_string())
    }
}

impl QueryParams {
    /// 共通フィルタ引数をパースする。不正値は tool error 文字列へ。
    fn into_filters(&self) -> anyhow::Result<(
        Option<grain_id::GrainId>, Option<chrono::NaiveDate>, Option<chrono::NaiveDate>)> {
        let activity = self.activity.as_deref().map(|s| s.parse::<grain_id::GrainId>()
            .map_err(|_| anyhow::anyhow!("invalid grain-id `{s}`"))).transpose()?;
        let from = parse_date(self.from.as_deref(), "from")?;
        let to = parse_date(self.to.as_deref(), "to")?;
        Ok((activity, from, to))
    }
}

fn parse_date(s: Option<&str>, field: &str) -> anyhow::Result<Option<chrono::NaiveDate>> {
    s.map(|s| chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d")
        .map_err(|e| anyhow::anyhow!("invalid {field} date: {e}")))
        .transpose()
}

#[rmcp::tool_handler(router = self.tool_router)]
impl ServerHandler for TallyServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_instructions("Sapphire Tally records stamps (activity + timestamp + optional one-line comment) into plain-text TOML files and aggregates them into GitHub-style heatmap buckets.")
    }
}
```

（注記: `Stamp`/`Activity`/`HeatmapBucket` のJSON出力は serde のまま。`GrainId` は serde feature により文字列としてシリアライズされる。journal-mcpと同じ流儀として `EmptyObject` 系ヘルパを使う場合は journal に倣うこと——`Parameters<serde_json::Value>` でも可）

- [ ] **Step 5: lib.rs / http.rs / main.rs を実装する**

`server/src/lib.rs`:

```rust
pub mod http;
pub mod server;

pub use http::serve_http;
pub use server::TallyServer;
```

`server/src/http.rs`: journalの `sapphire-journal-mcp/src/http.rs` と同型。要点:
- `LOOPBACK_HOSTS = ["localhost", "127.0.0.1", "::1"]` を allowlist に常に含める。`--allowed-host` 追加は journal の http.rs のコメントにある教訓（bind を広げると allowlist も広げる必要がある／空リスト＝全許可にならない）を踏襲
- `StreamableHttpServerConfig::default().with_cancellation_token(cancel).with_allowed_hosts(hosts)`
- `StreamableHttpService::new(factory, Arc::new(LocalSessionManager::default()), config)` を `axum::Router::new().route_service("/mcp", http_service)` で提供する
- stateは `Tally::open(data_dir)?` を `Arc<Mutex<Tally>>` に包み `TallyServer::from_shared` するだけ

`server/src/main.rs`:

```rust
use clap::Parser as _;

#[derive(clap::Parser)]
#[command(name = "sapphire-tally-server", about = "MCP server for sapphire-tally")]
struct Cli {
    /// データディレクトリ（stamps/ と activities/ を含む）。環境変数 SAPPHIRE_TALLY_DIR でも指定可。
    #[arg(long, env = "SAPPHIRE_TALLY_DIR")]
    data_dir: Option<String>,
    #[arg(long, default_value = "127.0.0.1")]
    addr: String,
    #[arg(long, default_value_t = 3174)]
    port: u16,
    /// MCP Host allowlist に追加するホスト名（繰り返し指定可）
    #[arg(long)]
    allowed_host: Vec<String>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| "info".into()))
        .with_writer(std::io::stderr)
        .init();
    let cli = Cli::parse();
    let data_dir = cli.data_dir.ok_or_else(|| {
        anyhow::anyhow!("--data-dir is required (or set SAPPHIRE_TALLY_DIR)")
    })?;
    sapphire_tally_server::serve_http(
        std::path::Path::new(&data_dir), &cli.addr, cli.port, &cli.allowed_host,
    ).await
}
```

- [ ] **Step 6: テストを通す** — Run: `cargo test --workspace` → Expected: PASS（core + server全テスト）。`cargo clippy --workspace -- -D warnings` もクリーン

- [ ] **Step 7: スモークテスト（HTTP起動確認）**

```powershell
# データディレクトリを作った状態でサーバーを起動
cargo run -p sapphire-tally-server -- --data-dir $env:TEMP/tally-data
# 別ターミナルで initialize を投げる:
curl.exe -s -X POST http://127.0.0.1:3174/mcp -H "Content-Type: application/json" -H "Accept: application/json, text/event-stream" -d '{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\",\"params\":{\"protocolVersion\":\"2025-03-26\",\"capabilities\":{},\"clientInfo\":{\"name\":\"smoke\",\"version\":\"0\"}}}'
```

Expected: serverInfo と tools に `activity_add, activity_list, stamp_add, stamp_list, heatmap` の5本が並ぶ JSON-RPC 応答。

- [ ] **Step 8: Commit**

```bash
git add server/ Cargo.toml crates/ && git commit -m "feat: MCP server with 5 tools over streamable HTTP (default port 3174)"
```

---

### Task 5: READMEと最終検証

**Files:**
- Modify: `README.md`
- Test: なし（検証タスク）

**Interfaces:**
- Consumes: Task 1–4の全成果物
- Produces: なし

- [ ] **Step 1: READMEを書く**（使い方: `sapphire-tally-server --data-dir ... [--addr] [--port 3174]`、MCPエンドポイント `<addr>:<port>/mcp`、データ形式は `stamps/<grain-id>.toml` / `activities/<grain-id>.toml`、フィールドは spec の例を転載、MCPツール5本の一覧とパラメータ）

- [ ] **Step 2: 最終検証** — `cargo fmt --check`、`cargo clippy --workspace -- -D warnings`、`cargo test --workspace` すべてクリーン

- [ ] **Step 3: Commit & push**

```bash
git add README.md && git commit -m "docs: usage, data format, MCP tools"
git push -u origin master
```

---

## 未実装（スコープ外の記録）

- リモートワークスペース同期（sapphire-framework remote-server / registry）
- キャッシュ、削除ツール（stamp_remove / activity_remove）、CLI/GUIクライアント
