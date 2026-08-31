//! runvault への記録の共通部分．
//!
//! 論文メタデータ (research) はどのサブコマンドでも同じなので，ここ 1 箇所で
//! 組み立てる．試行 1 本の書き方と，run 全体の集約指標もここに集める．
//!
//! この模型の観測に時間軸はほぼ無い．拡散はラウンドを刻んで進むが，指標
//! ([`Metrics`]) が計算されるのは **飽和した後の 1 点だけ**であり，ラウンドごとの
//! 数はどこにも書き出されていなかった．一方で 1 回の `run` は独立な試行を
//! `--runs` 本まとめて回すので，試行ごとの指標を `metrics.csv` に並べると主キー
//! (`name`, `step`, `step_unit`, `scope`) が全行で同じになり，試行どうしが衝突する．
//! 試行は «1 回の実行の中で観測された対象» なので，予約語 `unit_id` を持つ
//! `terminal` イベントとして `events.jsonl` に書き，`metrics.csv` には run に
//! 1 つしか無い値 (試行平均など) だけを置く．

use runvault::{Replication, Run, Target, Work};
use serde::Serialize;

use crate::metrics::Metrics;

/// runvault 上の実験名．`runvault path --experiment` に渡す値でもある．
pub const EXPERIMENT: &str = "granovetter";
/// リポジトリの安定 id．git remote の名前とは独立に固定する．
pub const REPO_ID: &str = "granovetter1973";
/// 分野．網生成・シード選択・SI の感染判定で乱数を引くので `simulation`
/// (= `master_seed` が必須)．`reproduce` も内部で同じ拡散を回すので同じ分野で，
/// 乱数を使わない突き合わせだけの `analysis` ではない．
pub const DOMAIN: &str = "simulation";

/// 時間軸の単位．
///
/// 拡散は全エージェントを同期更新する離散ラウンドで進み，コード上も一貫して
/// 「カスケードラウンド」と呼んでいる (`cascade_rounds` / `max_iterations`)．
/// runvault の語彙では `round`．
pub const T_UNIT: &str = "round";

/// `reproduce` の Claim A の 1 条件 (除去方策) を表す実験固有のイベント種別．
pub const ABLATION_CONDITION_EVENT: &str = "x.granovetter1973.ablation_condition";
/// `reproduce` の Claim B の 1 点 (θ) を表す実験固有のイベント種別．
pub const THRESHOLD_POINT_EVENT: &str = "x.granovetter1973.threshold_point";

/// 設計書 (Obsidian)．
const OBSIDIAN_NOTE: &str = "研究/98_論文レポート/80-再現実験/実装完了/granovetter1973/設計書.md";

/// 対象論文の書誌．
fn work() -> Work {
    let mut work = Work::doi("10.1086/225469")
        .title("The Strength of Weak Ties")
        .year(1973)
        .source_version("published");
    // vault 側の同定にも使えるよう paper-id も残す (work_id は DOI 側)．
    work.paper_id = Some("P00001793".to_string());
    work
}

/// この再現実験が対象としている論文．
///
/// 対象 (`Target`) は claim だけにした．1973 年の論文は概念的で，表も図も
/// 定量的な報告値も持たないため，掴めるのは命題そのものである．
///
/// `reproduce` の Claim B (閾値カスケードのティッピング) には対象を立てていない．
/// あれは Granovetter (1978)『Threshold Models of Collective Behavior』の定式化で
/// あって，1973 年の論文には無い．`research.work` は 1 本しか持てないので，
/// 1973 の DOI の下に置くと «この論文に閾値模型が載っている» という記録になる．
pub fn replication() -> Replication {
    Replication::new(work())
        .target(Target::claim(
            "all-bridges-are-weak-ties",
            "No strong tie is a bridge: every (local) bridge is a weak tie",
        ))
        .target(Target::claim(
            "weak-ties-dominate-reach",
            "Removing the weak ties confines diffusion to the seed's own cluster",
        ))
        .obsidian_note(OBSIDIAN_NOTE)
}

// ---------------------------------------------------------------------------
// 試行 1 本の記録
// ---------------------------------------------------------------------------

/// `events.jsonl` に書く観測行．
///
/// 予約キーだけを持つ．指標はここには書かない — 試行の最終値は下の
/// [`TerminalEvent`] が正本なので，同じ数を 2 箇所に置くと食い違う余地ができる．
/// この行が持つのは「その試行をいつ見たか」という時間軸だけである．
///
/// 指標が計算されるのは飽和後の 1 点だけなので，観測時刻も終端の 1 点しかない．
/// ラウンドごとの到達数は元から書き出されていないので，ここで作り足しもしない．
#[derive(Serialize)]
struct ObservationEvent<'a> {
    unit_id: &'a str,
    t: u64,
    t_unit: &'static str,
}

