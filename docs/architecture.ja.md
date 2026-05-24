[English](architecture.md) | **日本語**

# アーキテクチャ

## リポジトリ構成

Cargo workspace + uv workspace の 2 プロジェクト構成．

```
granovetter1973/
├── Cargo.toml                 # Cargo workspace ルート
├── pyproject.toml             # uv workspace ルート
├── simulation/                # Rust プロジェクト (granovetter-simulation, lib granovetter_ties)
│   ├── Cargo.toml
│   ├── src/
│   │   ├── main.rs            # CLI (run / ablation / sweep)
│   │   ├── lib.rs             # バイナリ + 統合テスト用のモジュール公開
│   │   ├── config.rs          # Config + config.json シリアライズ，DiffusionModel / RemovePolicy
│   │   ├── world.rs           # socsim WorldState 実装 (WeakTieWorld); TieStrength を辺重みに
│   │   ├── network.rs         # クラスタ弱紐帯ブリッジ網生成器
│   │   ├── mechanisms.rs      # socsim Mechanism 実装 (DiffusionMechanism, 同期ラウンド)
│   │   ├── metrics.rs         # ブリッジ / 禁制三者 / 到達 / 経路長 / 強度別到達
│   │   └── simulation.rs      # init_world + アブレーション + run ドライバ (SimulationBuilder 配線)
│   └── tests/
│       └── integration_test.rs
├── tools/                     # Python プロジェクト (granovetter-tools)
│   ├── pyproject.toml
│   └── src/granovetter_tools/
│       ├── cli.py                       # 統合 CLI (granovetter-tools)
│       ├── visualize.py                 # 網レイアウト (紐帯強度で着色) + メトリクス
│       ├── visualize_sweep.py           # 到達割合 vs p_bridge + 構造命題の確認
│       └── show_experiment_settings.py  # run / ablation / sweep 設定の表示
└── results/                   # シミュレーション出力 (gitignore 対象)
```

- `cargo run` は workspace ルートから `simulation` crate を起動する．
- `uv run` は uv workspace メンバ `tools` が公開する `granovetter-tools` コマンドを起動する．

## socsim フレームワーク上のモデル

