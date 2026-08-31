//! Granovetter (1973/1978) — 論文の主要な定量的主張の一括再現 (`reproduce`)．
//!
//! 本モジュールは網生成 + 拡散を内部で決定論的に走らせ，論文が狙う 2 つの
//! ヘッドライン主張を「観測値 vs 期待値 + PASS/off 判定」として集計する．
//! 外部プロセス (subprocess) を介さずライブラリ呼び出しで完結するため，秒境界
//! 衝突やビルド連鎖がなく，すべて seed 固定で再現性がある．
//!
//! 再現する主張:
//!
//! - **Claim A — 弱紐帯ブリッジ効果 (1973 ファクト7 + 中心命題)**: ベースライン
//!   到達割合は ≈1.0 だが，弱紐帯 (= すべての局所ブリッジ) を除去すると到達は
//!   シード所属クラスタに限局し ≈1/K へ急落する．同数の辺をランダム除去した
//!   対照群では到達はほぼ不変 (≈1.0)．加えて局所ブリッジの弱紐帯率は 1.0．
//! - **Claim B — 閾値カスケードのティッピング (Granovetter 1978)**: 閾値分布
//!   (一様 θ) をわずかに上へずらすと，最終カスケードサイズが大域カスケード
//!   (reach≈1.0) から局所カスケード (reach≈1/K) へ大きく跳ぶ．低 θ では reach≥0.9，
//!   高 θ では reach≤0.2 で，遷移は狭い θ 帯に集中する．
//!
//! 出力 (runvault の run ディレクトリ):
//!
//! - `metrics.csv` — run に 1 つしか無い値だけ (両 claim のヘッドライン)．
//! - `events.jsonl` — 除去方策ごと / θ ごとの 1 行．どちらも時間軸を持たないので
//!   `metrics.csv` に並べると主キーが全行で同じになり衝突する．旧
//!   `claim_a_ablation.csv` / `claim_b_threshold.csv` の中身がここに入る．
//! - `artifacts/reproduce_summary.json` — 各 claim の params + 観測値 vs 期待値 +
//!   verdict．許容幅と PASS/OFF は指標でも報告値でもないので artifacts に置く．
//!
//! 分野は `simulation`．突き合わせだけの決定的計算ではなく，網生成と拡散を
//! 自分で回して乱数を引くので `master_seed` が実在する．

use serde::Serialize;
use socsim_core::derive_seed;

use runvault::{Run, RunOptions};

use crate::config::{Config, DiffusionModel, RemovePolicy};
use crate::metrics::{frac_weak_bridges, reach_fraction};
use crate::record::{
    self, ABLATION_CONDITION_EVENT, DOMAIN, EXPERIMENT, REPO_ID, THRESHOLD_POINT_EVENT,
};
use crate::simulation::{apply_ablation, ensure_output_dir, init_world, run_diffusion};

// ---------------------------------------------------------------------------
// 共通: 試行平均ヘルパ
// ---------------------------------------------------------------------------

/// `(seed, run)` から各試行に独立なシードを派生させる (explicit identity)．
fn run_seed(root: u64, run_idx: usize) -> u64 {
    derive_seed(root, &[run_idx as u64])
}

/// 指定除去方策の到達割合を試行平均する (除去なしのベースラインも同時に返す)．
fn mean_reach_with_removal(
    base_cfg: &Config,
    remove: RemovePolicy,
    runs: usize,
    root: u64,
) -> (f64, f64, f64) {
    let mut sum_baseline = 0.0_f64;
    let mut sum_removed = 0.0_f64;
    let mut sum_frac_weak = 0.0_f64;
    for run_idx in 0..runs {
        let seed = run_seed(root, run_idx);

        // ベースライン (除去なし)．
        let mut cfg = base_cfg.clone();
        cfg.remove = RemovePolicy::None;
        let baseline_world = init_world(&cfg, seed);
        sum_frac_weak += frac_weak_bridges(&baseline_world.net);
        let baseline = run_diffusion(baseline_world, &cfg, seed);
        sum_baseline += reach_fraction(&baseline.world);

        // 除去後．
        let mut cfg_rm = base_cfg.clone();
        cfg_rm.remove = remove;
        let mut world = init_world(&cfg_rm, seed);
        apply_ablation(&mut world, &cfg_rm, seed);
        let removed = run_diffusion(world, &cfg_rm, seed);
        sum_removed += reach_fraction(&removed.world);
    }
    let n = runs as f64;
    (sum_baseline / n, sum_removed / n, sum_frac_weak / n)
}

