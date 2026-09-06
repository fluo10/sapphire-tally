//! データディレクトリをまたいだ操作（open/create/list/add）を提供する facade。

use std::collections::BTreeMap;
use std::path::PathBuf;

use chrono::{DateTime, Datelike, FixedOffset, Local, NaiveDate};
use grain_id::GrainId;

use crate::error::{Error, Result};
use crate::model::{Activity, HeatmapBucket, Stamp, Unit};
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
        let activity = Activity {
            id: GrainId::random(),
            title: title.to_string(),
            unit,
        };
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
                Err(e) => {
                    tracing::warn!(path = %path.display(), "skipping unreadable activity: {e}")
                }
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
        if !self.exists_activity(activity) {
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
            if activity.is_some_and(|a| stamp.activity != a) {
                continue;
            }
            let day = stamp.timestamp.date_naive();
            if from.is_some_and(|f| day < f) || to.is_some_and(|t| day > t) {
                continue;
            }
            out.push(stamp);
        }
        out.sort_by_key(|a| a.timestamp);
        Ok(out)
    }

    /// 種目の unit に応じた粒度で、期間内のスタンプ数をバケット化する。
    /// 空バケットは作らない（カウント0の日は含めない）。
    pub fn heatmap(
        &self,
        activity: Option<GrainId>,
        from: Option<NaiveDate>,
        to: Option<NaiveDate>,
    ) -> Result<Vec<HeatmapBucket>> {
        let units: BTreeMap<GrainId, Unit> = self
            .list_activities()?
            .into_iter()
            .filter(|a| activity.is_none_or(|id| a.id == id))
            .map(|a| (a.id, a.unit.unwrap_or_default()))
            .collect();

        let mut buckets: BTreeMap<(String, String), u64> = BTreeMap::new();
        for s in self.list_stamps(activity, from, to)? {
            let day = s.timestamp.date_naive();
            let unit = units.get(&s.activity).copied().unwrap_or_default();
            let period = match unit {
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
        out.sort_by(|a, b| {
            a.activity
                .to_string()
                .cmp(&b.activity.to_string())
                .then(a.period.cmp(&b.period))
        });
        Ok(out)
    }

    fn exists_activity(&self, id: GrainId) -> bool {
        store::activity_path(&self.root, id).exists()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tally() -> (tempfile::TempDir, Tally) {
        let dir = tempfile::tempdir().unwrap();
        let t = Tally::open(dir.path().to_path_buf()).unwrap();
        (dir, t)
    }

    fn ts(s: &str) -> DateTime<FixedOffset> {
        DateTime::parse_from_rfc3339(s).unwrap()
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
        let a = t
            .create_activity("タピオカミルクティー", Some(Unit::Week))
            .unwrap();
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
        let id: GrainId = "012atvw".parse().unwrap();
        let e = t
            .add_stamp(id, Some(ts("2026-09-05T09:12:00+09:00")), None)
            .unwrap_err();
        assert!(matches!(e, Error::ActivityNotFound(_)));
    }

    #[test]
    fn orphan_and_broken_stamps_are_skipped_with_warn() {
        let (dir, t) = tally();
        // 存在しない activity を参照する孤立スタンプ
        std::fs::write(
            dir.path().join("stamps/0000001.toml"),
            "id = \"0000001\"\nactivity = \"zzzzzzz\"\ntimestamp = \"2026-09-05T09:12:00+09:00\"\n",
        )
        .unwrap();
        // 壊れた TOML
        std::fs::write(dir.path().join("stamps/0000002.toml"), "not toml [[[").unwrap();
        assert!(t.list_stamps(None, None, None).unwrap().is_empty());
    }

    #[test]
    fn stamps_filter_by_activity_and_date_range_and_sort_by_timestamp() {
        let (_dir, t) = tally();
        let a = t.create_activity("タピオカ", None).unwrap();
        let b = t.create_activity("筋トレ", None).unwrap();
        t.add_stamp(a.id, Some(ts("2026-09-02T10:00:00+09:00")), None)
            .unwrap();
        t.add_stamp(a.id, Some(ts("2026-09-04T10:00:00+09:00")), None)
            .unwrap();
        t.add_stamp(b.id, Some(ts("2026-09-03T10:00:00+09:00")), None)
            .unwrap();

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

    #[test]
    fn heatmap_counts_per_day_by_default() {
        let (_dir, t) = tally();
        let a = t.create_activity("タピオカ", None).unwrap();
        t.add_stamp(a.id, Some(ts("2026-09-02T09:00:00+09:00")), None)
            .unwrap();
        t.add_stamp(a.id, Some(ts("2026-09-02T21:00:00+09:00")), None)
            .unwrap();
        t.add_stamp(a.id, Some(ts("2026-09-03T09:00:00+09:00")), None)
            .unwrap();
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
        t.add_stamp(a.id, Some(ts("2026-09-01T09:00:00+09:00")), None)
            .unwrap();
        t.add_stamp(a.id, Some(ts("2026-09-05T09:00:00+09:00")), None)
            .unwrap();
        let m = t.create_activity("月一習慣", Some(Unit::Month)).unwrap();
        t.add_stamp(m.id, Some(ts("2026-09-15T09:00:00+09:00")), None)
            .unwrap();

        let buckets = t.heatmap(None, None, None).unwrap();
        let week = buckets.iter().find(|b| b.activity == a.id).unwrap();
        assert_eq!(week.period, "2026-W36");
        assert_eq!(week.count, 2);
        let month = buckets.iter().find(|b| b.activity == m.id).unwrap();
        assert_eq!(month.period, "2026-09");
        assert_eq!(month.count, 1);
    }
}
