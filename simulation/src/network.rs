//! 弱紐帯ブリッジ網生成器 (caveman/clustered communities + sparse weak bridges)．
//!
//! 論文の構造を以下で操作化する:
//!
//! 1. `K` 個のクラスタ (コミュニティ) を作り，各クラスタ内を**強い紐帯**で密に結ぶ
//!    (クラスタ内辺確率 `p_strong` の ER; `p_strong=1.0` で準クリーク)．これが局所
//!    凝集を生む．
//! 2. クラスタ間を**弱い紐帯**で疎に橋渡しする．クラスタ対ごとに確率 `p_bridge` で
//!    1 本の弱紐帯を張る (両端は各クラスタの一様乱択ノード)．これがブリッジ /
//!    局所ブリッジ候補となる．
//! 3. 必要なら少数の**クラスタ内弱紐帯**も許す (`p_weak_intra`; 既定 0.0)．
//!
//! 禁じられた三者関係 (A–B 強・A–C 強・C–B 欠如) を抑えるため，強紐帯はクラスタ内
//! クリーク的に張る (`p_strong` を高くする)．これにより強紐帯がブリッジになり得ない
//! 構造が生まれ，「すべての (局所) ブリッジは弱紐帯」という命題が高確率で成立する．
//!
//! 紐帯強度は [`TieStrength`] を辺重みとして [`WeightedNetwork::add_edge_weighted`]
//! で書き込む (side-table なし)．

use std::collections::BTreeMap;

use rand::Rng;
use socsim_core::{AgentId, SimRng};
use socsim_net::WeightedNetwork;

use crate::config::Config;
use crate::world::TieStrength;

/// 生成された網と所属クラスタ表．
pub struct GeneratedNetwork {
    pub net: WeightedNetwork<TieStrength>,
    pub cluster_of: BTreeMap<AgentId, usize>,
}

/// 設定と RNG から弱紐帯ブリッジ網を生成する．
///
/// 強紐帯はクラスタ内 ER (確率 `p_strong`)，弱紐帯はクラスタ対間の橋渡し
/// (確率 `p_bridge`) と少数のクラスタ内弱紐帯 (確率 `p_weak_intra`)．
pub fn generate(cfg: &Config, rng: &mut SimRng) -> GeneratedNetwork {
    generate_observed(cfg, rng, |_| {})
}

/// The same, calling `on_cluster` once for every cluster wired.
///
/// The callback is where a caller counts its progress. A cluster is the unit
/// because it is the unit the cost is in: wiring one is quadratic in its size
/// and then [`close_triangles`] iterates to a fixed point over the same
/// members, and everything else a trial does is small beside it. Measured at
/// `--clusters 10`, one trial takes 1.11s at `--cluster-size 100` and 102s at
/// `--cluster-size 400`, while `--beta` — which decides how many diffusion
/// rounds follow — moves the same trial only from 93.06s to 97.03s.
///
/// It is given the cluster index rather than nothing so a caller can report
/// against the grid rather than against its own tally.
pub fn generate_observed(
    cfg: &Config,
    rng: &mut SimRng,
    mut on_cluster: impl FnMut(usize),
) -> GeneratedNetwork {
    let k = cfg.clusters;
    let size = cfg.cluster_size;
    let n = k * size;

    let ids: Vec<AgentId> = (0..n as u64).map(AgentId).collect();
    let mut net: WeightedNetwork<TieStrength> = WeightedNetwork::empty();
    for &id in &ids {
        net.add_node(id);
    }

    // クラスタ割当: エージェント c*size .. (c+1)*size がクラスタ c．
    let mut cluster_of: BTreeMap<AgentId, usize> = BTreeMap::new();
    // 各クラスタのメンバ ID リスト (橋渡し端点の乱択用)．
    let mut members: Vec<Vec<AgentId>> = vec![Vec::with_capacity(size); k];
    for c in 0..k {
        for j in 0..size {
            let id = ids[c * size + j];
            cluster_of.insert(id, c);
            members[c].push(id);
        }
    }

    // 1. クラスタ内強紐帯．
    //
    //    (a) 強紐帯リング (隣接 i↔i+1 を強紐帯で結ぶ) でクラスタを連結にする．
    //    (b) 残りの対を ER (確率 p_strong) で強紐帯化する (密度の調整つまみ)．
    //    (c) **三角形閉包パス**: 共通隣接を持たない強紐帯 (= クラスタ内局所ブリッジに
    //        なってしまう辺) を，端点の隣接 1 つと結んで三角形に閉じる．
    //
    //    これにより強紐帯は常に三角形の一部となり (clique-ish)，**クラスタ内に局所
    //    ブリッジが生じない**ことが保証される (設計書 §4.3 の「強紐帯はブリッジに
    //    なり得ない」)．結果として局所ブリッジはクラスタ間の弱紐帯のみとなり，
    //    `frac_weak_bridges == 1.0` が安定して成立する．`p_strong` を下げると禁制
    //    三者が増えるが連結リングと閉包により命題自体は保たれる (前提依存性は
    //    `forbidden_triad_rate` で計測する)．
    let p_strong = cfg.p_strong.clamp(0.0, 1.0);
    let p_weak_intra = cfg.p_weak_intra.clamp(0.0, 1.0);
    for (cluster_idx, cluster_members) in members.iter().take(k).enumerate() {
        let m = cluster_members.clone();
        let sz = m.len();
        // (a) 強紐帯リングで連結を保証 (size>=3 なら閉路, size==2 なら 1 辺)．
        if sz >= 2 {
            for i in 0..sz {
                let j = (i + 1) % sz;
                if sz == 2 && j < i {
                    continue; // 2 ノードは 1 辺で十分．
                }
                net.add_edge_weighted(m[i], m[j], TieStrength::Strong);
            }
        }
        // (b) ER で強紐帯を追加 (密度つまみ)．
        for i in 0..sz {
            for j in (i + 1)..sz {
                if net.edge_weight(m[i], m[j]).is_some() {
                    continue;
                }
                if rng.gen::<f64>() < p_strong {
                    net.add_edge_weighted(m[i], m[j], TieStrength::Strong);
                } else if p_weak_intra > 0.0 && rng.gen::<f64>() < p_weak_intra {
                    // 強紐帯を張らなかった対に少数の弱紐帯を許す (連続性近似)．
                    net.add_edge_weighted(m[i], m[j], TieStrength::Weak);
                }
            }
        }
        // (c) 三角形閉包: 各強紐帯 (u,v) が共通隣接を持たなければ三角形に閉じる．
        if sz >= 3 {
            close_triangles(&mut net, &m);
        }
        on_cluster(cluster_idx);
    }

    // 2. クラスタ間弱紐帯橋渡し (クラスタ対ごとに確率 p_bridge で 1 本)．
    let p_bridge = cfg.p_bridge.clamp(0.0, 1.0);
    for a in 0..k {
        for b in (a + 1)..k {
            if rng.gen::<f64>() < p_bridge {
                let ia = rng.gen_range(0..members[a].len());
                let ib = rng.gen_range(0..members[b].len());
                let u = members[a][ia];
                let v = members[b][ib];
                // 既存辺があっても弱紐帯で上書きする (橋渡し優先)．
                net.add_edge_weighted(u, v, TieStrength::Weak);
            }
        }
    }

    GeneratedNetwork { net, cluster_of }
}

