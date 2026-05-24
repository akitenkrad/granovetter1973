//! Granovetter (1973)「The Strength of Weak Ties」— 再現実験の CLI エントリポイント．
//!
//! `run`      : 1 つの (網構成, 拡散設定) で網生成 + 情報拡散を実行する．
//! `ablation` : 弱紐帯 / 強紐帯 / ランダム辺を除去して到達範囲の差を計測する．
//! `sweep`    : パラメータ (p_bridge / theta) を走査して到達割合等を集計する．
//!
//! Phase 3 の `reproduce` (論文 Fig./Table 一括再現) は拡張点として未実装．

use std::fs::{self, File};
use std::io::BufWriter;
use std::path::Path;

use chrono::Local;
use clap::{Parser, Subcommand};
use csv::Writer;

use granovetter_ties::config::{
    parse_diffusion, parse_remove, Config, DiffusionModel, RemovePolicy,
};
use granovetter_ties::metrics::{reach_fraction, Metrics};
use granovetter_ties::simulation::{
    apply_ablation, ensure_output_dir, init_world, run, run_diffusion, save_edges, save_metrics,
    save_nodes,
};

// ---------------------------------------------------------------------------
// CLI 定義
// ---------------------------------------------------------------------------

#[derive(Parser, Debug)]
#[command(
    name = "granovetter",
    about = "Granovetter (1973) The Strength of Weak Ties — 弱紐帯ブリッジ網生成 + 情報拡散の再現実験"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// 網生成 + 情報拡散を 1 構成で実行する．
    Run(RunArgs),
    /// 弱紐帯 / 強紐帯 / ランダム辺除去アブレーション (到達範囲の差を計測)．
    Ablation(AblationArgs),
    /// パラメータ (p_bridge / theta) を走査して集計する．
    Sweep(SweepArgs),
}

#[derive(Parser, Debug)]
struct RunArgs {
    /// クラスタ数 K．
    #[arg(long, default_value_t = 10)]
    clusters: usize,
    /// 各クラスタのエージェント数．
    #[arg(long, default_value_t = 20)]
    cluster_size: usize,
    /// クラスタ内強紐帯の生起確率．
    #[arg(long, default_value_t = 0.6)]
    p_strong: f64,
    /// クラスタ対ごとの弱紐帯橋渡し確率．
    #[arg(long, default_value_t = 0.3)]
    p_bridge: f64,
    /// クラスタ内弱紐帯の生起確率 (連続性近似; 既定 0.0)．
    #[arg(long, default_value_t = 0.0)]
    p_weak_intra: f64,
    /// 拡散モデル: si / threshold．
    #[arg(long, default_value = "si")]
    diffusion: String,
    /// SI 感染確率 β．
    #[arg(long, default_value_t = 0.5)]
    beta: f64,
    /// 閾値 θ (threshold モデル時)．
    #[arg(long, default_value_t = 0.2)]
    theta: f64,
    /// 拡散シード数．
    #[arg(long, default_value_t = 1)]
    n_seeds: usize,
    /// 独立試行数 (メトリクスを各試行 1 行で記録)．
    #[arg(long, default_value_t = 1)]
    runs: usize,
    /// 最大反復回数 (カスケードラウンド上限)．
    #[arg(long, default_value_t = 200)]
    max_iterations: usize,
    /// 乱数シード基点 (省略時はランダム)．
    #[arg(long)]
    seed: Option<u64>,
    /// 結果出力ディレクトリ．
    #[arg(long, default_value = "results")]
    output_dir: String,
}

#[derive(Parser, Debug)]
struct AblationArgs {
    /// 除去対象: none / weak / strong / random．
    #[arg(long, default_value = "weak")]
    remove: String,
    /// クラスタ数 K．
    #[arg(long, default_value_t = 10)]
    clusters: usize,
    /// 各クラスタのエージェント数．
    #[arg(long, default_value_t = 20)]
    cluster_size: usize,
    /// クラスタ内強紐帯の生起確率．
    #[arg(long, default_value_t = 0.6)]
    p_strong: f64,
    /// クラスタ対ごとの弱紐帯橋渡し確率．
    #[arg(long, default_value_t = 0.3)]
    p_bridge: f64,
    /// クラスタ内弱紐帯の生起確率．
    #[arg(long, default_value_t = 0.0)]
    p_weak_intra: f64,
    /// 拡散モデル: si / threshold．
    #[arg(long, default_value = "si")]
    diffusion: String,
    /// SI 感染確率 β．
    #[arg(long, default_value_t = 0.5)]
    beta: f64,
    /// 閾値 θ．
    #[arg(long, default_value_t = 0.2)]
    theta: f64,
    /// 拡散シード数．
    #[arg(long, default_value_t = 1)]
    n_seeds: usize,
    /// 独立試行数．
    #[arg(long, default_value_t = 10)]
    runs: usize,
    /// 最大反復回数．
    #[arg(long, default_value_t = 200)]
    max_iterations: usize,
    /// 乱数シード基点．
    #[arg(long, default_value_t = 42)]
    seed: u64,
    /// 結果出力ディレクトリ．
    #[arg(long, default_value = "results")]
    output_dir: String,
}

