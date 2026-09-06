//! sapphire-tally の MCP サーバーハンドラ。`Tally` を `Arc<Mutex<Tally>>` で共有する。
//! journal-mcp の SapphireJournalServer と同型。

use std::path::Path;
use std::sync::{Arc, Mutex, MutexGuard};

use anyhow::Context as _;
use rmcp::{
    ServerHandler,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::*,
    schemars, tool, tool_router,
};
use sapphire_tally_core::{Tally, Unit};
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
        Self {
            state,
            tool_router: Self::tool_router(),
        }
    }
    /// journal と同じく poison を panic にせず recover する。
    fn lock_state(&self) -> MutexGuard<'_, Tally> {
        if self.state.is_poisoned() {
            tracing::warn!("tally state mutex was poisoned by an earlier panic; recovering");
        }
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
    fn with_state<R>(&self, f: impl FnOnce(&Tally) -> anyhow::Result<R>) -> anyhow::Result<R> {
        f(&self.lock_state())
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
/// 繧ｳ繝ｼ繝舌・縺吶ｋ蜈ｱ騾壹ヵ繧｣繝ｫ繧ｿ繧｡繧､繝ｫ繧堤ｺｸ蜃ｺ縺ｮ空繧ｹ繧ｿ繧ｿ繧｢繧ｿ。
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct EmptyParams {}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct QueryParams {
    #[serde(default)]
    pub activity: Option<String>,
    /// 期間開始（YYYY-MM-DD、両端含む）
    #[serde(default)]
    pub from: Option<String>,
    /// 期間終了（YYYY-MM-DD、両端含む）
    #[serde(default)]
    pub to: Option<String>,
}

#[tool_router]
impl TallyServer {
    #[tool(
        description = "Create a new activity (tracker). `unit` is an optional display granularity: day/week/month (default day). Returns the created activity JSON including its id."
    )]
    fn activity_add(&self, Parameters(p): Parameters<ActivityAddParams>) -> Result<String, String> {
        (|| -> anyhow::Result<String> {
            let unit = match p.unit.as_deref() {
                Some(s) => Some(Unit::parse_unit(s).map_err(anyhow::Error::msg)?),
                None => None,
            };
            let a = self.with_state(|t| t.create_activity(&p.title, unit).map_err(Into::into))?;
            Ok(serde_json::to_string_pretty(&a)?)
        })()
        .map_err(|e: anyhow::Error| e.to_string())
    }

    #[tool(description = "List all activities as a JSON array, each with id, title, and unit.")]
    fn activity_list(&self, _: Parameters<EmptyParams>) -> Result<String, String> {
        (|| -> anyhow::Result<String> {
            let list = self.with_state(|t| t.list_activities().map_err(Into::into))?;
            Ok(serde_json::to_string_pretty(&list)?)
        })()
        .map_err(|e: anyhow::Error| e.to_string())
    }

    #[tool(
        description = "Add a stamp. `activity` is the activity's grain-id. `timestamp` is optional ISO 8601 with timezone (server time is used when omitted). `comment` is an optional one-line note."
    )]
    fn stamp_add(&self, Parameters(p): Parameters<StampAddParams>) -> Result<String, String> {
        (|| -> anyhow::Result<String> {
            let activity = p
                .activity
                .parse()
                .map_err(|_| anyhow::anyhow!("invalid grain-id `{}`", p.activity))?;
            let ts = match p.timestamp.as_deref() {
                Some(s) => Some(
                    chrono::DateTime::parse_from_rfc3339(s)
                        .map_err(|e| anyhow::anyhow!("invalid timestamp: {e}"))?,
                ),
                None => None,
            };
            let s = self.with_state(|t| {
                t.add_stamp(activity, ts, p.comment.clone())
                    .map_err(Into::into)
            })?;
            Ok(serde_json::to_string_pretty(&s)?)
        })()
        .map_err(|e: anyhow::Error| e.to_string())
    }

    #[tool(
        description = "List stamps as a JSON array, sorted by timestamp ascending. Optional filters: activity (grain-id), from/to (YYYY-MM-DD, inclusive)."
    )]
    fn stamp_list(&self, Parameters(p): Parameters<QueryParams>) -> Result<String, String> {
        (|| -> anyhow::Result<String> {
            let (activity, from, to) = p.to_filters()?;
            let list =
                self.with_state(|t| t.list_stamps(activity, from, to).map_err(Into::into))?;
            Ok(serde_json::to_string_pretty(&list)?)
        })()
        .map_err(|e: anyhow::Error| e.to_string())
    }

    #[tool(
        description = "Heatmap buckets as a JSON array. Buckets use the activity's unit (day/week/month); period format is 2026-09-05 / 2026-W36 / 2026-09. Same filters as stamp_list. Zero-count periods are omitted."
    )]
    fn heatmap(&self, Parameters(p): Parameters<QueryParams>) -> Result<String, String> {
        (|| -> anyhow::Result<String> {
            let (activity, from, to) = p.to_filters()?;
            let buckets = self.with_state(|t| t.heatmap(activity, from, to).map_err(Into::into))?;
            Ok(serde_json::to_string_pretty(&buckets)?)
        })()
        .map_err(|e: anyhow::Error| e.to_string())
    }
}