/// クラスタ `members` 内の各強紐帯について，共通隣接 (三角形) を持たないものを
/// 強紐帯 1 本の追加で三角形に閉じる．
///
/// これにより，クラスタ内のいかなる強紐帯も局所ブリッジ
/// ([`WeightedNetwork::is_local_bridge`] = 共通隣接なし) にならなくなる．閉包で
/// 追加した辺自身も検査対象に含めて固定点まで反復する (members は小さいので安価)．
fn close_triangles(net: &mut WeightedNetwork<TieStrength>, members: &[AgentId]) {
    use std::collections::BTreeSet;
    let member_set: BTreeSet<AgentId> = members.iter().copied().collect();
    loop {
        // 共通隣接を持たないクラスタ内強紐帯を 1 本探す．
        let mut fix: Option<(AgentId, AgentId, AgentId)> = None;
        'outer: for &u in members {
            let un: BTreeSet<AgentId> = net.neighbors_iter(u).collect();
            for &v in &un {
                if !member_set.contains(&v) || v <= u {
                    continue; // クラスタ内・正準向きのみ．
                }
                if net.edge_weight(u, v) != Some(&TieStrength::Strong) {
                    continue;
                }
                let vn: BTreeSet<AgentId> = net.neighbors_iter(v).collect();
                if un.intersection(&vn).next().is_some() {
                    continue; // 共通隣接あり → すでに三角形．
                }
                // 共通隣接なし → クラスタ内の別ノード w で三角形を作る．
                if let Some(&w) = members.iter().find(|&&w| w != u && w != v) {
                    fix = Some((u, v, w));
                    break 'outer;
                }
            }
        }
        match fix {
            Some((u, v, w)) => {
                net.add_edge_weighted(u, w, TieStrength::Strong);
                net.add_edge_weighted(v, w, TieStrength::Strong);
            }
            None => break,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use socsim_core::derive_seed;

    fn cfg() -> Config {
        Config {
            clusters: 5,
            cluster_size: 8,
            p_strong: 0.7,
            p_bridge: 0.5,
            p_weak_intra: 0.0,
            ..Config::default()
        }
    }

    #[test]
    fn node_count_matches() {
        let mut rng = SimRng::from_seed(derive_seed(1, &[0]));
        let g = generate(&cfg(), &mut rng);
        assert_eq!(g.net.node_count(), 5 * 8);
        assert_eq!(g.cluster_of.len(), 5 * 8);
    }

    #[test]
    fn intra_edges_are_strong_inter_edges_are_weak() {
        let mut rng = SimRng::from_seed(derive_seed(2, &[0]));
        let g = generate(&cfg(), &mut rng);
        for (a, b, w) in g.net.weighted_edges() {
            let ca = g.cluster_of[&a];
            let cb = g.cluster_of[&b];
            if ca == cb {
                // クラスタ内: p_weak_intra=0 なので強紐帯のみ．
                assert_eq!(*w, TieStrength::Strong);
            } else {
                // クラスタ間: 弱紐帯のみ．
                assert_eq!(*w, TieStrength::Weak);
            }
        }
    }

    #[test]
    fn deterministic_for_same_seed() {
        let mut r1 = SimRng::from_seed(derive_seed(7, &[0]));
        let mut r2 = SimRng::from_seed(derive_seed(7, &[0]));
        let g1 = generate(&cfg(), &mut r1);
        let g2 = generate(&cfg(), &mut r2);
        let mut e1: Vec<_> = g1
            .net
            .weighted_edges()
            .map(|(a, b, w)| (a, b, *w))
            .collect();
        let mut e2: Vec<_> = g2
            .net
            .weighted_edges()
            .map(|(a, b, w)| (a, b, *w))
            .collect();
        e1.sort_by_key(|(a, b, _)| (a.0, b.0));
        e2.sort_by_key(|(a, b, _)| (a.0, b.0));
        assert_eq!(e1, e2);
    }
}