/// 指定 θ の閾値カスケードの到達割合・カスケードラウンドを試行平均する．
fn mean_reach_threshold(base_cfg: &Config, theta: f64, runs: usize, root: u64) -> (f64, f64) {
    let mut sum_reach = 0.0_f64;
    let mut sum_rounds = 0.0_f64;
    for run_idx in 0..runs {
        let seed = run_seed(root, run_idx);
        let mut cfg = base_cfg.clone();
        cfg.diffusion = DiffusionModel::Threshold;
        cfg.theta = theta;
        cfg.remove = RemovePolicy::None;
        let result = run_diffusion(init_world(&cfg, seed), &cfg, seed);
        sum_reach += reach_fraction(&result.world);
        sum_rounds += result.cascade_rounds as f64;
    }
    let n = runs as f64;
    (sum_reach / n, sum_rounds / n)
}

// ---------------------------------------------------------------------------
// CSV 行 / JSON サマリ構造体
// ---------------------------------------------------------------------------

/// Claim A の 1 条件 (除去方策ごと) — `events.jsonl` の 1 行．
///
/// 除去方策は «1 回の実行の中で観測された対象» なので予約語 `unit_id` を持つ．
/// 4 条件を `metrics.csv` に並べると主キー (`name`, `step`, `step_unit`, `scope`)
/// が全行で同じになって衝突し，かといって子 run に割れば «起きていない 4 つの
/// 実行» を主張することになる (1 回の `reproduce` は 4 条件をひと続きに測る)．
#[derive(Serialize)]
struct AblationConditionEvent {
    unit_id: &'static str,
    remove: &'static str,
    baseline_reach: f64,
    removed_reach: f64,
    delta_reach: f64,
    frac_weak_bridges: f64,
}

/// Claim B の 1 点 (θ ごと) — `events.jsonl` の 1 行．
///
/// θ は時間軸ではないので `step` にはできない．`cascade_rounds` は試行平均で，
/// «その条件で測れた値» であって刻みではない．
#[derive(Serialize)]
struct ThresholdPointEvent {
    unit_id: String,
    theta: f64,
    reach: f64,
    cascade_rounds: f64,
}

/// 1 つの主張の観測 vs 期待 + 判定．
#[derive(Serialize)]
struct ClaimVerdict {
    id: &'static str,
    description: String,
    expectation: String,
    observed: String,
    verdict: &'static str,
}

/// `artifacts/reproduce_summary.json` のトップレベル構造．
#[derive(Serialize)]
struct ReproduceSummary {
    run_slug: String,
    quick: bool,
    seed: u64,
    network: NetworkParams,
    claim_a: ClaimAResult,
    claim_b: ClaimBResult,
    claims: Vec<ClaimVerdict>,
}

#[derive(Serialize)]
struct NetworkParams {
    clusters: usize,
    cluster_size: usize,
    p_strong: f64,
    p_bridge: f64,
    runs: usize,
}

#[derive(Serialize)]
struct ClaimAResult {
    runs: usize,
    p_bridge: f64,
    beta: f64,
    baseline_reach: f64,
    weak_removed_reach: f64,
    strong_removed_reach: f64,
    random_removed_reach: f64,
    frac_weak_bridges: f64,
    expected_local_reach: f64,
}

#[derive(Serialize)]
struct ClaimBResult {
    runs: usize,
    p_bridge: f64,
    n_seeds: usize,
    theta_values: Vec<f64>,
    reach_values: Vec<f64>,
    theta_low: f64,
    reach_low: f64,
    theta_high: f64,
    reach_high: f64,
    transition_width: f64,
}

