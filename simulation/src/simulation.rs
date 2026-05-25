//! 初期化と実行ドライバ (SimulationBuilder 配線)．

use std::fs::File;
use std::io::BufWriter;

use csv::Writer;

use socsim_core::{derive_seed, AgentId, Mechanism, SimRng};
use socsim_engine::{RandomActivationScheduler, SimulationBuilder};
use socsim_social_dynamics::{SiContagionMechanism, ThresholdContagionMechanism};

use crate::config::{Config, DiffusionModel, RemovePolicy};
use crate::metrics::{edge_rows, Metrics};
use crate::network::{self, GeneratedNetwork};
use crate::world::{TieStrength, WeakTieWorld};

// 単一 root シードから用途別の独立な決定論的 RNG ストリームを派生させるラベル．
/// 網生成・シード選択用 RNG ラベル．
const RNG_WORLD_INIT: u64 = 0;
/// socsim エンジン (= SI 感染確率の抽出・活性化順) 用 RNG ラベル．
const RNG_ENGINE: u64 = 1;
/// アブレーションのランダム除去用 RNG ラベル．
const RNG_ABLATION: u64 = 2;

/// シミュレーション全体の実行結果．
pub struct SimulationResult {
    /// 飽和時の世界状態 (メトリクス計算・edges.csv 用)．
    pub world: WeakTieWorld,
    /// 各ラウンドの到達数履歴 (n_informed_history)．
    pub history: Vec<usize>,
    /// 飽和までのラウンド数．
    pub cascade_rounds: usize,
    /// 使用した root シード．
    pub seed: u64,
}

/// 設定から世界状態を初期化する (網生成 + シード選択)．
///
/// 網生成・シード選択は `derive_seed(root, &[0])` の init ストリームで行う．
/// シードは ID 0 を含む先頭 `n_seeds` 個 (クラスタ 0 から拡散開始; 弱紐帯除去の効果を
/// クラスタ間到達の崩壊として観察しやすくするため固定する)．
pub fn init_world(cfg: &Config, root: u64) -> WeakTieWorld {
    let mut init_rng = SimRng::from_seed(derive_seed(root, &[RNG_WORLD_INIT]));
    let GeneratedNetwork { net, cluster_of } = network::generate(cfg, &mut init_rng);

    let n = cfg.n_agents();
    let n_seeds = cfg.n_seeds.clamp(1, n.max(1));
    let seeds: Vec<AgentId> = (0..n_seeds as u64).map(AgentId).collect();

    WeakTieWorld::new(net, cluster_of, seeds, cfg.max_iterations as u64)
}

/// アブレーション方策に従って世界状態の網から辺を除去する．
///
/// `remove_edge` を用いて該当辺を消す (世界状態を破壊的に変更)．
/// `Random` は弱紐帯と同数の辺をランダムに除去する (対照群)．
pub fn apply_ablation(world: &mut WeakTieWorld, cfg: &Config, root: u64) {
    let all_edges: Vec<(AgentId, AgentId, TieStrength)> = world
        .net
        .weighted_edges()
        .map(|(a, b, w)| (a, b, *w))
        .collect();

    let to_remove: Vec<(AgentId, AgentId)> = match cfg.remove {
        RemovePolicy::None => Vec::new(),
        RemovePolicy::Weak => all_edges
            .iter()
            .filter(|(_, _, w)| *w == TieStrength::Weak)
            .map(|(a, b, _)| (*a, *b))
            .collect(),
        RemovePolicy::Strong => all_edges
            .iter()
            .filter(|(_, _, w)| *w == TieStrength::Strong)
            .map(|(a, b, _)| (*a, *b))
            .collect(),
        RemovePolicy::Random => {
            use rand::seq::SliceRandom;
            let n_weak = all_edges
                .iter()
                .filter(|(_, _, w)| *w == TieStrength::Weak)
                .count();
            let mut rng = SimRng::from_seed(derive_seed(root, &[RNG_ABLATION]));
            let mut pairs: Vec<(AgentId, AgentId)> =
                all_edges.iter().map(|(a, b, _)| (*a, *b)).collect();
            pairs.shuffle(&mut rng);
            pairs.into_iter().take(n_weak).collect()
        }
    };

    for (a, b) in to_remove {
        world.net.remove_edge(a, b);
    }
}

