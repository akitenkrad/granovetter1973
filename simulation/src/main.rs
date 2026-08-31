//! Granovetter (1973)「The Strength of Weak Ties」— 再現実験の CLI エントリポイント．
//!
//! `run`       : 1 つの (網構成, 拡散設定) で網生成 + 情報拡散を実行する．
//! `ablation`  : 弱紐帯 / 強紐帯 / ランダム辺を除去して到達範囲の差を計測する．
//! `sweep`     : パラメータ (p_bridge / theta) を走査して到達割合等を集計する．
//! `reproduce` : 論文 (1973/1978) の主要な定量的主張を一括再現し，観測値 vs
//!               期待値の PASS/off 判定付きサマリを書き出す．
//!
//! 出力の置き場と同一性は runvault が持つ．タイムスタンプ付きディレクトリも
//! `latest` シンボリックリンクもこちらでは作らず，`Run::start` が決めた run
//! ディレクトリへ書く．試行ごとの指標は `events.jsonl` の `terminal` 行に，
//! 試行平均は `metrics.csv` に入る (理由は `record` モジュール)．

use clap::{Parser, Subcommand};
use runvault::{Lineage, Run, RunOptions};
use serde::Serialize;

use granovetter_ties::config::{
    parse_diffusion, parse_remove, Config, DiffusionModel, RemovePolicy,
};
use granovetter_ties::metrics::{reach_fraction, Metrics};
use granovetter_ties::record::{self, DOMAIN, EXPERIMENT, REPO_ID};
use granovetter_ties::reproduce::{run_reproduce, ReproduceOptions};
use granovetter_ties::simulation::{
    apply_ablation, ensure_output_dir, init_world, run, run_diffusion, save_edges, save_nodes,
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
    /// 論文 (1973/1978) の主要な定量的主張を一括再現する．
    Reproduce(ReproduceArgs),
}

#[derive(Parser, Debug)]
struct ReproduceArgs {
    /// 乱数シード基点．
    #[arg(long, default_value_t = 42)]
    seed: u64,
    /// 簡略化モード (クラスタ規模・試行数・θ 解像度を縮小; 動作確認用)．
    #[arg(long, default_value_t = false)]
    quick: bool,
    /// 結果出力ルート (この下に <実験名>/<run_slug>/ が作られる)．
    #[arg(long, default_value = "results")]
    output_dir: String,
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
    /// 結果出力ルート (この下に <実験名>/<run_slug>/ が作られる)．
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
    /// 結果出力ルート (この下に <実験名>/<run_slug>/ が作られる)．
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
    /// 結果出力ルート (この下に <実験名>/<run_slug>/ が作られる)．
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

/// `(seed, run)` から各試行に独立なシードを派生させる (explicit identity)．
fn run_seed(root: u64, run_idx: usize) -> u64 {
    socsim_core::derive_seed(root, &[run_idx as u64])
}

// ---------------------------------------------------------------------------
// run
// ---------------------------------------------------------------------------

fn cmd_run(args: RunArgs) {
    let diffusion = parse_diffusion(&args.diffusion).unwrap_or_else(|e| panic!("{}", e));

    // シードを実体化してから記録する．--seed 省略時にシミュレーション側で
    // rand::random に落とすと，実際に使われたシードがどこにも残らない．
    let root = args.seed.unwrap_or_else(rand::random::<u64>);

    // 出力先は Run::start が run ディレクトリを決めた後に確定する．
    let mut cfg = Config {
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
        seed: Some(root),
        output_dir: String::new(),
    };

    let parameters = cfg.to_parameters(args.runs, root);
    let mut rv = Run::start(
        RunOptions::new(EXPERIMENT, "run")
            .repo_id(REPO_ID)
            .domain(DOMAIN)
            .results_root(&args.output_dir)
            .parameters(&parameters)
            .expect("runvault: parameters の組み立てに失敗")
            .seed_pointers(["/seed"])
            .master_seed(root)
            .replication(record::replication()),
    )
    .expect("runvault: run の開始に失敗");

    // 網とノードの一覧は run が走っている間に書いたものなので artifacts/ の下へ．
    cfg.output_dir = rv.dir().join("artifacts").to_string_lossy().into_owned();
    ensure_output_dir(&cfg.output_dir);

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
    println!("出力先: {}", rv.dir().display());
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
    save_edges(&world, &cfg.output_dir);
    save_nodes(&world, &cfg.output_dir);

    for m in &metrics {
        record::log_trial(
            &mut rv,
            &format!("trial-{}", m.run),
            m,
            cfg.max_iterations,
            None,
        );
    }
    record::log_trial_summary(&mut rv, &metrics);

    // 試行平均を表示 (metrics.csv に入る値と同じもの)．
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

    let dir = rv.finish().expect("runvault: run の完了に失敗");
    println!("試行ごとの指標 → {}/events.jsonl", dir.display());
    println!("試行平均       → {}/metrics.csv", dir.display());
    println!("辺リスト       → {}/artifacts/edges.csv", dir.display());
    println!("ノード         → {}/artifacts/nodes.csv", dir.display());
    println!("設定           → {}/config.json", dir.display());
}

// ---------------------------------------------------------------------------
// ablation
// ---------------------------------------------------------------------------

fn cmd_ablation(args: AblationArgs) {
    let diffusion = parse_diffusion(&args.diffusion).unwrap_or_else(|e| panic!("{}", e));
    let remove = parse_remove(&args.remove).unwrap_or_else(|e| panic!("{}", e));

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
        output_dir: String::new(),
    };

