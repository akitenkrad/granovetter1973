//! シミュレーション設定．

use serde::Serialize;

/// 拡散モデルの種別．
///
/// - `Si`: SI (Susceptible-Infected) 接触過程．informed なノードが各 uninformed
///   隣接を確率 `beta` で informed 化する．
/// - `Threshold`: Granovetter (1978) 閾値カスケード．uninformed ノードは informed
///   隣接割合が `theta` 以上で active 化する．
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffusionModel {
    /// SI 接触過程 (確率 `beta`)．
    Si,
    /// 閾値カスケード (閾値 `theta`)．
    Threshold,
}

impl DiffusionModel {
    pub fn label(&self) -> &'static str {
        match self {
            DiffusionModel::Si => "si",
            DiffusionModel::Threshold => "threshold",
        }
    }
}

/// 文字列から `DiffusionModel` をパースする．
pub fn parse_diffusion(s: &str) -> Result<DiffusionModel, String> {
    match s.trim().to_lowercase().as_str() {
        "si" => Ok(DiffusionModel::Si),
        "threshold" | "thr" => Ok(DiffusionModel::Threshold),
        _ => Err(format!(
            "不正な拡散モデル: \"{}\" (si / threshold のみ対応)",
            s
        )),
    }
}

/// アブレーション (辺除去) の対象．
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemovePolicy {
    /// 除去なし (ベースライン)．
    None,
    /// 弱い紐帯をすべて除去する．
    Weak,
    /// 強い紐帯をすべて除去する．
    Strong,
    /// 弱紐帯と同数の辺をランダムに除去する (対照群)．
    Random,
}

impl RemovePolicy {
    pub fn label(&self) -> &'static str {
        match self {
            RemovePolicy::None => "none",
            RemovePolicy::Weak => "weak",
            RemovePolicy::Strong => "strong",
            RemovePolicy::Random => "random",
        }
    }
}

/// 文字列から `RemovePolicy` をパースする．
pub fn parse_remove(s: &str) -> Result<RemovePolicy, String> {
    match s.trim().to_lowercase().as_str() {
        "none" => Ok(RemovePolicy::None),
        "weak" => Ok(RemovePolicy::Weak),
        "strong" => Ok(RemovePolicy::Strong),
        "random" => Ok(RemovePolicy::Random),
        _ => Err(format!(
            "不正な除去対象: \"{}\" (none / weak / strong / random のみ対応)",
            s
        )),
    }
}

/// 単一実行の設定．
#[derive(Debug, Clone)]
pub struct Config {
    /// クラスタ (コミュニティ) 数 K．
    pub clusters: usize,
    /// 各クラスタのエージェント数．
    pub cluster_size: usize,
    /// クラスタ内強紐帯の生起確率 (ER モデル; 1.0 で準クリーク)．
    pub p_strong: f64,
    /// クラスタ対ごとの弱紐帯橋渡しの生起確率．
    pub p_bridge: f64,
    /// クラスタ内に許す弱紐帯の生起確率 (連続性近似; 既定 0.0)．
    pub p_weak_intra: f64,
    /// 拡散モデル (SI / 閾値)．
    pub diffusion: DiffusionModel,
    /// SI 感染確率 β (diffusion=si のとき使用)．
    pub beta: f64,
    /// 閾値 θ (diffusion=threshold のとき使用)．
    pub theta: f64,
    /// アブレーション除去対象．
    pub remove: RemovePolicy,
    /// 拡散シード数 (既定 1)．
    pub n_seeds: usize,
    /// 最大反復回数 (カスケードラウンド上限)．
    pub max_iterations: usize,
    /// 乱数シード (None の場合はランダム)．
    pub seed: Option<u64>,
    /// 結果出力ディレクトリ．
    pub output_dir: String,
}

impl Default for Config {
    /// 設計書 §5 に近い標準設定 (10 クラスタ × 20 エージェント, SI, β=0.5)．
    fn default() -> Self {
        Config {
            clusters: 10,
            cluster_size: 20,
            p_strong: 0.6,
            p_bridge: 0.3,
            p_weak_intra: 0.0,
            diffusion: DiffusionModel::Si,
            beta: 0.5,
            theta: 0.2,
            remove: RemovePolicy::None,
            n_seeds: 1,
            max_iterations: 200,
            seed: Some(42),
            output_dir: "results".to_string(),
        }
    }
}

/// 総エージェント数 (= clusters * cluster_size)．
impl Config {
    pub fn n_agents(&self) -> usize {
        self.clusters * self.cluster_size
    }
}

/// `config.json` (run 用) のシリアライズ表現．
#[derive(Serialize)]
pub struct RunConfigJson {
    pub command: &'static str,
    pub clusters: usize,
    pub cluster_size: usize,
    pub n_agents: usize,
    pub p_strong: f64,
    pub p_bridge: f64,
    pub p_weak_intra: f64,
    pub diffusion: &'static str,
    pub beta: f64,
    pub theta: f64,
    pub remove: &'static str,
    pub n_seeds: usize,
    pub max_iterations: usize,
    pub seed: Option<u64>,
    pub output_dir: String,
}

impl Config {
    /// `config.json` 用の表現を組み立てる (`command` は呼び出し側が与える)．
    pub fn to_run_config_json(&self, command: &'static str) -> RunConfigJson {
        RunConfigJson {
            command,
            clusters: self.clusters,
            cluster_size: self.cluster_size,
            n_agents: self.n_agents(),
            p_strong: self.p_strong,
            p_bridge: self.p_bridge,
            p_weak_intra: self.p_weak_intra,
            diffusion: self.diffusion.label(),
            beta: self.beta,
            theta: self.theta,
            remove: self.remove.label(),
            n_seeds: self.n_seeds,
            max_iterations: self.max_iterations,
            seed: self.seed,
            output_dir: self.output_dir.clone(),
        }
    }
}