impl QueryParams {
    /// 共通フィルタ引数をパースする。不正値は tool error 文字列へ。
    fn to_filters(
        &self,
    ) -> anyhow::Result<(
        Option<grain_id::GrainId>,
        Option<chrono::NaiveDate>,
        Option<chrono::NaiveDate>,
    )> {
        let activity = self
            .activity
            .as_deref()
            .map(|s| {
                s.parse::<grain_id::GrainId>()
                    .map_err(|_| anyhow::anyhow!("invalid grain-id `{s}`"))
            })
            .transpose()?;
        let from = parse_date(self.from.as_deref(), "from")?;
        let to = parse_date(self.to.as_deref(), "to")?;
        Ok((activity, from, to))
    }
}

fn parse_date(s: Option<&str>, field: &str) -> anyhow::Result<Option<chrono::NaiveDate>> {
    s.map(|s| {
        chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d")
            .map_err(|e| anyhow::anyhow!("invalid {field} date: {e}"))
    })
    .transpose()
}

#[rmcp::tool_handler(router = self.tool_router)]
impl ServerHandler for TallyServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build()).with_instructions(
            "Sapphire Tally records stamps (activity + timestamp + optional one-line comment) into plain-text TOML files and aggregates them into GitHub-style heatmap buckets.",
        )
    }
}

impl TallyServer {
    /// テスト・サーバー起動共通のヘルパ: データディレクトリを開いてサーバーを組む。
    pub fn open(data_dir: &Path) -> anyhow::Result<Self> {
        let tally = Tally::open(data_dir.to_path_buf()).context("failed to open tally data dir")?;
        Ok(Self::new(tally))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
            .activity_add(Parameters(ActivityAddParams {
                title: "タピオカミルクティー".into(),
                unit: Some("week".into()),
            }))
            .unwrap();
        let created: serde_json::Value = serde_json::from_str(&created).unwrap();
        let id = created["id"].as_str().unwrap().to_string();
        assert_eq!(created["unit"], "week");

        server
            .stamp_add(Parameters(StampAddParams {
                activity: id.clone(),
                timestamp: Some("2026-09-05T09:12:00+09:00".into()),
                comment: Some("おいしい".into()),
            }))
            .unwrap();

        let list = server
            .stamp_list(Parameters(QueryParams {
                activity: Some(id.clone()),
                from: None,
                to: None,
            }))
            .unwrap();
        let list: serde_json::Value = serde_json::from_str(&list).unwrap();
        assert_eq!(list.as_array().unwrap().len(), 1);
        assert_eq!(list[0]["comment"], "おいしい");

        let heat = server
            .heatmap(Parameters(QueryParams {
                activity: Some(id),
                from: None,
                to: None,
            }))
            .unwrap();
        let heat: serde_json::Value = serde_json::from_str(&heat).unwrap();
        assert_eq!(heat.as_array().unwrap()[0]["count"], 1);
        assert_eq!(heat.as_array().unwrap()[0]["period"], "2026-W36");
    }

    #[test]
    fn stamp_on_unknown_activity_returns_tool_error() {
        let (_dir, server) = server();
        let err = server
            .stamp_add(Parameters(StampAddParams {
                activity: "012atvw".into(),
                timestamp: Some("2026-09-05T09:12:00+09:00".into()),
                comment: None,
            }))
            .unwrap_err();
        assert!(err.contains("not found"), "{err}");
    }

    #[test]
    fn invalid_unit_returns_tool_error() {
        let (_dir, server) = server();
        let err = server
            .activity_add(Parameters(ActivityAddParams {
                title: "x".into(),
                unit: Some("fortnight".into()),
            }))
            .unwrap_err();
        assert!(err.contains("fortnight"), "{err}");
    }
}
