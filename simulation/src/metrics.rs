//! 評価指標．
//!
//! 論文の構造的命題と拡散結果に対応する指標を計算する．紐帯強度は socsim-net の
//! 辺重みとして保持されているので，局所ブリッジ判定 ([`WeightedNetwork::local_bridges`])・
//! 平均経路長 ([`WeightedNetwork::average_path_length`])・連結成分
//! ([`WeightedNetwork::component_membership`] / `largest_component_size`) は
//! ライブラリヘルパを直接利用する．強度別到達 (`reach_by_strength`) のみ，辺重みで
//! 隣接をフィルタする BFS を metrics 側で実装する (ヘルパに該当機能がないため)．

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use serde::Serialize;
use socsim_core::AgentId;
use socsim_net::WeightedNetwork;

use crate::world::{TieStrength, WeakTieWorld};

/// 局所ブリッジのうち弱紐帯である割合 (論文ファクト7「すべてのブリッジは弱紐帯」)．
///
/// 局所ブリッジが 1 本も無い場合は `1.0` を返す (反証なし)．
pub fn frac_weak_bridges(net: &WeightedNetwork<TieStrength>) -> f64 {
    let bridges = net.local_bridges();
    if bridges.is_empty() {
        return 1.0;
    }
    let weak = bridges
        .iter()
        .filter(|&&(a, b)| net.edge_weight(a, b) == Some(&TieStrength::Weak))
        .count();
    weak as f64 / bridges.len() as f64
}

/// 局所ブリッジの本数 (`d>2`)．
pub fn n_local_bridges(net: &WeightedNetwork<TieStrength>) -> usize {
    net.local_bridges().len()
}

/// 禁じられた三者関係の生起率 (観測値 / ランダム期待値)．
///
/// 禁制三者 = 「A–B 強・A–C 強・C–B 欠如」のパス．強紐帯で接続された 2-パス
/// (A を中心とする強紐帯ペア B,C) のうち，B–C 辺が存在しないものの割合を観測値とする．
/// 戻り値は (観測される禁制三者の割合) で，値が小さいほど重なり仮説が成立する
/// (論文ファクト6: Davis)．強紐帯 2-パスが無ければ `0.0`．
pub fn forbidden_triad_rate(net: &WeightedNetwork<TieStrength>) -> f64 {
    let mut total_pairs = 0usize; // 強紐帯 2-パス (中心 A から強紐帯で B, C)．
    let mut forbidden = 0usize; // そのうち B–C 辺が欠如するもの．

    let mut ids: Vec<AgentId> = net.edges().flat_map(|(a, b)| [a, b]).collect();
    ids.sort();
    ids.dedup();

    for &a in &ids {
        // A の強紐帯隣接を集める．
        let strong_nb: Vec<AgentId> = net
            .neighbors_iter(a)
            .filter(|&x| net.edge_weight(a, x) == Some(&TieStrength::Strong))
            .collect();
        for i in 0..strong_nb.len() {
            for j in (i + 1)..strong_nb.len() {
                let b = strong_nb[i];
                let c = strong_nb[j];
                total_pairs += 1;
                // B–C 辺が (強弱問わず) 存在しなければ禁制三者．
                if net.edge_weight(b, c).is_none() {
                    forbidden += 1;
                }
            }
        }
    }
    if total_pairs == 0 {
        0.0
    } else {
        forbidden as f64 / total_pairs as f64
    }
}

/// シードから到達した網の割合 (飽和時の informed 割合)．
pub fn reach_fraction(world: &WeakTieWorld) -> f64 {
    let n = world.n();
    if n == 0 {
        return 0.0;
    }
    world.n_informed() as f64 / n as f64
}

/// 到達ノード集合 (BFS で網全体の連結到達可能集合; 拡散確率は無視した構造的到達)．
///
/// シード集合から辿れるノードを `strength` でフィルタしながら BFS する．
/// `strength=None` なら全辺，`Some(s)` なら重み `s` の辺のみを辿る．
fn structural_reach(
    net: &WeightedNetwork<TieStrength>,
    seeds: &[AgentId],
    strength: Option<TieStrength>,
) -> BTreeSet<AgentId> {
    let mut visited: BTreeSet<AgentId> = BTreeSet::new();
    let mut queue: VecDeque<AgentId> = VecDeque::new();
    for &s in seeds {
        if visited.insert(s) {
            queue.push_back(s);
        }
    }
    while let Some(cur) = queue.pop_front() {
        for nb in net.neighbors_iter(cur) {
            let ok = match strength {
                None => true,
                Some(s) => net.edge_weight(cur, nb) == Some(&s),
            };
            if ok && visited.insert(nb) {
                queue.push_back(nb);
            }
        }
    }
    visited
}

/// 強紐帯のみ / 弱紐帯のみで辿った構造的到達人数 (論文 Rapoport–Horvath)．
///
/// 戻り値は (強紐帯のみ到達, 弱紐帯のみ到達)．シード自身は除く．
pub fn reach_by_strength(net: &WeightedNetwork<TieStrength>, seeds: &[AgentId]) -> (usize, usize) {
    let seed_set: BTreeSet<AgentId> = seeds.iter().copied().collect();
    let strong = structural_reach(net, seeds, Some(TieStrength::Strong));
    let weak = structural_reach(net, seeds, Some(TieStrength::Weak));
    let n_strong = strong.difference(&seed_set).count();
    let n_weak = weak.difference(&seed_set).count();
    (n_strong, n_weak)
}

