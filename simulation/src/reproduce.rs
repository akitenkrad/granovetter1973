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
//! 出力 (`<output_dir>/reproduce_<ts>/`):
//!
//! - `claim_a_ablation.csv` — none/weak/strong/random 各除去方策の試行平均到達割合．
//! - `claim_b_threshold.csv` — θ 走査ごとの試行平均 reach / cascade_rounds．
//! - `reproduce_summary.json` — 各 claim の params + 観測値 vs 期待値 + verdict．

use serde::Serialize;
use socsim_core::derive_seed;
use socsim_results::{ensure_dir, refresh_latest_symlink, timestamp, write_csv, write_json};

use crate::config::{Config, DiffusionModel, RemovePolicy};
use crate::metrics::{frac_weak_bridges, reach_fraction};
use crate::simulation::{apply_ablation, init_world, run_diffusion};

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

/// Claim A の 1 行 (除去方策ごと)．
#[derive(Serialize)]
struct AblationRow {
    remove: &'static str,
    baseline_reach: f64,
    removed_reach: f64,
    delta_reach: f64,
    frac_weak_bridges: f64,
}

/// Claim B の 1 行 (θ ごと)．
#[derive(Serialize)]
struct ThresholdRow {
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

/// `reproduce_summary.json` のトップレベル構造．
#[derive(Serialize)]
struct ReproduceSummary {
    timestamp: String,
    quick: bool,
    seed: u64,
    network: NetworkParams,
    claim_a: ClaimAResult,
    claim_b: ClaimBResult,
    claims: Vec<ClaimVerdict>,
    csv_files: Vec<String>,
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

/// 論文主要主張の一括再現を実行し，CSV + summary JSON を書き出す．
pub fn run_reproduce(opts: &ReproduceOptions) {
    let ts = timestamp();
    let base_dir = format!("{}/reproduce_{}", opts.output_dir, ts);
    ensure_dir(&base_dir).expect("reproduce 出力ディレクトリの作成に失敗");

    // quick はクラスタ数・試行数・θ 解像度を縮小する (動作確認用)．
    let clusters = 10usize;
    let cluster_size = if opts.quick { 12 } else { 20 };
    let runs_a = if opts.quick { 5 } else { 20 };
    let runs_b = if opts.quick { 5 } else { 10 };
    let p_strong = 0.6;

    println!("=== Granovetter (1973/1978) 論文主要主張の一括再現 ===");
    println!("出力先   : {}", base_dir);
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
    let p_bridge_a = 0.5;
    let beta_a = 0.9;
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
        max_iterations: 200,
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

    let ablation_rows = vec![
        AblationRow {
            remove: "none",
            baseline_reach: base_w,
            removed_reach: base_w,
            delta_reach: 0.0,
            frac_weak_bridges: frac_weak,
        },
        AblationRow {
            remove: "weak",
            baseline_reach: base_w,
            removed_reach: weak_w,
            delta_reach: weak_w - base_w,
            frac_weak_bridges: frac_weak,
        },
        AblationRow {
            remove: "strong",
            baseline_reach: base_w,
            removed_reach: strong_w,
            delta_reach: strong_w - base_w,
            frac_weak_bridges: frac_weak,
        },
        AblationRow {
            remove: "random",
            baseline_reach: base_w,
            removed_reach: random_w,
            delta_reach: random_w - base_w,
            frac_weak_bridges: frac_weak,
        },
    ];
    let claim_a_csv = format!("{}/claim_a_ablation.csv", base_dir);
    write_csv(&ablation_rows, &claim_a_csv).expect("claim_a_ablation.csv の書き込みに失敗");

    // 判定: ベースライン高 + 弱除去で大幅減 + ランダム除去は不変 + frac_weak=1．
    let a_baseline_high = base_w >= 0.9;
    let a_weak_collapse = weak_w <= expected_local + 0.1;
    let a_random_robust = random_w >= 0.9;
    let a_frac_weak = frac_weak >= 0.999;
    let claim_a_pass = a_baseline_high && a_weak_collapse && a_random_robust && a_frac_weak;

    // -------------------------------------------------------------------
    // Claim B — 閾値カスケードのティッピング (Granovetter 1978)．
    // -------------------------------------------------------------------
    let p_bridge_b = 0.5;
    let n_seeds_b = cluster_size * clusters / 10; // 各クラスタ平均 ~1 シード相当の密度．
                                                  // quick はクラスタ規模が小さく (cluster_size=12) 崩壊 θ が上にずれるため，
                                                  // 崩壊帯 (θ≈0.10→0.16) を捉える粗いグリッドにする．フル版はより細かい．
    let theta_values: Vec<f64> = if opts.quick {
        vec![0.04, 0.10, 0.12, 0.14, 0.16, 0.20]
    } else {
        vec![0.04, 0.06, 0.07, 0.08, 0.09, 0.10, 0.11, 0.12, 0.15, 0.20]
    };
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
        max_iterations: 200,
        seed: Some(opts.seed),
        output_dir: base_dir.clone(),
    };

