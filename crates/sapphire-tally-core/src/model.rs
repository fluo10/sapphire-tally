use chrono::{DateTime, FixedOffset};
use grain_id::GrainId;
use serde::{Deserialize, Serialize};

/// 表示粒度。活動ごとの表示設定。既定は Day。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Unit {
    #[default]
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

/// 活動（種目）。id はファイル名と重複するが、ファイル単体を自己完結させるため書き込む。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Activity {
    /// id: ファイル名と重複するが自己完結のため書く（読み込み時はファイル名が正、不一致は警告）
    pub id: GrainId,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unit: Option<Unit>,
}

/// 1スタンプ = 1ファイル。id はファイル名と重複するが、ファイル単体を自己完結させるため書き込む。
/// 読み込み時はファイル名由来のIDを正とし、不一致は警告してファイル名を正とする。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Stamp {
    /// id: ファイル名と重複するが自己完結のため書く（読み込み時はファイル名が正、不一致は警告）
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn activity_roundtrips_through_toml() {
        let toml = "id = \"012atvw\"\ntitle = \"タピオカミルクティー\"\nunit = \"week\"\n";
        let a: Activity = toml::from_str(toml).unwrap();
        assert_eq!(a.title, "タピオカミルクティー");
        assert_eq!(a.unit, Some(Unit::Week));
    }

    #[test]
    fn activity_without_unit_deserializes_to_none() {
        let a: Activity = toml::from_str("id = \"012atvw\"\ntitle = \"筋トレ\"\n").unwrap();
        assert_eq!(a.unit, None);
    }

    #[test]
    fn stamp_roundtrips_through_toml() {
        let toml = "id = \"012atvw\"\nactivity = \"012atvw\"\ntimestamp = \"2026-09-05T09:12:00+09:00\"\ncomment = \"横浜で買った\"\n";
        let s: Stamp = toml::from_str(toml).unwrap();
        assert_eq!(s.id.to_string(), "012atvw");
        assert_eq!(s.activity.to_string(), "012atvw");
        assert_eq!(s.timestamp.to_rfc3339(), "2026-09-05T09:12:00+09:00");
        assert_eq!(s.comment.as_deref(), Some("横浜で買った"));
    }

    #[test]
    fn stamp_without_comment_deserializes_and_serializes_compactly() {
        let s: Stamp = toml::from_str(
            "id = \"012atvw\"\nactivity = \"012atvw\"\ntimestamp = \"2026-09-05T09:12:00+09:00\"\n",
        )
        .unwrap();
        assert!(s.comment.is_none());
        // comment: None は出力しない（skip_serializing_if）
        let out = toml::to_string(&s).unwrap();
        assert!(!out.contains("comment"), "{out}");
    }

    #[test]
    fn invalid_unit_is_rejected_by_parse_unit() {
        let e = Unit::parse_unit("fortnight").unwrap_err();
        assert!(e.contains("fortnight"), "{e}");
        assert_eq!(Unit::parse_unit("WEEK").unwrap(), Unit::Week);
    }
}