/// 到達ノードへの平均最短経路長 (= 社会的距離)．
///
/// socsim-net の [`WeightedNetwork::average_path_length`] (全到達対の BFS 平均) を
/// そのまま使う (網全体; 弱紐帯除去後の網に対して呼べば除去後の社会的距離になる)．
pub fn avg_path_length(net: &WeightedNetwork<TieStrength>) -> f64 {
    net.average_path_length().unwrap_or(0.0)
}

/// 最大連結成分のサイズ割合．
pub fn largest_component_fraction(net: &WeightedNetwork<TieStrength>) -> f64 {
    let n = net.node_count();
    if n == 0 {
        return 0.0;
    }
    net.largest_component_size() as f64 / n as f64
}

/// 単一実行の構造 + 拡散メトリクス (metrics.csv の 1 行)．
#[derive(Debug, Clone, Serialize)]
pub struct Metrics {
    /// 試行番号 (run index)．
    pub run: usize,
    /// 乱数シード．
    pub seed: u64,
    /// 総エージェント数．
    pub n_agents: usize,
    /// 辺の総数．
    pub n_edges: usize,
    /// 局所ブリッジ数．
    pub n_local_bridges: usize,
    /// 局所ブリッジのうち弱紐帯の割合．
    pub frac_weak_bridges: f64,
    /// 禁じられた三者関係の生起率．
    pub forbidden_triad_rate: f64,
    /// 到達割合 (飽和時の informed 割合)．
    pub reach_fraction: f64,
    /// 平均最短経路長．
    pub avg_path_length: f64,
    /// 最大連結成分のサイズ割合．
    pub largest_component_fraction: f64,
    /// 強紐帯のみで辿った到達人数．
    pub reach_strong_only: usize,
    /// 弱紐帯のみで辿った到達人数．
    pub reach_weak_only: usize,
    /// 飽和までのラウンド数．
    pub cascade_rounds: usize,
}

impl Metrics {
    /// 世界状態とラウンド数からメトリクスを計算する．
    pub fn compute(world: &WeakTieWorld, run: usize, seed: u64, cascade_rounds: usize) -> Self {
        let net = &world.net;
        let (reach_strong, reach_weak) = reach_by_strength(net, &world.seeds);
        Metrics {
            run,
            seed,
            n_agents: world.n(),
            n_edges: net.edge_count(),
            n_local_bridges: n_local_bridges(net),
            frac_weak_bridges: frac_weak_bridges(net),
            forbidden_triad_rate: forbidden_triad_rate(net),
            reach_fraction: reach_fraction(world),
            avg_path_length: avg_path_length(net),
            largest_component_fraction: largest_component_fraction(net),
            reach_strong_only: reach_strong,
            reach_weak_only: reach_weak,
            cascade_rounds,
        }
    }
}

/// 辺リスト (edges.csv の 1 行)．
#[derive(Debug, Clone, Serialize)]
pub struct EdgeRow {
    pub a: u64,
    pub b: u64,
    pub strength: &'static str,
}

/// 網から辺リストを抽出する (`weighted_edges` を使用)．
pub fn edge_rows(net: &WeightedNetwork<TieStrength>) -> Vec<EdgeRow> {
    let mut rows: Vec<EdgeRow> = net
        .weighted_edges()
        .map(|(a, b, w)| EdgeRow {
            a: a.0,
            b: b.0,
            strength: w.label(),
        })
        .collect();
    rows.sort_by_key(|r| (r.a, r.b));
    rows
}

/// クラスタ割当リスト (nodes.csv 相当; 可視化のレイアウト着色用)．
pub fn cluster_map(world: &WeakTieWorld) -> BTreeMap<u64, usize> {
    world.cluster_of.iter().map(|(id, &c)| (id.0, c)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use socsim_net::WeightedNetwork;

    /// 2 三角形を 1 本の弱紐帯ブリッジで繋いだ網．
    fn two_triangles() -> WeightedNetwork<TieStrength> {
        let mut net: WeightedNetwork<TieStrength> = WeightedNetwork::empty();
        for i in 0..6u64 {
            net.add_node(AgentId(i));
        }
        for (a, b) in [(0, 1), (1, 2), (0, 2), (3, 4), (4, 5), (3, 5)] {
            net.add_edge_weighted(AgentId(a), AgentId(b), TieStrength::Strong);
        }
        net.add_edge_weighted(AgentId(2), AgentId(3), TieStrength::Weak);
        net
    }

    #[test]
    fn all_bridges_are_weak() {
        let net = two_triangles();
        assert_eq!(n_local_bridges(&net), 1);
        assert_eq!(frac_weak_bridges(&net), 1.0);
    }

    #[test]
    fn forbidden_triad_low_in_cliquey_clusters() {
        // 三角形内は強紐帯クリークなので禁制三者は 0．
        let net = two_triangles();
        // A=0 の強紐帯ペア (1,2) は 1–2 辺が存在 → 禁制でない．
        assert_eq!(forbidden_triad_rate(&net), 0.0);
    }

    #[test]
    fn reach_by_strength_weak_bridges_dominate() {
        let net = two_triangles();
        // シード 0 から: 強紐帯のみだと {1,2} の 2 人，弱紐帯のみだと 0 人 (0 に弱紐帯なし)．
        let (s, w) = reach_by_strength(&net, &[AgentId(0)]);
        assert_eq!(s, 2);
        assert_eq!(w, 0);
        // シード 2 からは弱紐帯 2–3 で 3 へ到達．
        let (_, w2) = reach_by_strength(&net, &[AgentId(2)]);
        assert_eq!(w2, 1);
    }
}
