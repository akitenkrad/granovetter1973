//! socsim フレームワーク上の弱紐帯ブリッジ網の世界状態．
//!
//! エージェントは固定サイトであり「移動」しないため空間格子は不要 (`socsim-grid`
//! 不使用)．網トポロジは socsim-net の **重み付き無向ネットワーク**
//! [`WeightedNetwork<TieStrength>`] で保持し，**辺の重み (payload) が紐帯強度そのもの**
//! である．これにより設計書 §4.3 が UNCONFIRMED として挙げていた
//! `BTreeMap<(AgentId,AgentId),TieStrength>` の side-table を完全に廃した
//! (socsim issue #18 の weighted-edge API による)．
//!
//! 各エージェントの拡散状態 (`informed` 真偽値) と所属クラスタ (`cluster_of`)，
//! クロックを保持する．`#[derive(Clone)]` でスナップショット (アブレーション用の
//! 網複製・save/resume) に対応する．

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use socsim_core::{AgentId, BinaryState, Neighbors, SimClock, WorldState};
use socsim_net::WeightedNetwork;

/// 紐帯強度 (論文の 2 値: 強い / 弱い．欠如は辺の不在で表現)．
///
/// socsim-net の辺重み (payload) として保持する．`Serialize`/`Deserialize` を
/// 実装するので `to_weighted_json` / `from_weighted_json` でスナップショット可能．
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TieStrength {
    /// 強い紐帯 (クラスタ内の密な凝集を生む)．
    Strong,
    /// 弱い紐帯 (クラスタ間を橋渡しする)．
    Weak,
}

impl TieStrength {
    pub fn label(&self) -> &'static str {
        match self {
            TieStrength::Strong => "strong",
            TieStrength::Weak => "weak",
        }
    }
}

/// 弱紐帯ブリッジ網の世界状態．
#[derive(Clone)]
pub struct WeakTieWorld {
    /// シミュレーションクロック．
    pub clock: SimClock,
    /// 網トポロジ (紐帯強度を辺重みとして保持; side-table なし)．
    pub net: WeightedNetwork<TieStrength>,
    /// 各エージェントの所属クラスタ番号．
    pub cluster_of: BTreeMap<AgentId, usize>,
    /// 拡散状態 (SI の I / 閾値の active)．`true` = informed．
    pub informed: BTreeMap<AgentId, bool>,
    /// 拡散開始シード．
    pub seeds: Vec<AgentId>,
    /// 各ラウンドの到達数 (ドライバが CSV へ)．
    pub n_informed_history: Vec<usize>,
}

impl WeakTieWorld {
    /// 構成済みのフィールドから世界状態を組み立てる (網生成は [`crate::network`])．
    pub fn new(
        net: WeightedNetwork<TieStrength>,
        cluster_of: BTreeMap<AgentId, usize>,
        seeds: Vec<AgentId>,
        max_iterations: u64,
    ) -> Self {
        let mut informed: BTreeMap<AgentId, bool> =
            cluster_of.keys().map(|&id| (id, false)).collect();
        for &s in &seeds {
            informed.insert(s, true);
        }
        let n0 = informed.values().filter(|&&v| v).count();
        WeakTieWorld {
            clock: SimClock::new(max_iterations),
            net,
            cluster_of,
            informed,
            seeds,
            n_informed_history: vec![n0],
        }
    }

    /// 総エージェント数．
    pub fn n(&self) -> usize {
        self.informed.len()
    }

    /// 現在の informed 数．
    pub fn n_informed(&self) -> usize {
        self.informed.values().filter(|&&v| v).count()
    }

    /// あるエージェントが informed か．
    pub fn is_informed(&self, id: AgentId) -> bool {
        self.informed.get(&id).copied().unwrap_or(false)
    }
}

impl WorldState for WeakTieWorld {
    fn agent_ids(&self) -> Vec<AgentId> {
        // BTreeMap のキーはソート済み．契約 (sorted) を明示する．
        self.informed.keys().copied().collect()
    }

    fn clock(&self) -> &SimClock {
        &self.clock
    }

    fn clock_mut(&mut self) -> &mut SimClock {
        &mut self.clock
    }
}

impl Neighbors for WeakTieWorld {
    /// 紐帯に沿った影響集合 = socsim-net の無向隣接そのもの．
    ///
    /// 旧 `DiffusionMechanism` が用いた `net.neighbors_iter(id)` と同一の
    /// 隣接集合・走査順を返す (`neighbors` と `neighbors_iter` は同じ
    /// `graph.neighbors(ni)` を辿るためバイト等価)．contagion メカニズムの
    /// RNG 抽出順 (inactive エージェント × active 隣接) を不変に保つ．
    fn neighbors_of(&self, id: AgentId) -> Vec<AgentId> {
        self.net.neighbors(id)
    }
}

impl BinaryState for WeakTieWorld {
    /// active = informed (拡散状態の真偽値)．
    fn is_active(&self, id: AgentId) -> bool {
        self.is_informed(id)
    }

    /// informed フラグを書き換える (同期更新の一括書き戻しで使用)．
    fn set_active(&mut self, id: AgentId, active: bool) {
        self.informed.insert(id, active);
    }
}