#[derive(Parser, Debug)]
struct SweepArgs {
    /// p_bridge 走査の最小値．
    #[arg(long, default_value_t = 0.0)]
    p_bridge_min: f64,
    /// p_bridge 走査の最大値 (含む)．
    #[arg(long, default_value_t = 0.5)]
    p_bridge_max: f64,
    /// p_bridge 走査の刻み幅．
    #[arg(long, default_value_t = 0.05)]
    p_bridge_step: f64,
    /// カンマ区切りの θ 候補 (threshold モデル時に使用)．
    #[arg(long, default_value = "0.1,0.2,0.3")]
    theta_values: String,
    /// クラスタ数 K．
    #[arg(long, default_value_t = 10)]
    clusters: usize,
    /// 各クラスタのエージェント数．
    #[arg(long, default_value_t = 20)]
    cluster_size: usize,
    /// クラスタ内強紐帯の生起確率．
    #[arg(long, default_value_t = 0.6)]
    p_strong: f64,
    /// 拡散モデル: si / threshold．
    #[arg(long, default_value = "si")]
    diffusion: String,
    /// SI 感染確率 β．
    #[arg(long, default_value_t = 0.5)]
    beta: f64,
    /// 拡散シード数．
    #[arg(long, default_value_t = 1)]
    n_seeds: usize,
    /// 各条件あたりの独立試行数．
    #[arg(long, default_value_t = 10)]
    runs: usize,
    /// 最大反復回数．
    #[arg(long, default_value_t = 200)]
    max_iterations: usize,
    /// 乱数シード基点．
    #[arg(long, default_value_t = 42)]
    seed: u64,
    /// 結果出力ディレクトリ．
    #[arg(long, default_value = "results")]
    output_dir: String,
}

// ---------------------------------------------------------------------------
// 補助
// ---------------------------------------------------------------------------

/// 小数点以下の桁数を文字列表現から推定する．
fn step_decimals(v: f64) -> usize {
    let s = format!("{}", v);
    match s.find('.') {
        Some(pos) => s.len() - pos - 1,
        None => 0,
    }
}

/// `min..=max` を `step` 刻みの等差数列に展開する (浮動小数点誤差を丸める)．
fn float_range(min: f64, max: f64, step: f64) -> Vec<f64> {
    assert!(step > 0.0, "step は正でなければなりません");
    let n_steps = ((max - min) / step + 0.5e-9).floor() as usize;
    let decimals = step_decimals(step);
    let factor = 10_f64.powi(decimals as i32);
    (0..=n_steps)
        .map(|i| ((min + step * i as f64) * factor).round() / factor)
        .collect()
}

/// latest シンボリックリンクを (再) 作成する．
fn refresh_latest(output_dir: &str, target: &str) {
    let symlink_path = Path::new(output_dir).join("latest");
    if symlink_path.is_symlink() {
        let _ = fs::remove_file(&symlink_path);
    }
    #[cfg(unix)]
    {
        let _ = std::os::unix::fs::symlink(target, &symlink_path);
    }
}

/// `(seed, run)` から各試行に独立なシードを派生させる (explicit identity)．
fn run_seed(root: u64, run_idx: usize) -> u64 {
    socsim_core::derive_seed(root, &[run_idx as u64])
}

// ---------------------------------------------------------------------------
// run
// ---------------------------------------------------------------------------