/// シミュレーションを実行する (網生成は呼び出し側が `init_world` で済ませて渡す)．
///
/// socsim の [`Simulation`](socsim_engine::Simulation) エンジンを駆動する．拡散の状態更新は
/// 汎用 social-dynamics メカニズム ([`SiContagionMechanism`] / [`ThresholdContagionMechanism`])
/// に委譲する．両者とも `Interaction` フェーズで同期ラウンドを適用し，SI は
/// `ctx.agent_order` (= [`RandomActivationScheduler`] のシャッフル順) を走査して各 inactive
/// エージェント × active 隣接で独立 Bernoulli(β) を引き，1 つでも成功すれば感染する
/// (break-on-first-success)．閾値は `active/max(deg,1) ≥ θ` で決定論的に活性化する．
/// 飽和 (新規 0) または全到達でメカニズムが `request_stop` する．
///
/// 旧 repo 固有 `DiffusionMechanism` と同一の隣接走査順 (`Neighbors::neighbors_of` =
/// `net.neighbors`) ・RNG 抽出順・飽和判定を保つため出力はバイト等価である．各ラウンド後の
/// informed 数を `n_informed_history` へ記録する処理だけは観測コールバックで肩代わりする
/// (メカニズム自体は履歴を持たないため)．
pub fn run_diffusion(world: WeakTieWorld, cfg: &Config, root: u64) -> SimulationResult {
    let mechanism: Box<dyn Mechanism<WeakTieWorld>> = match cfg.diffusion {
        DiffusionModel::Si => Box::new(SiContagionMechanism::new(cfg.beta)),
        DiffusionModel::Threshold => Box::new(ThresholdContagionMechanism::new(cfg.theta)),
    };

    let mut sim = SimulationBuilder::new(world)
        .scheduler(Box::new(RandomActivationScheduler))
        .seed(derive_seed(root, &[RNG_ENGINE]))
        .add_mechanism(mechanism)
        .build();

    let mut cascade_rounds = 0usize;
    let mut history: Vec<usize> = vec![sim.world().n_informed()];
    sim.run_observed(|report| {
        cascade_rounds = report.t as usize;
        history.push(report.world.n_informed());
    })
    .expect("シミュレーションの実行に失敗");

    let mut world = sim.world().clone();
    // 旧メカニズムは world.n_informed_history を維持していた．出力には現れないが
    // 世界状態フィールドとしての意味 (各ラウンドの到達数) を保つため再構成する．
    world.n_informed_history = history.clone();
    SimulationResult {
        world,
        history,
        cascade_rounds,
        seed: root,
    }
}

/// 設定から 1 試行を最初から最後まで実行する (init → ablation → diffusion)．
pub fn run(cfg: &Config, root: u64) -> SimulationResult {
    let mut world = init_world(cfg, root);
    if cfg.remove != RemovePolicy::None {
        apply_ablation(&mut world, cfg, root);
    }
    run_diffusion(world, cfg, root)
}

/// メトリクス履歴を CSV に保存する．
///
/// 各行を `serialize` し先頭行にヘッダを書く csv クレートの標準挙動を
/// `socsim_results::write_csv` に委譲する (従来の手書き writer とバイト等価)．
/// 行構造体 [`Metrics`] は repo 固有のままで，writer だけを共有化する．
pub fn save_metrics(metrics: &[Metrics], output_dir: &str) {
    let path = format!("{}/metrics.csv", output_dir);
    socsim_results::write_csv(metrics, &path).expect("metrics.csv の書き込みに失敗");
}

/// 網の辺リストを edges.csv に保存する (a, b, strength)．
///
/// `edge_rows` の serde 行を `socsim_results::write_csv` で書き出す
/// (従来の手書き writer とバイト等価)．
pub fn save_edges(world: &WeakTieWorld, output_dir: &str) {
    let path = format!("{}/edges.csv", output_dir);
    socsim_results::write_csv(&edge_rows(&world.net), &path).expect("edges.csv の書き込みに失敗");
}

/// ノードのクラスタ割当を nodes.csv に保存する (id, cluster, is_seed)．
pub fn save_nodes(world: &WeakTieWorld, output_dir: &str) {
    let path = format!("{}/nodes.csv", output_dir);
    let file = File::create(&path).expect("nodes.csv の作成に失敗");
    let mut wtr = Writer::from_writer(BufWriter::new(file));
    wtr.write_record(["id", "cluster", "is_seed"])
        .expect("ヘッダ書き込みに失敗");
    let seeds: std::collections::BTreeSet<AgentId> = world.seeds.iter().copied().collect();
    for (&id, &c) in &world.cluster_of {
        wtr.write_record(&[
            id.0.to_string(),
            c.to_string(),
            if seeds.contains(&id) { "1" } else { "0" }.to_string(),
        ])
        .expect("レコード書き込みに失敗");
    }
    wtr.flush().expect("フラッシュに失敗");
}

/// 出力ディレクトリを作成する．
pub fn ensure_output_dir(output_dir: &str) {
    socsim_results::ensure_dir(output_dir).expect("出力ディレクトリの作成に失敗");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::DiffusionModel;

    fn test_config() -> Config {
        Config {
            clusters: 5,
            cluster_size: 10,
            p_strong: 0.6,
            p_bridge: 0.5,
            p_weak_intra: 0.0,
            diffusion: DiffusionModel::Si,
            beta: 0.9,
            theta: 0.2,
            remove: RemovePolicy::None,
            n_seeds: 1,
            max_iterations: 200,
            seed: Some(42),
            output_dir: "results".to_string(),
        }
    }

    #[test]
    fn same_seed_is_deterministic() {
        let a = run(&test_config(), 42);
        let b = run(&test_config(), 42);
        assert_eq!(a.world.n_informed(), b.world.n_informed());
        assert_eq!(a.cascade_rounds, b.cascade_rounds);
        assert_eq!(a.history, b.history);
    }

    #[test]
    fn history_starts_with_seed_count() {
        let r = run(&test_config(), 42);
        assert_eq!(r.history[0], 1); // n_seeds=1．
    }
}