/// `events.jsonl` に書く終端行 (試行 1 本 = 1 行)．
///
/// 先頭 6 フィールドは runvault の予約語 (`terminal` はこれを全部要求する)．
/// 残りは旧 `metrics.csv` の 1 行そのもので，`run` 列は `unit_id` に，
/// `cascade_rounds` 列は `t` になった．
///
/// `baseline_reach_fraction` は `ablation` のときだけ載る (除去前の到達割合)．
/// 旧実装はこれを試行平均だけ画面に出して，どこにも書き残していなかった．
/// 欠測は 0 で埋めず，フィールドごと落とす．
#[derive(Serialize)]
struct TerminalEvent<'a> {
    unit_id: &'a str,
    t: u64,
    t_unit: &'static str,
    outcome: &'static str,
    censored: bool,
    budget: u64,
    seed: u64,
    n_agents: usize,
    n_edges: usize,
    n_local_bridges: usize,
    frac_weak_bridges: f64,
    forbidden_triad_rate: f64,
    reach_fraction: f64,
    avg_path_length: f64,
    largest_component_fraction: f64,
    reach_strong_only: usize,
    reach_weak_only: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    baseline_reach_fraction: Option<f64>,
}

/// 拡散が止まった理由．
///
/// メカニズムは «そのラウンドで新規活性化が無い» か «全員が活性» で
/// `request_stop` し，どちらも起きなければクロックの上限
/// (`max_iterations`) まで回る．上限に達して，かつ未到達が残っていれば
/// 右側打ち切り (`censored`) である．
fn outcome_of(m: &Metrics, max_iterations: usize) -> (&'static str, bool) {
    if m.reach_fraction >= 1.0 {
        ("all_informed", false)
    } else if m.cascade_rounds >= max_iterations {
        ("budget_exhausted", true)
    } else {
        ("saturated", false)
    }
}

/// 試行 1 本を `observation` + `terminal` として書く．
///
/// `terminal` だけでも生存時間解析は組めるが，`runvault verify --deep` は
/// `terminal` の `unit_id` が `observation` にも現れ，かつ終端の `t` が
/// その単位の最大観測時刻であることを要求する．観測は飽和後の 1 点なので，
/// 同じ `t` の `observation` を 1 行だけ先に書く．
pub fn log_trial(
    run: &mut Run,
    unit_id: &str,
    m: &Metrics,
    max_iterations: usize,
    baseline_reach_fraction: Option<f64>,
) {
    let t = m.cascade_rounds as u64;
    run.log_event(
        "observation",
        &ObservationEvent {
            unit_id,
            t,
            t_unit: T_UNIT,
        },
    )
    .unwrap_or_else(|e| panic!("{unit_id} の t={t} の observation の記録に失敗: {e}"));

    let (outcome, censored) = outcome_of(m, max_iterations);
    let event = TerminalEvent {
        unit_id,
        t,
        t_unit: T_UNIT,
        outcome,
        censored,
        budget: max_iterations as u64,
        seed: m.seed,
        n_agents: m.n_agents,
        n_edges: m.n_edges,
        n_local_bridges: m.n_local_bridges,
        frac_weak_bridges: m.frac_weak_bridges,
        forbidden_triad_rate: m.forbidden_triad_rate,
        reach_fraction: m.reach_fraction,
        avg_path_length: m.avg_path_length,
        largest_component_fraction: m.largest_component_fraction,
        reach_strong_only: m.reach_strong_only,
        reach_weak_only: m.reach_weak_only,
        baseline_reach_fraction,
    };
    run.log_event("terminal", &event)
        .unwrap_or_else(|e| panic!("{unit_id} の terminal イベントの記録に失敗: {e}"));
}

// ---------------------------------------------------------------------------
// run 全体の集約
// ---------------------------------------------------------------------------

