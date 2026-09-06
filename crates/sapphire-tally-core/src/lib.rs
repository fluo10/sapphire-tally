//! sapphire-tally-core: sapphire-tally のデータモデルと TOML ストア。
//!
//! データはむき出しの TOML ファイルとして `<data-dir>/activities/` と
//! `<data-dir>/stamps/` に平坦配置される（1ファイル＝1レコード、ファイル名は
//! grain-id のみ）。ファイル本文にも id フィールドを持つが、ファイル名が正で
//! 不一致は警告する。キャッシュ・git 同期は MVP 範囲外。

pub mod error;
pub mod model;
pub mod store;
pub mod tally;

pub use error::{Error, Result};
pub use model::{Activity, HeatmapBucket, Stamp, Unit};
pub use tally::Tally;