fn cmd_run(args: RunArgs) {
    let diffusion = parse_diffusion(&args.diffusion).unwrap_or_else(|e| panic!("{}", e));
    let timestamp = Local::now().format("%Y%m%d_%H%M%S").to_string();
    let output_dir = format!("{}/{}", args.output_dir, timestamp);

    let cfg = Config {
        clusters: args.clusters,
        cluster_size: args.cluster_size,
        p_strong: args.p_strong,
        p_bridge: args.p_bridge,
        p_weak_intra: args.p_weak_intra,
        diffusion,
        beta: args.beta,
        theta: args.theta,
        remove: RemovePolicy::None,
        n_seeds: args.n_seeds,
        max_iterations: args.max_iterations,
        seed: args.seed,
        output_dir: output_dir.clone(),
    };

    ensure_output_dir(&cfg.output_dir);
    let root = cfg.seed.unwrap_or_else(rand::random);

    println!("=== Granovetter 弱紐帯ブリッジ網 + 情報拡散 再現実験 ===");
    println!(
        "クラスタ: {} × {} (= {} エージェント) | p_strong: {} | p_bridge: {}",
        cfg.clusters,
        cfg.cluster_size,
        cfg.n_agents(),
        cfg.p_strong,
        cfg.p_bridge,
    );
    println!(
        "拡散: {} | β: {} | θ: {} | シード数: {} | 試行: {}",
        cfg.diffusion.label(),
        cfg.beta,
        cfg.theta,
        cfg.n_seeds,
        args.runs,
    );
    println!("乱数シード基点: {}", root);
    println!("出力先: {}", cfg.output_dir);
    println!("-------------------------------------------------------");

    let mut metrics: Vec<Metrics> = Vec::with_capacity(args.runs);
    let mut first_world = None;
    for run_idx in 0..args.runs {
        let seed = run_seed(root, run_idx);
        let result = run(&cfg, seed);
        metrics.push(Metrics::compute(
            &result.world,
            run_idx,
            seed,
            result.cascade_rounds,
        ));
        if first_world.is_none() {
            first_world = Some(result.world);
        }
    }

    let world = first_world.expect("少なくとも 1 試行は実行されるはず");
    save_metrics(&metrics, &cfg.output_dir);
    save_edges(&world, &cfg.output_dir);
    save_nodes(&world, &cfg.output_dir);

    // config.json
    {
        let path = format!("{}/config.json", cfg.output_dir);
        let file = File::create(&path).expect("config.json の作成に失敗");
        serde_json::to_writer_pretty(BufWriter::new(file), &cfg.to_run_config_json("run"))
            .expect("config.json の書き込みに失敗");
    }

    refresh_latest(&args.output_dir, &timestamp);

    // 試行平均を表示．
    let avg = |f: &dyn Fn(&Metrics) -> f64| -> f64 {
        metrics.iter().map(f).sum::<f64>() / metrics.len() as f64
    };
    println!("-------------------------------------------------------");
    println!("試行平均 ({} 試行):", metrics.len());
    println!(
        "  局所ブリッジ数: {:.1} | 弱紐帯ブリッジ率: {:.4}",
        avg(&|m| m.n_local_bridges as f64),
        avg(&|m| m.frac_weak_bridges),
    );
    println!("  禁制三者率: {:.4}", avg(&|m| m.forbidden_triad_rate));
    println!(
        "  到達割合: {:.4} | 平均経路長: {:.3} | カスケードラウンド: {:.1}",
        avg(&|m| m.reach_fraction),
        avg(&|m| m.avg_path_length),
        avg(&|m| m.cascade_rounds as f64),
    );
    println!(
        "  強紐帯のみ到達: {:.1} | 弱紐帯のみ到達: {:.1}",
        avg(&|m| m.reach_strong_only as f64),
        avg(&|m| m.reach_weak_only as f64),
    );
    println!("メトリクス → {}/metrics.csv", cfg.output_dir);
    println!("辺リスト   → {}/edges.csv", cfg.output_dir);
    println!("ノード     → {}/nodes.csv", cfg.output_dir);
    println!("設定       → {}/config.json", cfg.output_dir);
}

// ---------------------------------------------------------------------------
// ablation
// ---------------------------------------------------------------------------