/// 試行群を 1 つの値にまとめた指標だけを `metrics.csv` に書く．
///
/// ここに置けるのは «run に 1 つしか無い» 値に限る．試行ごとの値は
/// `events.jsonl` の担当で，こちらに降ろすと主キーが衝突する．内容は旧実装が
/// 画面に出していた「試行平均」そのものである．
///
/// `n_units` は予約指標名で «観測主体の数»．ここでの主体は試行なので試行数が入る．
/// `n_agents` は試行によらず `clusters × cluster_size` で一定なので平均を取らない．
pub fn log_trial_summary(run: &mut Run, metrics: &[Metrics]) {
    assert!(!metrics.is_empty(), "試行が 1 本もありません");
    let n = metrics.len() as f64;
    let mean = |f: &dyn Fn(&Metrics) -> f64| metrics.iter().map(f).sum::<f64>() / n;

    run.log_metrics(
        "run",
        &[
            ("n_units", n),
            ("n_agents", metrics[0].n_agents as f64),
            ("mean_n_edges", mean(&|m| m.n_edges as f64)),
            ("mean_n_local_bridges", mean(&|m| m.n_local_bridges as f64)),
            ("mean_frac_weak_bridges", mean(&|m| m.frac_weak_bridges)),
            (
                "mean_forbidden_triad_rate",
                mean(&|m| m.forbidden_triad_rate),
            ),
            ("mean_reach_fraction", mean(&|m| m.reach_fraction)),
            ("mean_avg_path_length", mean(&|m| m.avg_path_length)),
            (
                "mean_largest_component_fraction",
                mean(&|m| m.largest_component_fraction),
            ),
            (
                "mean_reach_strong_only",
                mean(&|m| m.reach_strong_only as f64),
            ),
            ("mean_reach_weak_only", mean(&|m| m.reach_weak_only as f64)),
            ("mean_cascade_rounds", mean(&|m| m.cascade_rounds as f64)),
        ],
    )
    .expect("run スコープの指標の記録に失敗");
}

/// アブレーションの比較そのもの (除去前 / 除去後 / 差分) を `metrics.csv` に書く．
///
/// 除去後の到達割合は [`log_trial_summary`] の `mean_reach_fraction` と同じ値だが，
/// あちらは «試行群の平均» の一覧，こちらは «この run が主張する比較» であり，
/// 別々に読めた方がよいので両方置く．旧実装はこの 3 つを画面に出すだけだった．
pub fn log_ablation_comparison(run: &mut Run, baseline_reach: f64, ablated_reach: f64) {
    run.log_metrics(
        "run",
        &[
            ("mean_baseline_reach_fraction", baseline_reach),
            ("mean_ablated_reach_fraction", ablated_reach),
            ("mean_delta_reach_fraction", ablated_reach - baseline_reach),
        ],
    )
    .expect("アブレーション比較の記録に失敗");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn metrics_with(reach_fraction: f64, cascade_rounds: usize) -> Metrics {
        Metrics {
            run: 0,
            seed: 42,
            n_agents: 100,
            n_edges: 200,
            n_local_bridges: 3,
            frac_weak_bridges: 1.0,
            forbidden_triad_rate: 0.3,
            reach_fraction,
            avg_path_length: 3.0,
            largest_component_fraction: 1.0,
            reach_strong_only: 9,
            reach_weak_only: 0,
            cascade_rounds,
        }
    }

    #[test]
    fn full_reach_is_not_censored() {
        let (outcome, censored) = outcome_of(&metrics_with(1.0, 8), 200);
        assert_eq!(outcome, "all_informed");
        assert!(!censored);
    }

    #[test]
    fn saturation_below_the_budget_is_not_censored() {
        let (outcome, censored) = outcome_of(&metrics_with(0.2, 3), 200);
        assert_eq!(outcome, "saturated");
        assert!(!censored);
    }

    /// 打ち切りの行は `t == budget` でなければならない (runvault が書き込み時に
    /// 検査する)．上限に達したときだけ `censored` を名乗ることを固定する．
    #[test]
    fn hitting_the_budget_is_censored_at_the_budget() {
        let m = metrics_with(0.5, 200);
        let (outcome, censored) = outcome_of(&m, 200);
        assert_eq!(outcome, "budget_exhausted");
        assert!(censored);
        assert_eq!(m.cascade_rounds, 200);
    }

    /// 書誌の同定子を固定する．`work_id` は DOI 側で，vault の `paper-id` は
    /// 併記に留める (両方を id にすると同じ論文が 2 つの run に割れる)．
    #[test]
    fn the_work_is_identified_by_its_doi() {
        let work = work();
        assert_eq!(work.work_id, "doi:10.1086/225469");
        assert_eq!(work.doi.as_deref(), Some("10.1086/225469"));
        assert_eq!(work.paper_id.as_deref(), Some("P00001793"));
        assert_eq!(work.year, Some(1973));
    }

    /// 対象は claim だけ (1973 年の論文は表も図も持たない)．
    #[test]
    fn the_replication_is_built_from_claim_targets() {
        let expected = Replication::new(work())
            .target(Target::claim(
                "all-bridges-are-weak-ties",
                "No strong tie is a bridge: every (local) bridge is a weak tie",
            ))
            .target(Target::claim(
                "weak-ties-dominate-reach",
                "Removing the weak ties confines diffusion to the seed's own cluster",
            ))
            .obsidian_note(OBSIDIAN_NOTE);
        assert_eq!(replication(), expected);
    }
}