    println!(
        "--- Claim B: 閾値カスケードのティッピング (p_bridge={p_bridge_b}, n_seeds={n_seeds_b}, {runs_b} 試行) ---"
    );
    let mut threshold_rows: Vec<ThresholdRow> = Vec::with_capacity(theta_values.len());
    let mut reach_values: Vec<f64> = Vec::with_capacity(theta_values.len());
    for &theta in &theta_values {
        let (reach, rounds) = mean_reach_threshold(&cfg_b, theta, runs_b, opts.seed);
        println!("  θ={theta:.2}  reach={reach:.4}  cascade_rounds={rounds:.1}");
        threshold_rows.push(ThresholdRow {
            theta,
            reach,
            cascade_rounds: rounds,
        });
        reach_values.push(reach);
    }
    let claim_b_csv = format!("{}/claim_b_threshold.csv", base_dir);
    write_csv(&threshold_rows, &claim_b_csv).expect("claim_b_threshold.csv の書き込みに失敗");

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
        timestamp: ts.clone(),
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
        csv_files: vec![
            "claim_a_ablation.csv".to_string(),
            "claim_b_threshold.csv".to_string(),
        ],
    };

    let summary_path = format!("{}/reproduce_summary.json", base_dir);
    write_json(&summary, &summary_path).expect("reproduce_summary.json の書き込みに失敗");

    let _ = refresh_latest_symlink(&opts.output_dir, &format!("reproduce_{}", ts));

    println!("-------------------------------------------------------");
    println!(
        "Claim A (弱紐帯ブリッジ効果)        : {}",
        verdict(claim_a_pass)
    );
    println!(
        "Claim B (閾値カスケードのティッピング): {}",
        verdict(claim_b_pass)
    );
    println!("CSV    → {}", claim_a_csv);
    println!("CSV    → {}", claim_b_csv);
    println!("サマリ → {}", summary_path);
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

        // reproduce_<ts>/reproduce_summary.json が生成されているはず．
        let mut found = false;
        for entry in std::fs::read_dir(&dir).expect("出力ディレクトリが無い") {
            let p = entry.unwrap().path();
            if p.is_dir()
                && p.file_name()
                    .and_then(|s| s.to_str())
                    .map(|s| s.starts_with("reproduce_"))
                    .unwrap_or(false)
            {
                assert!(p.join("reproduce_summary.json").exists());
                assert!(p.join("claim_a_ablation.csv").exists());
                assert!(p.join("claim_b_threshold.csv").exists());
                let json = std::fs::read_to_string(p.join("reproduce_summary.json")).unwrap();
                // quick モードでも両 claim が PASS することを確認する．
                assert!(json.contains("\"verdict\": \"PASS\""));
                assert!(!json.contains("\"verdict\": \"OFF\""));
                found = true;
            }
        }
        assert!(found, "reproduce_<ts> ディレクトリが生成されていない");

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