fn cmd_ablation(args: AblationArgs) {
    let diffusion = parse_diffusion(&args.diffusion).unwrap_or_else(|e| panic!("{}", e));
    let remove = parse_remove(&args.remove).unwrap_or_else(|e| panic!("{}", e));
    let timestamp = Local::now().format("%Y%m%d_%H%M%S").to_string();
    let output_dir = format!("{}/{}", args.output_dir, timestamp);

    let mut cfg = Config {
        clusters: args.clusters,
        cluster_size: args.cluster_size,
        p_strong: args.p_strong,
        p_bridge: args.p_bridge,
        p_weak_intra: args.p_weak_intra,
        diffusion,
        beta: args.beta,
        theta: args.theta,
        remove,
        n_seeds: args.n_seeds,
        max_iterations: args.max_iterations,
        seed: Some(args.seed),
        output_dir: output_dir.clone(),
    };
    ensure_output_dir(&cfg.output_dir);

    println!("=== Granovetter アブレーション (辺除去) 実験 ===");
    println!(
        "除去対象: {} | クラスタ: {} × {} | p_bridge: {} | 拡散: {} | 試行: {}",
        remove.label(),
        cfg.clusters,
        cfg.cluster_size,
        cfg.p_bridge,
        cfg.diffusion.label(),
        args.runs,
    );
    println!("乱数シード基点: {}", args.seed);
    println!("出力先: {}", cfg.output_dir);
    println!("------------------------------------------------");

    let mut metrics: Vec<Metrics> = Vec::with_capacity(args.runs);
    let mut sum_baseline = 0.0_f64;
    let mut sum_ablated = 0.0_f64;
    let mut ablated_world = None;

    for run_idx in 0..args.runs {
        let seed = run_seed(args.seed, run_idx);

        // ベースライン (除去なし) の到達割合．
        let baseline_world = init_world(&cfg, seed);
        let baseline = run_diffusion(baseline_world, &cfg, seed);
        sum_baseline += reach_fraction(&baseline.world);

        // アブレーション後の到達割合．
        let mut world = init_world(&cfg, seed);
        apply_ablation(&mut world, &cfg, seed);
        let ablated = run_diffusion(world, &cfg, seed);
        sum_ablated += reach_fraction(&ablated.world);

        metrics.push(Metrics::compute(
            &ablated.world,
            run_idx,
            seed,
            ablated.cascade_rounds,
        ));
        if ablated_world.is_none() {
            ablated_world = Some(ablated.world);
        }
    }

    let world = ablated_world.expect("少なくとも 1 試行は実行されるはず");
    save_metrics(&metrics, &cfg.output_dir);
    save_edges(&world, &cfg.output_dir);
    save_nodes(&world, &cfg.output_dir);

    cfg.output_dir = output_dir.clone();
    {
        let path = format!("{}/config.json", cfg.output_dir);
        let file = File::create(&path).expect("config.json の作成に失敗");
        serde_json::to_writer_pretty(BufWriter::new(file), &cfg.to_run_config_json("ablation"))
            .expect("config.json の書き込みに失敗");
    }

    refresh_latest(&args.output_dir, &timestamp);

    let n = args.runs as f64;
    let baseline_reach = sum_baseline / n;
    let ablated_reach = sum_ablated / n;
    println!("------------------------------------------------");
    println!("試行平均 ({} 試行):", args.runs);
    println!("  ベースライン到達割合 (除去なし): {:.4}", baseline_reach);
    println!("  {} 除去後 到達割合: {:.4}", remove.label(), ablated_reach);
    println!(
        "  到達割合の差分 (Δreach): {:.4}",
        ablated_reach - baseline_reach
    );
    println!("メトリクス → {}/metrics.csv", cfg.output_dir);
    println!("設定       → {}/config.json", cfg.output_dir);
}

// ---------------------------------------------------------------------------
// sweep
// ---------------------------------------------------------------------------

/// `sweep_summary.csv` の 1 行．
#[derive(serde::Serialize)]
struct SweepRow {
    p_bridge: f64,
    theta: f64,
    run: usize,
    seed: u64,
    reach_fraction: f64,
    frac_weak_bridges: f64,
    forbidden_triad_rate: f64,
    avg_path_length: f64,
    largest_component_fraction: f64,
    cascade_rounds: usize,
}

/// `sweep_config.json` の構造体．
#[derive(serde::Serialize)]
struct SweepConfigJson {
    command: &'static str,
    p_bridge_min: f64,
    p_bridge_max: f64,
    p_bridge_step: f64,
    theta_values: Vec<f64>,
    clusters: usize,
    cluster_size: usize,
    p_strong: f64,
    diffusion: &'static str,
    beta: f64,
    n_seeds: usize,
    runs: usize,
    max_iterations: usize,
    seed: u64,
}