// ---------------------------------------------------------------------------
// reproduce ドライバ
// ---------------------------------------------------------------------------

/// reproduce の実行設定．
pub struct ReproduceOptions {
    pub output_dir: String,
    pub seed: u64,
    pub quick: bool,
}

/// `reproduce` run の `parameters` — 両 claim が使う条件をまとめて持つ．
///
/// `quick` は規模を切り替えるフラグで，条件そのものを変える (クラスタ規模・試行数・
/// θ 解像度)．結果が変わる以上ハッシュから外せないので `parameters` に入れる．
#[derive(Serialize)]
struct ReproduceParameters {
    quick: bool,
    clusters: usize,
    cluster_size: usize,
    p_strong: f64,
    max_iterations: usize,
    runs_a: usize,
    p_bridge_a: f64,
    beta_a: f64,
    runs_b: usize,
    p_bridge_b: f64,
    n_seeds_b: usize,
    theta_values: Vec<f64>,
    seed: u64,
}

/// 論文主要主張の一括再現を実行し，指標・イベント・summary JSON を書き出す．
pub fn run_reproduce(opts: &ReproduceOptions) {
    // quick はクラスタ数・試行数・θ 解像度を縮小する (動作確認用)．
    let clusters = 10usize;
    let cluster_size = if opts.quick { 12 } else { 20 };
    let runs_a = if opts.quick { 5 } else { 20 };
    let runs_b = if opts.quick { 5 } else { 10 };
    let p_strong = 0.6;
    let max_iterations = 200usize;

    let p_bridge_a = 0.5;
    let beta_a = 0.9;
    let p_bridge_b = 0.5;
    let n_seeds_b = cluster_size * clusters / 10; // 各クラスタ平均 ~1 シード相当の密度．
                                                  // quick はクラスタ規模が小さく (cluster_size=12) 崩壊 θ が上にずれるため，
                                                  // 崩壊帯 (θ≈0.10→0.16) を捉える粗いグリッドにする．フル版はより細かい．
    let theta_values: Vec<f64> = if opts.quick {
        vec![0.04, 0.10, 0.12, 0.14, 0.16, 0.20]
    } else {
        vec![0.04, 0.06, 0.07, 0.08, 0.09, 0.10, 0.11, 0.12, 0.15, 0.20]
    };

    let parameters = ReproduceParameters {
        quick: opts.quick,
        clusters,
        cluster_size,
        p_strong,
        max_iterations,
        runs_a,
        p_bridge_a,
        beta_a,
        runs_b,
        p_bridge_b,
        n_seeds_b,
        theta_values: theta_values.clone(),
        seed: opts.seed,
    };

    let mut rv = Run::start(
        RunOptions::new(EXPERIMENT, "reproduce")
            .repo_id(REPO_ID)
            .domain(DOMAIN)
            .results_root(&opts.output_dir)
            .parameters(&parameters)
            .expect("runvault: parameters の組み立てに失敗")
            .seed_pointers(["/seed"])
            .master_seed(opts.seed)
            .replication(record::replication()),
    )
    .expect("runvault: reproduce run の開始に失敗");

    // 判定表は run が走っている間に書くものなので artifacts/ の下へ．
    let artifacts = rv.dir().join("artifacts");
    ensure_output_dir(&artifacts.to_string_lossy());
    let base_dir = artifacts.to_string_lossy().into_owned();

    println!("=== Granovetter (1973/1978) 論文主要主張の一括再現 ===");
    println!("出力先   : {}", rv.dir().display());
    println!("seed     : {}", opts.seed);
    println!("quick    : {}", opts.quick);
    println!(
        "網       : {} クラスタ × {} = {} エージェント | p_strong={}",
        clusters,
        cluster_size,
        clusters * cluster_size,
        p_strong,
    );
    println!("-------------------------------------------------------");

    // -------------------------------------------------------------------
    // Claim A — 弱紐帯ブリッジ効果 (ablation)．
    // -------------------------------------------------------------------
    let cfg_a = Config {
        clusters,
        cluster_size,
        p_strong,
        p_bridge: p_bridge_a,
        p_weak_intra: 0.0,
        diffusion: DiffusionModel::Si,
        beta: beta_a,
        theta: 0.2,
        remove: RemovePolicy::None,
        n_seeds: 1,
        max_iterations,
        seed: Some(opts.seed),
        output_dir: base_dir.clone(),
    };

    println!(
        "--- Claim A: 弱紐帯ブリッジ効果 (p_bridge={p_bridge_a}, β={beta_a}, {runs_a} 試行) ---"
    );
    let (base_w, weak_w, frac_weak) =
        mean_reach_with_removal(&cfg_a, RemovePolicy::Weak, runs_a, opts.seed);
    let (_b2, strong_w, _f2) =
        mean_reach_with_removal(&cfg_a, RemovePolicy::Strong, runs_a, opts.seed);
    let (_b3, random_w, _f3) =
        mean_reach_with_removal(&cfg_a, RemovePolicy::Random, runs_a, opts.seed);
    let expected_local = 1.0 / clusters as f64;

    println!(
        "  ベースライン reach={base_w:.4} | 弱除去 reach={weak_w:.4} (期待 ≈1/K={expected_local:.4}) | 強除去 reach={strong_w:.4} | ランダム除去 reach={random_w:.4}"
    );
    println!("  局所ブリッジ弱紐帯率 frac_weak_bridges={frac_weak:.4}");

    for (remove, removed_reach) in [
        ("none", base_w),
        ("weak", weak_w),
        ("strong", strong_w),
        ("random", random_w),
    ] {
        let event = AblationConditionEvent {
            unit_id: remove,
            remove,
            baseline_reach: base_w,
            removed_reach,
            delta_reach: removed_reach - base_w,
            frac_weak_bridges: frac_weak,
        };
        rv.log_event(ABLATION_CONDITION_EVENT, &event)
            .unwrap_or_else(|e| panic!("除去条件 {remove} の記録に失敗: {e}"));
    }

    // 判定: ベースライン高 + 弱除去で大幅減 + ランダム除去は不変 + frac_weak=1．
    let a_baseline_high = base_w >= 0.9;
    let a_weak_collapse = weak_w <= expected_local + 0.1;
    let a_random_robust = random_w >= 0.9;
    let a_frac_weak = frac_weak >= 0.999;
    let claim_a_pass = a_baseline_high && a_weak_collapse && a_random_robust && a_frac_weak;

    // -------------------------------------------------------------------
    // Claim B — 閾値カスケードのティッピング (Granovetter 1978)．
    // -------------------------------------------------------------------
    let cfg_b = Config {
        clusters,
        cluster_size,
        p_strong,
        p_bridge: p_bridge_b,
        p_weak_intra: 0.0,
        diffusion: DiffusionModel::Threshold,
        beta: 0.5,
        theta: 0.1,
        remove: RemovePolicy::None,
        n_seeds: n_seeds_b,
        max_iterations,
        seed: Some(opts.seed),
        output_dir: base_dir.clone(),
    };

    println!(
        "--- Claim B: 閾値カスケードのティッピング (p_bridge={p_bridge_b}, n_seeds={n_seeds_b}, {runs_b} 試行) ---"
    );
    let mut reach_values: Vec<f64> = Vec::with_capacity(theta_values.len());
    for &theta in &theta_values {
        let (reach, rounds) = mean_reach_threshold(&cfg_b, theta, runs_b, opts.seed);
        println!("  θ={theta:.2}  reach={reach:.4}  cascade_rounds={rounds:.1}");
        let event = ThresholdPointEvent {
            unit_id: format!("theta-{theta}"),
            theta,
            reach,
            cascade_rounds: rounds,
        };
        rv.log_event(THRESHOLD_POINT_EVENT, &event)
            .unwrap_or_else(|e| panic!("θ={theta} の記録に失敗: {e}"));
        reach_values.push(reach);
    }

    // 低 θ の最大 reach 点と高 θ の最小 reach 点を取り，遷移帯の狭さを測る．
    // 「小さな θ シフト → 大きなカスケードサイズの跳び」を定量化する．
    let mut theta_low = theta_values[0];
    let mut reach_low = reach_values[0];
    for (&t, &r) in theta_values.iter().zip(&reach_values) {
        if r >= 0.9 && t >= theta_low {
            theta_low = t;
            reach_low = r;
        }
    }
    let mut theta_high = *theta_values.last().unwrap();
    let mut reach_high = *reach_values.last().unwrap();
    for (&t, &r) in theta_values.iter().zip(&reach_values).rev() {
        if r <= 0.2 && t <= theta_high {
            theta_high = t;
            reach_high = r;
        }
    }
    let transition_width = (theta_high - theta_low).abs();

    println!(
        "  ティッピング: θ_low={theta_low:.2} (reach={reach_low:.3}) → θ_high={theta_high:.2} (reach={reach_high:.3}) | 遷移帯幅 Δθ={transition_width:.3}"
    );

    // 判定: 低 θ で大域 (≥0.9)，高 θ で局所 (≤0.2)，遷移帯が狭い (≤0.07)．
    let b_global_low = reach_low >= 0.9;
    let b_local_high = reach_high <= 0.2;
    let b_narrow = transition_width > 0.0 && transition_width <= 0.07;
    let claim_b_pass = b_global_low && b_local_high && b_narrow;

    // -------------------------------------------------------------------
    // 両 claim のヘッドライン (run に 1 つしか無い値) を metrics.csv へ．
    //
    // `expected_local_reach` (=1/K) と各判定の許容幅はここには置かない．
    // 観測ではなく，こちらが立てた基準だからである (summary JSON の担当)．
    // -------------------------------------------------------------------
    rv.log_metrics(
        "run",
        &[
            ("baseline_reach", base_w),
            ("weak_removed_reach", weak_w),
            ("strong_removed_reach", strong_w),
            ("random_removed_reach", random_w),
            ("frac_weak_bridges", frac_weak),
            ("theta_low", theta_low),
            ("reach_low", reach_low),
            ("theta_high", theta_high),
            ("reach_high", reach_high),
            ("transition_width", transition_width),
        ],
    )
    .expect("run スコープの指標の記録に失敗");

    // -------------------------------------------------------------------
    // サマリ JSON．
    // -------------------------------------------------------------------
    let verdict = |pass: bool| if pass { "PASS" } else { "OFF" };

    let claims = vec![
        ClaimVerdict {
            id: "claim_a_weak_tie_bridges",
            description: "弱紐帯 (= すべての局所ブリッジ) の除去が大域到達を崩壊させ，同数のランダム除去は崩壊させない".to_string(),
            expectation: format!(
                "baseline reach≥0.9, weak-removed reach≤1/K+0.1 (≈{:.2}), random-removed reach≥0.9, frac_weak_bridges=1.0",
                expected_local + 0.1,
            ),
            observed: format!(
                "baseline={base_w:.3}, weak={weak_w:.3}, strong={strong_w:.3}, random={random_w:.3}, frac_weak={frac_weak:.3}"
            ),
            verdict: verdict(claim_a_pass),
        },
        ClaimVerdict {
            id: "claim_b_threshold_tipping",
            description: "閾値分布の小さな上方シフトが最終カスケードサイズを大域→局所へ跳ばせる (Granovetter 1978)".to_string(),
            expectation: "low-θ reach≥0.9, high-θ reach≤0.2, 遷移帯幅 Δθ≤0.07".to_string(),
            observed: format!(
                "θ_low={theta_low:.2} reach={reach_low:.3} → θ_high={theta_high:.2} reach={reach_high:.3}, Δθ={transition_width:.3}"
            ),
            verdict: verdict(claim_b_pass),
        },
    ];

    let summary = ReproduceSummary {
        run_slug: rv.run_slug().to_string(),
        quick: opts.quick,
        seed: opts.seed,
        network: NetworkParams {
            clusters,
            cluster_size,
            p_strong,
            p_bridge: p_bridge_a,
            runs: runs_a,
        },
        claim_a: ClaimAResult {
            runs: runs_a,
            p_bridge: p_bridge_a,
            beta: beta_a,
            baseline_reach: base_w,
            weak_removed_reach: weak_w,
            strong_removed_reach: strong_w,
            random_removed_reach: random_w,
            frac_weak_bridges: frac_weak,
            expected_local_reach: expected_local,
        },
        claim_b: ClaimBResult {
            runs: runs_b,
            p_bridge: p_bridge_b,
            n_seeds: n_seeds_b,
            theta_values: theta_values.clone(),
            reach_values: reach_values.clone(),
            theta_low,
            reach_low,
            theta_high,
            reach_high,
            transition_width,
        },
        claims,
    };

    let summary_path = format!("{}/reproduce_summary.json", base_dir);
    write_json_pretty(&summary, &summary_path);

    println!("-------------------------------------------------------");
    println!(
        "Claim A (弱紐帯ブリッジ効果)        : {}",
        verdict(claim_a_pass)
    );
    println!(
        "Claim B (閾値カスケードのティッピング): {}",
        verdict(claim_b_pass)
    );

    let dir = rv.finish().expect("runvault: reproduce run の完了に失敗");
    println!("条件ごとの観測 → {}/events.jsonl", dir.display());
    println!("ヘッドライン   → {}/metrics.csv", dir.display());
    println!(
        "判定表         → {}/artifacts/reproduce_summary.json",
        dir.display()
    );
}

