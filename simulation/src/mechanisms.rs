//! socsim フレームワーク上の情報拡散メカニズム．
//!
//! 弱紐帯ブリッジ網の上で情報拡散を走らせる．`Interaction` フェーズで発火し，
//! **同期ラウンド (synchronous rounds)** を行う:
//!
//! 1. ラウンド開始時の informed 集合をスナップショット (`prev`)．
//! 2. SI モデル: informed な各ノードが各 uninformed 隣接を確率 `beta` で informed
//!    化する (`ctx.rng` を使用)．
//!    閾値モデル: uninformed ノードは informed 隣接割合が `theta` 以上のとき active
//!    化する (Granovetter 1978 閾値モデルの定式化を借用)．
//! 3. 当該ラウンドの感染は `prev` の informed 集合からのみ生じる (mid-round に
//!    informed 化したノードは次ラウンドまで感染源にならない)．
//! 4. 新規 informed 数を `scratch` へ記録し，到達履歴 `n_informed_history` を更新．
//! 5. **収束**: 新規 informed 数が 0 (カスケード飽和) または全到達のとき
//!    [`StepContext::request_stop`] でエンジンに停止を要求する．
//!
//! 同期更新なので活性化順は結果に影響しないが，イベント駆動拡張用に
//! `RandomActivationScheduler` を使う (設計書 §4.3)．近傍走査は socsim-net の
//! ゼロアロケーション `neighbors_iter` をホットループで用いる．

use socsim_core::{AgentId, Mechanism, Phase, Result, StepContext};

use crate::config::DiffusionModel;
use crate::world::WeakTieWorld;

/// 情報を紐帯に沿って拡散し，飽和で停止する同期ラウンドのメカニズム．
pub struct DiffusionMechanism {
    /// 拡散モデル (SI / 閾値)．
    pub model: DiffusionModel,
    /// SI 感染確率 β．
    pub beta: f64,
    /// 閾値 θ．
    pub theta: f64,
}

impl DiffusionMechanism {
    pub fn new(model: DiffusionModel, beta: f64, theta: f64) -> Self {
        DiffusionMechanism { model, beta, theta }
    }
}

impl Mechanism<WeakTieWorld> for DiffusionMechanism {
    fn name(&self) -> &str {
        "diffusion"
    }

    fn phases(&self) -> &'static [Phase] {
        &[Phase::Interaction]
    }

    fn apply(&mut self, _phase: Phase, ctx: &mut StepContext<'_, WeakTieWorld>) -> Result<()> {
        // ラウンド開始時の informed 集合スナップショット (同期更新の正本)．
        let prev: std::collections::BTreeMap<AgentId, bool> = ctx.world.informed.clone();

        // 当該ラウンドで新たに informed 化するノード．
        let mut newly: Vec<AgentId> = Vec::new();
        let ids: Vec<AgentId> = ctx.agent_order.to_vec();

        match self.model {
            DiffusionModel::Si => {
                // SI: prev で informed な各ノードが各 uninformed 隣接を確率 beta で感染．
                // uninformed 視点で「いずれかの informed 隣接から感染するか」を判定する
                // (同期; 各 informed-uninformed 辺で独立判定し，1 つでも成功すれば感染)．
                for &v in &ids {
                    if *prev.get(&v).unwrap_or(&false) {
                        continue; // すでに informed．
                    }
                    let mut infected = false;
                    for u in ctx.world.net.neighbors_iter(v) {
                        if *prev.get(&u).unwrap_or(&false) && ctx.rng.gen_bool_p(self.beta) {
                            infected = true;
                            break;
                        }
                    }
                    if infected {
                        newly.push(v);
                    }
                }
            }
            DiffusionModel::Threshold => {
                // 閾値: uninformed ノードは informed 隣接割合 >= theta で active 化．
                for &v in &ids {
                    if *prev.get(&v).unwrap_or(&false) {
                        continue;
                    }
                    let mut deg = 0usize;
                    let mut informed_nb = 0usize;
                    for u in ctx.world.net.neighbors_iter(v) {
                        deg += 1;
                        if *prev.get(&u).unwrap_or(&false) {
                            informed_nb += 1;
                        }
                    }
                    if deg > 0 && (informed_nb as f64) / (deg as f64) >= self.theta {
                        newly.push(v);
                    }
                }
            }
        }

        // 一括書き戻し (同期更新)．
        for &v in &newly {
            ctx.world.informed.insert(v, true);
        }
        let n_informed = ctx.world.n_informed();
        ctx.world.n_informed_history.push(n_informed);

        let n_new = newly.len();
        ctx.scratch.insert("n_new", n_new);
        ctx.scratch.insert("n_informed", n_informed);

        // 収束: 新規 informed 0 (飽和) または全到達で停止．
        let saturated = n_new == 0 || n_informed >= ctx.world.n();
        ctx.scratch.insert("saturated", saturated);
        if saturated {
            ctx.request_stop();
        }

        Ok(())
    }
}

// socsim-net の SimRng (= ChaCha20) は `rand::Rng` を実装する．`gen_bool` は
// rand 0.8 の API だが panic 条件 (p<0 || p>1) を避けるためクランプ版を用意する．
trait GenBoolClamped {
    fn gen_bool_p(&mut self, p: f64) -> bool;
}

impl GenBoolClamped for socsim_core::SimRng {
    fn gen_bool_p(&mut self, p: f64) -> bool {
        use rand::Rng;
        let p = p.clamp(0.0, 1.0);
        self.gen::<f64>() < p
    }
}