    let parameters = cfg.to_parameters(args.runs, args.seed);
    let mut rv = Run::start(
        RunOptions::new(EXPERIMENT, "ablation")
            .repo_id(REPO_ID)
            .domain(DOMAIN)
            .results_root(&args.output_dir)
            .parameters(&parameters)
            .expect("runvault: parameters の組み立てに失敗")
            .seed_pointers(["/seed"])
            .master_seed(args.seed)
            .replication(record::replication()),
    )
    .expect("runvault: run の開始に失敗");

    cfg.output_dir = rv.dir().join("artifacts").to_string_lossy().into_owned();
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
    println!("出力先: {}", rv.dir().display());
    println!("------------------------------------------------");

    let mut metrics: Vec<Metrics> = Vec::with_capacity(args.runs);
    // 試行ごとのベースライン到達割合 (除去前)．旧実装は総和しか持たなかったが，
    // 各試行の terminal 行に載せるので試行ごとに取っておく．
    let mut baseline_reaches: Vec<f64> = Vec::with_capacity(args.runs);
    let mut sum_ablated = 0.0_f64;
    let mut ablated_world = None;

    for run_idx in 0..args.runs {
        let seed = run_seed(args.seed, run_idx);

        // ベースライン (除去なし) の到達割合．
        let baseline_world = init_world(&cfg, seed);
        let baseline = run_diffusion(baseline_world, &cfg, seed);
        baseline_reaches.push(reach_fraction(&baseline.world));

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
    save_edges(&world, &cfg.output_dir);
    save_nodes(&world, &cfg.output_dir);

    for (m, &baseline_reach) in metrics.iter().zip(&baseline_reaches) {
        record::log_trial(
            &mut rv,
            &format!("trial-{}", m.run),
            m,
            cfg.max_iterations,
            Some(baseline_reach),
        );
    }
    record::log_trial_summary(&mut rv, &metrics);

    let n = args.runs as f64;
    let baseline_reach = baseline_reaches.iter().sum::<f64>() / n;
    let ablated_reach = sum_ablated / n;
    record::log_ablation_comparison(&mut rv, baseline_reach, ablated_reach);

    println!("------------------------------------------------");
    println!("試行平均 ({} 試行):", args.runs);
    println!("  ベースライン到達割合 (除去なし): {:.4}", baseline_reach);
    println!("  {} 除去後 到達割合: {:.4}", remove.label(), ablated_reach);
    println!(
        "  到達割合の差分 (Δreach): {:.4}",
        ablated_reach - baseline_reach
    );

    let dir = rv.finish().expect("runvault: run の完了に失敗");
    println!("試行ごとの指標 → {}/events.jsonl", dir.display());
    println!("試行平均・比較 → {}/metrics.csv", dir.display());
    println!("設定           → {}/config.json", dir.display());
}

// ---------------------------------------------------------------------------
// sweep
// ---------------------------------------------------------------------------

/// sweep 親 run の `parameters` — 走査グリッドの定義そのもの．
#[derive(Serialize)]
struct SweepParameters {
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
    let n_total = p_bridges.len() * thetas.len() * args.runs;