fn cmd_sweep(args: SweepArgs) {
    let diffusion = parse_diffusion(&args.diffusion).unwrap_or_else(|e| panic!("{}", e));

    let thetas: Vec<f64> = args
        .theta_values
        .split(',')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| {
            s.parse::<f64>()
                .unwrap_or_else(|_| panic!("不正な θ: {}", s))
        })
        .collect();
    // SI モデルでは θ は使わないので 1 値だけ走らせる．
    let thetas = if diffusion == DiffusionModel::Si {
        vec![args_default_theta()]
    } else {
        thetas
    };

    let p_bridges = float_range(args.p_bridge_min, args.p_bridge_max, args.p_bridge_step);

    let timestamp = Local::now().format("%Y%m%d_%H%M%S").to_string();
    let sweep_dir = format!("{}/{}_sweep", args.output_dir, timestamp);
    fs::create_dir_all(&sweep_dir).expect("sweep ディレクトリの作成に失敗");

    let n_total = p_bridges.len() * thetas.len() * args.runs;

    println!("=== Granovetter パラメータスイープ ===");
    println!(
        "p_bridge: {} 値 ({}..={}, step {}) | θ: {} 値 | 拡散: {} | 試行: {} | 合計: {} 実行",
        p_bridges.len(),
        args.p_bridge_min,
        args.p_bridge_max,
        args.p_bridge_step,
        thetas.len(),
        diffusion.label(),
        args.runs,
        n_total,
    );
    println!("出力先: {}", sweep_dir);
    println!("-------------------------------------");

    let mut rows: Vec<SweepRow> = Vec::with_capacity(n_total);
    let mut done = 0usize;

    for &theta in &thetas {
        for &p_bridge in &p_bridges {
            for run_idx in 0..args.runs {
                let seed = socsim_core::derive_seed(
                    args.seed,
                    &[theta.to_bits(), p_bridge.to_bits(), run_idx as u64],
                );
                let cfg = Config {
                    clusters: args.clusters,
                    cluster_size: args.cluster_size,
                    p_strong: args.p_strong,
                    p_bridge,
                    p_weak_intra: 0.0,
                    diffusion,
                    beta: args.beta,
                    theta,
                    remove: RemovePolicy::None,
                    n_seeds: args.n_seeds,
                    max_iterations: args.max_iterations,
                    seed: Some(seed),
                    output_dir: sweep_dir.clone(),
                };
                let result = run(&cfg, seed);
                let m = Metrics::compute(&result.world, run_idx, seed, result.cascade_rounds);
                rows.push(SweepRow {
                    p_bridge,
                    theta,
                    run: run_idx,
                    seed,
                    reach_fraction: m.reach_fraction,
                    frac_weak_bridges: m.frac_weak_bridges,
                    forbidden_triad_rate: m.forbidden_triad_rate,
                    avg_path_length: m.avg_path_length,
                    largest_component_fraction: m.largest_component_fraction,
                    cascade_rounds: m.cascade_rounds,
                });
                done += 1;
            }
            println!(
                "[{}/{}] θ={:.3} p_bridge={:.3} 完了 ({} 試行)",
                done, n_total, theta, p_bridge, args.runs,
            );
        }
    }

    {
        let path = format!("{}/sweep_summary.csv", sweep_dir);
        let file = File::create(&path).expect("sweep_summary.csv の作成に失敗");
        let mut wtr = Writer::from_writer(BufWriter::new(file));
        for row in &rows {
            wtr.serialize(row).expect("サマリ行の書き込みに失敗");
        }
        wtr.flush().expect("フラッシュに失敗");
    }

    {
        let config_json = SweepConfigJson {
            command: "sweep",
            p_bridge_min: args.p_bridge_min,
            p_bridge_max: args.p_bridge_max,
            p_bridge_step: args.p_bridge_step,
            theta_values: thetas.clone(),
            clusters: args.clusters,
            cluster_size: args.cluster_size,
            p_strong: args.p_strong,
            diffusion: diffusion.label(),
            beta: args.beta,
            n_seeds: args.n_seeds,
            runs: args.runs,
            max_iterations: args.max_iterations,
            seed: args.seed,
        };
        let path = format!("{}/sweep_config.json", sweep_dir);
        let file = File::create(&path).expect("sweep_config.json の作成に失敗");
        serde_json::to_writer_pretty(BufWriter::new(file), &config_json)
            .expect("sweep_config.json の書き込みに失敗");
    }

    refresh_latest(&args.output_dir, &format!("{}_sweep", timestamp));

    println!("-------------------------------------");
    println!("スイープ完了: {} 実行", n_total);
    println!("サマリ → {}/sweep_summary.csv", sweep_dir);
    println!("設定   → {}/sweep_config.json", sweep_dir);
}

/// SI モデルで θ を使わないときのプレースホルダ値．
fn args_default_theta() -> f64 {
    0.0
}

// ---------------------------------------------------------------------------
// main
// ---------------------------------------------------------------------------

fn main() {
    let cli = Cli::parse();
    match cli.command {
        Commands::Run(args) => cmd_run(args),
        Commands::Ablation(args) => cmd_ablation(args),
        Commands::Sweep(args) => cmd_sweep(args),
    }
}
