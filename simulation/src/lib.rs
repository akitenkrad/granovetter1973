//! Granovetter (1973)「The Strength of Weak Ties」の再現実装ライブラリ．
//!
//! socsim フレームワーク上に構築した弱紐帯ブリッジ網生成 + 情報拡散モデルの
//! 公開 API を提供する．設定構造体 (`config`)・世界状態 (`world`)・網生成器
//! (`network`)・実行ドライバ (`simulation`)・集計メトリクス (`metrics`) を
//! モジュールとして公開し，バイナリ (`granovetter`) と統合テストの双方から利用する．
//!
//! 拡散の状態更新は汎用 social-dynamics クレートの `SiContagionMechanism` /
//! `ThresholdContagionMechanism` に委譲する (`simulation::run_diffusion` で配線)．
//! 世界状態は `socsim_core::{BinaryState, Neighbors}` を実装し，これらのメカニズムが
//! 操作できるようにする (`world` モジュール)．
//!
//! 実行結果の置き場と同一性は runvault が持つ (`record` モジュールが記録の作法を
//! まとめる)．タイムスタンプ付きディレクトリも `latest` シンボリックリンクも
//! こちらでは作らない．
//!
//! 紐帯強度は socsim-net の **重み付き無向ネットワーク** (`WeightedNetwork<TieStrength>`)
//! の辺重みとして保持する (設計書 §4.3 の WorldState 側 side-table 案を socsim issue
//! #18 の weighted-edge API で置き換えた)．局所ブリッジ判定・平均経路長・連結成分は
//! socsim-net の解析ヘルパ (issue #20) を直接利用する．

pub mod config;
pub mod metrics;
pub mod network;
pub mod record;
pub mod reproduce;
pub mod simulation;
pub mod world;