エンジンは [rs-social-simulation-tools](https://github.com/akitenkrad/rs-social-simulation-tools) (socsim) 上に構築する (git 依存，commit は `Cargo.lock` で固定)．本論文は **空間格子を持たない純網モデル** なので，`socsim-core` (トレイト)・`socsim-engine` (Simulation / Builder)・`socsim-net` (網レイヤ) を使い，**`socsim-grid` は不使用**．

### side-table ではなく重み付き辺 (設計書からの逸脱)

設計書 (§4.3) は socsim 更新以前のものであり，旧 `SocialNetwork` が `UnGraph<AgentId, ()>` (辺ペイロードなし) だったため紐帯強度を `BTreeMap<(AgentId, AgentId), TieStrength>` の side-table で保持する案を採っていた．この回避策は現在は不要である．`main` の socsim-net は汎用の重み付きネットワークを提供するので，**辺の重みが紐帯強度そのもの** になる:

- `WeightedNetwork<TieStrength>` (= `Network<TieStrength, Undirected>`) — `add_edge_weighted(a, b, TieStrength)`，`edge_weight(a, b) -> Option<&TieStrength>`，`weighted_edges() -> (a, b, &TieStrength)`，`remove_edge(a, b)`．
- 解析ヘルパ (socsim issue #20): `local_bridges()` / `is_local_bridge(a, b)`，`average_path_length()`，`component_membership()` / `largest_component_size()`，`connected_components()`，`edge_count()`．
- ホットループの近傍走査: `neighbors_iter` (ゼロアロケーション)．

したがって `WeakTieWorld` は **紐帯 side-table を持たない**: `WeightedNetwork<TieStrength>`，`cluster_of`，拡散状態 `informed`，拡散シード `seeds`，到達履歴 `n_informed_history` を保持する．スナップショットおよびアブレーション (世界を複製し辺を除去して再実行する) のため `#[derive(Clone)]` する．

使用する socsim API: `WorldState` (`agent_ids` ソート済み / `clock` / `clock_mut`)，`Mechanism` + `Phase::Interaction`，`RandomActivationScheduler`，`StepContext::request_stop` / `Simulation::run_observed` / `StepContext::scratch`，`SimRng` / `derive_seed`．

## 弱紐帯ブリッジ網生成器 (`network.rs`)

論文の構造を，疎な弱紐帯橋渡しを持つクラスタコミュニティとして操作化する:

1. **強紐帯クラスタ**: `K` 個のクラスタ (各 `cluster_size` エージェント)．クラスタ内は強紐帯リング (連結を保証) + 確率 `p_strong` の ER 強紐帯 (密度つまみ)．
2. **三角形閉包**: 閉包パスにより，**クラスタ内のすべての強紐帯が三角形の一部になる** (共通隣接を持つ) よう強紐帯を追加する．これにより強紐帯はクリーク的になり，クラスタ内に局所ブリッジが生じない (設計書の「強紐帯はブリッジになり得ない」)．結果として **すべての局所ブリッジはクラスタ間の弱紐帯** となり，既定生成器の下で `frac_weak_bridges == 1.0` が安定して成立する．
3. **弱紐帯ブリッジ**: クラスタ対ごとに確率 `p_bridge` で，両クラスタの一様乱択メンバ間に弱紐帯を 1 本張る．
4. 任意で少数のクラスタ内弱紐帯 (`p_weak_intra`, 既定 0.0)．

## 拡散メカニズム (同期ラウンド)

`DiffusionMechanism` は `Phase::Interaction` で発火し，**同期更新** する: ラウンド開始時の informed 集合をスナップショットし，その集合から新規活性化を計算し，一括適用する (ラウンド途中で informed 化したノードは次ラウンドまで感染源にならない)．これによりラウンド数が経路長の代理量になる (論文の「社会的距離 = 最短経路長」)．

- **SI** (`--diffusion si`): uninformed ノードは informed 隣接のいずれかが確率 `beta` (`ctx.rng` から抽出) で感染させれば informed 化する．
- **閾値** (`--diffusion threshold`): uninformed ノードは informed 隣接割合が `theta` 以上で active 化する (Granovetter 1978 閾値カスケード)．

収束: カスケード飽和 (新規 informed 0) または全到達で `request_stop()`．`RandomActivationScheduler` が毎ラウンド活性化順をシャッフルするが，同期更新では順序は結果に影響しない (イベント駆動拡張用に確保)．

## RNG ストリーム

単一 root シードを独立なラベル付きストリームに分割する (socsim 規約): `derive_seed(root, &[0])` = world 初期化 (網生成 + シード選択)，`derive_seed(root, &[1])` = engine / scheduler (= SI 感染判定)，`derive_seed(root, &[2])` = アブレーションのランダム除去．各 `sweep` / 多試行は `derive_seed(seed, &[...])` で独自 root を派生させるので，試行は再現可能かつ無相関である．

## メトリクス

| 指標 | 定義 | 論文での対応 |
|---|---|---|
| `frac_weak_bridges` | `local_bridges()` のうち辺重みが `Weak` の割合 | ファクト7「すべてのブリッジは弱紐帯」 |
| `n_local_bridges` | 局所ブリッジ数 (除去すると `d > 2`) | §4.3 局所ブリッジ命題 |
| `forbidden_triad_rate` | 強紐帯 2-パス (A–B, A–C 強) のうち B–C 辺が欠如する割合 | ファクト6 (Davis) |
| `reach_fraction` | 飽和時の informed 割合 | ファクト2「弱紐帯でより多くの人に到達」 |
| `avg_path_length` | 平均最短経路長 (`average_path_length()`) | ファクト3 連鎖長 |
| `reach_by_strength` | シードから強紐帯のみ / 弱紐帯のみで辿った構造的到達 | Rapoport–Horvath |
| `largest_component_fraction` | 最大連結成分サイズ / n | 網到達 / 分断 |
| `cascade_rounds` | 飽和までのラウンド数 | 経路長の代理量 |

`reach_by_strength` は socsim-net の `reachable_from(seed, |w| *w == strength)` (辺重みでフィルタした部分網上の BFS) を利用する．`reachable_from` はシード自身を含むため，従来の「到達人数はシードを除く」という定義に揃えるためシード集合を差し引く．構造指標はすべて socsim-net ヘルパを直接利用する．

## 再現性・決定論

固定シードでは全パイプライン (網生成・アブレーション・拡散) が決定論的である．統合テストは同一シードの 2 回実行で到達数と履歴が一致することを検証する．

## 今後の拡張 (Phase 3)

`reproduce` (論文 Fig./Table 一括再現: 経路長分布・連鎖長・友人順位別到達) の拡張点を残してある．本実装には含まない．

## 参考文献

- Granovetter, M. S. (1973). The Strength of Weak Ties. *American Journal of Sociology*, 78(6), 1360–1380. DOI: 10.1086/225469.
- Granovetter, M. S. (1978). Threshold Models of Collective Behavior. *AJS*, 83(6), 1420–1443. (閾値カスケードの定式化的基盤)
- Centola, D., & Macy, M. (2007). Complex Contagions and the Weakness of Long Ties. *AJS*, 113(3), 702–734.
- Davis, J. A. (1970). Clustering and Hierarchy in Interpersonal Relations. *ASR*, 35(5), 843–851. (禁じられた三者関係の経験的支持)

---
*This file was generated by Claude Code.*