/// pretty-print JSON を書き出す (旧 `socsim_results::write_json` の置き換え)．
fn write_json_pretty<T: Serialize>(value: &T, path: &str) {
    let text = serde_json::to_string_pretty(value).expect("JSON へのシリアライズに失敗");
    std::fs::write(path, text + "\n").unwrap_or_else(|e| panic!("{path} の書き込みに失敗: {e}"));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn quick_opts(dir: &str) -> ReproduceOptions {
        ReproduceOptions {
            output_dir: dir.to_string(),
            seed: 42,
            quick: true,
        }
    }

    #[test]
    fn reproduce_quick_writes_outputs_and_claims_pass() {
        let dir = std::env::temp_dir().join(format!("gv_reproduce_test_{}", std::process::id()));
        let dir_str = dir.to_string_lossy().to_string();
        run_reproduce(&quick_opts(&dir_str));

        // run ディレクトリの場所と名前は runvault が決めるので，実験ディレクトリの
        // 下を走査して見つける (名前を当てにしない)．
        let experiment_dir = dir.join(EXPERIMENT);
        let mut found = false;
        for entry in std::fs::read_dir(&experiment_dir).expect("実験ディレクトリが無い")
        {
            let p = entry.unwrap().path();
            if !p.is_dir() || !p.join("run.json").exists() {
                continue;
            }
            // 条件ごとの観測は events.jsonl，判定表は artifacts に入る．
            assert!(p.join("metrics.csv").exists());
            let events = std::fs::read_to_string(p.join("events.jsonl")).unwrap();
            assert!(events.contains(ABLATION_CONDITION_EVENT));
            assert!(events.contains(THRESHOLD_POINT_EVENT));

            let json = std::fs::read_to_string(p.join("artifacts").join("reproduce_summary.json"))
                .unwrap();
            // quick モードでも両 claim が PASS することを確認する．
            assert!(json.contains("\"verdict\": \"PASS\""));
            assert!(!json.contains("\"verdict\": \"OFF\""));
            found = true;
        }
        assert!(found, "run ディレクトリが生成されていない");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn mean_reach_with_removal_weak_collapses() {
        let cfg = Config {
            clusters: 6,
            cluster_size: 12,
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
        };
        let (base, weak, frac) = mean_reach_with_removal(&cfg, RemovePolicy::Weak, 5, 42);
        assert!(base >= 0.9, "baseline reach should be high: {base}");
        assert!(
            weak < base,
            "weak removal should reduce reach: {weak} < {base}"
        );
        assert!(
            (frac - 1.0).abs() < 1e-9,
            "all bridges should be weak: {frac}"
        );
    }
}