    let sweep_parameters = SweepParameters {
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

    // 親 run: 走査グリッドの定義そのものを parameters に持ち，指標は持たない．
    // 親は 1 本のシミュレーションではないので master_seed を名乗らず，base seed は
    // /parameters.seed と seed_pointers 経由で execution_hash に残る．sweep_id は
    // runvault が親の run_slug で埋める．
    let parent = Run::start(
        RunOptions::new(EXPERIMENT, "sweep")
            .repo_id(REPO_ID)
            .domain(DOMAIN)
            .results_root(&args.output_dir)
            .parameters(&sweep_parameters)
            .expect("runvault: sweep の parameters の組み立てに失敗")
            .seed_pointers(["/seed"])
            .sweep_parent()
            .replication(record::replication()),
    )
    .expect("runvault: sweep 親 run の開始に失敗");

    let sweep_id = parent
        .sweep_id()
        .expect("runvault: sweep 親に sweep_id がありません")
        .to_string();
    let parent_run_uid = parent.run_uid().to_string();

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
    println!("出力先: {}", parent.dir().display());
    println!("-------------------------------------");

    let mut done = 0usize;

    for &theta in &thetas {
        for &p_bridge in &p_bridges {
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
                seed: Some(args.seed),
                output_dir: String::new(),
            };

            // 子は «その (θ, p_bridge) の試行群» そのもの．parameters は手で回した
            // `run` と同じ形なので，同じ条件なら config_hash が一致する．
            // 同じ条件の繰り返しは無いので replicate_index は 0．
            let mut child = Run::start(
                RunOptions::new(EXPERIMENT, "sweep-point")
                    .repo_id(REPO_ID)
                    .domain(DOMAIN)
                    .results_root(&args.output_dir)
                    .parameters(&cfg.to_parameters(args.runs, args.seed))
                    .expect("runvault: 子 run の parameters の組み立てに失敗")
                    .seed_pointers(["/seed"])
                    .master_seed(args.seed)
                    .replicate_index(0)
                    .lineage(Lineage {
                        sweep_id: Some(sweep_id.clone()),
                        parent_run_uid: Some(parent_run_uid.clone()),
                        ..Default::default()
                    })
                    .replication(record::replication()),
            )
            .expect("runvault: 子 run の開始に失敗");

            let mut metrics: Vec<Metrics> = Vec::with_capacity(args.runs);
            for run_idx in 0..args.runs {
                let seed = socsim_core::derive_seed(
                    args.seed,
                    &[theta.to_bits(), p_bridge.to_bits(), run_idx as u64],
                );
                let result = run(&cfg, seed);
                let m = Metrics::compute(&result.world, run_idx, seed, result.cascade_rounds);
                record::log_trial(
                    &mut child,
                    &format!("trial-{run_idx}"),
                    &m,
                    args.max_iterations,
                    None,
                );
                metrics.push(m);
                done += 1;
            }
            record::log_trial_summary(&mut child, &metrics);
            child.finish().expect("runvault: 子 run の完了に失敗");

            println!(
                "[{}/{}] θ={:.3} p_bridge={:.3} 完了 ({} 試行)",
                done, n_total, theta, p_bridge, args.runs,
            );
        }
    }

    let dir = parent
        .finish()
        .expect("runvault: sweep 親 run の完了に失敗");
    println!("-------------------------------------");
    println!("スイープ完了: {} 実行", n_total);
    println!("スイープ定義 → {}/config.json", dir.display());
    println!("各条件の試行は子 run (subcommand=sweep-point) の events.jsonl にあります");
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
        Commands::Reproduce(args) => cmd_reproduce(args),
    }
}

/// 論文主要主張の一括再現を実行する．
fn cmd_reproduce(args: ReproduceArgs) {
    run_reproduce(&ReproduceOptions {
        output_dir: args.output_dir,
        seed: args.seed,
        quick: args.quick,
    });
}
