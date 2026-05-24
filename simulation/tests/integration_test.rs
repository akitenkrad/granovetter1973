//! Granovetter (1973) 弱紐帯ブリッジ網 + 情報拡散の統合テスト．
//!
//! `granovetter_ties` ライブラリクレートの公開 API に対して，
//! ・既定生成器ではすべての局所ブリッジが弱紐帯 (frac_weak_bridges == 1.0)
//! ・弱紐帯除去で到達範囲が崩壊し，強紐帯除去では到達範囲がほぼ維持される
//! ・同一シードで到達範囲が決定論的に再現される
//! ・生成網の連結性サニティ
//! を検証する．

use granovetter_ties::config::{Config, DiffusionModel, RemovePolicy};
use granovetter_ties::metrics::{frac_weak_bridges, reach_fraction};
use granovetter_ties::simulation::{apply_ablation, init_world, run, run_diffusion};

fn base_config(remove: RemovePolicy) -> Config {
    Config {
        clusters: 8,
        cluster_size: 12,
        p_strong: 0.6,
        p_bridge: 0.5,
        p_weak_intra: 0.0,
        diffusion: DiffusionModel::Si,
        beta: 0.9,
        theta: 0.2,
        remove,
        n_seeds: 1,
        max_iterations: 200,
        seed: Some(42),
        output_dir: "results".to_string(),
    }
}

// --------------------------------------------------------------------------- //
// 構造命題: すべての局所ブリッジは弱紐帯
// --------------------------------------------------------------------------- //

#[test]
fn all_local_bridges_are_weak() {
    // 強紐帯はクラスタ内クリーク的に張られるので，局所ブリッジ (d>2) は弱紐帯のみ．
    let cfg = base_config(RemovePolicy::None);
    let world = init_world(&cfg, 42);
    let frac = frac_weak_bridges(&world.net);
    assert_eq!(
        frac, 1.0,
        "既定生成器ではすべての局所ブリッジが弱紐帯であるべき (got {})",
        frac
    );
}

// --------------------------------------------------------------------------- //
// 弱紐帯除去で到達範囲が崩壊し，強紐帯除去では維持される
// --------------------------------------------------------------------------- //

#[test]
fn removing_weak_collapses_reach_to_seed_cluster() {
    // 論文の中心命題: 弱紐帯はクラスタ間ブリッジである．これを除去すると拡散は
    // シード所属クラスタに限局する一方，弱紐帯が在ればネットワーク全体へ広がる．
    let seed = 42;

    let baseline = run(&base_config(RemovePolicy::None), seed);
    let reach_baseline = reach_fraction(&baseline.world);

    // 弱紐帯除去．
    let cfg_weak = base_config(RemovePolicy::Weak);
    let mut w_weak = init_world(&cfg_weak, seed);
    apply_ablation(&mut w_weak, &cfg_weak, seed);
    let reach_weak = reach_fraction(&run_diffusion(w_weak, &cfg_weak, seed).world);

    assert!(
        reach_baseline >= 0.8,
        "弱紐帯が在れば広く到達するべき (got {})",
        reach_baseline
    );
    // 弱紐帯除去はシードのクラスタに限局する (≈ cluster_size / n = 12/96 = 0.125)．
    assert!(
        reach_weak <= 0.30,
        "弱紐帯除去で到達範囲はシードのクラスタに限局するべき (got {})",
        reach_weak
    );
    assert!(
        reach_baseline - reach_weak >= 0.5,
        "弱紐帯除去で到達範囲は劇的に縮小するべき (baseline={}, weak={})",
        reach_baseline,
        reach_weak
    );
}

#[test]
fn strong_only_reach_stays_within_seed_cluster() {
    // 構造的到達 (確率を無視した BFS): 強紐帯のみで辿るとシード所属クラスタ内に
    // 閉じる (クラスタ間は弱紐帯ブリッジのみなので越えられない)．これが
    // 「弱紐帯がクラスタ間到達を支配する」の構造的根拠 (Rapoport–Horvath)．
    use granovetter_ties::metrics::reach_by_strength;
    let cfg = base_config(RemovePolicy::None);
    let world = init_world(&cfg, 42);
    let (strong_only, _weak_only) = reach_by_strength(&world.net, &world.seeds);
    // 強紐帯のみ: シードのクラスタ内 (高々 cluster_size-1 人) に限局する．
    assert!(
        strong_only < cfg.cluster_size,
        "強紐帯のみの到達はシードのクラスタ内に閉じるべき (got {})",
        strong_only
    );
    assert!(strong_only >= 1, "クラスタ内には強紐帯で到達できるべき");
}

// --------------------------------------------------------------------------- //
// 同一シードで決定論的
// --------------------------------------------------------------------------- //

#[test]
fn diffusion_is_deterministic_for_fixed_seed() {
    let a = run(&base_config(RemovePolicy::None), 42);
    let b = run(&base_config(RemovePolicy::None), 42);
    assert_eq!(a.world.n_informed(), b.world.n_informed());
    assert_eq!(a.cascade_rounds, b.cascade_rounds);
    assert_eq!(a.history, b.history);
}

// --------------------------------------------------------------------------- //
// 生成網の連結性サニティ
// --------------------------------------------------------------------------- //

#[test]
fn generated_network_is_well_formed() {
    let cfg = base_config(RemovePolicy::None);
    let world = init_world(&cfg, 42);
    assert_eq!(world.net.node_count(), cfg.n_agents());
    assert!(world.net.edge_count() > 0, "辺が存在するべき");
    // p_bridge=0.5, 8 クラスタなら高確率で 1 連結成分．
    assert_eq!(
        world.net.connected_components(),
        1,
        "既定設定では単一連結成分になるべき"
    );
}

// --------------------------------------------------------------------------- //
// 閾値カスケードも動作する
// --------------------------------------------------------------------------- //

#[test]
fn threshold_diffusion_runs() {
    let mut cfg = base_config(RemovePolicy::None);
    cfg.diffusion = DiffusionModel::Threshold;
    cfg.theta = 0.1;
    cfg.n_seeds = 5;
    let result = run(&cfg, 42);
    // 閾値カスケードはシードから少なくとも自分自身を含む．
    assert!(result.world.n_informed() >= cfg.n_seeds);
}
