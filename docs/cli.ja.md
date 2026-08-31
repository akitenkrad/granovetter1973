[English](cli.md) | **日本語**

# CLI

Rust バイナリ `granovetter` (`cargo run --release -- …` で実行) は 4 つのサブコマンドを公開する: `run` / `ablation` / `sweep` / `reproduce`．

## `run` — 網生成 + 拡散

弱紐帯ブリッジ網を 1 つ生成し，その上で情報拡散を実行する．

```bash
cargo run --release -- run \
    --clusters 10 --cluster-size 20 \
    --p-strong 0.6 --p-bridge 0.3 \
    --diffusion si --beta 0.5 --runs 1 --seed 42
```

| フラグ | 既定値 | 説明 |
|---|---|---|
| `--clusters` | 10 | クラスタ数 `K` |
| `--cluster-size` | 20 | クラスタあたりエージェント数 |
| `--p-strong` | 0.6 | クラスタ内強紐帯 ER 確率 (密度つまみ; リング + 三角形閉包で連結とクリーク性を保証) |
| `--p-bridge` | 0.3 | クラスタ対ごとの弱紐帯ブリッジ確率 |
| `--p-weak-intra` | 0.0 | クラスタ内弱紐帯確率 (連続性近似) |
| `--diffusion` | si | 拡散モデル: `si` / `threshold` |
| `--beta` | 0.5 | SI 感染確率 `β` |
| `--theta` | 0.2 | 閾値 `θ` (threshold モデル) |
| `--n-seeds` | 1 | 拡散シード数 (エージェント `0..n_seeds`，クラスタ 0) |
| `--runs` | 1 | 独立試行数 (各試行 1 メトリクス行) |
| `--max-iterations` | 200 | カスケードラウンド上限 |
| `--seed` | ランダム | 乱数シード基点 |
| `--output-dir` | results | 結果ルート (この下に `granovetter/{run_slug}/` を作る) |

出力の置き場と名前は [runvault](https://github.com/akitenkrad/rs-runvault) が決める (`{--output-dir}/granovetter/{run_slug}/`)．

- `config.json` — 封筒．実行条件は `parameters` の下．
- `events.jsonl` — 試行ごと 1 行の `terminal` 行 (+ 同じ時刻の `observation`)．旧 `metrics.csv` の各行がここに入り，`run` 列は `unit_id`，`cascade_rounds` 列は `t` (単位 `round`) になった．`seed, n_agents, n_edges, n_local_bridges, frac_weak_bridges, forbidden_triad_rate, reach_fraction, avg_path_length, largest_component_fraction, reach_strong_only, reach_weak_only` はそのまま載る．
- `metrics.csv` — run に 1 つしか無い値だけ: `n_units` (試行数)，`n_agents`，および各指標の試行平均 `mean_*`．
- `artifacts/edges.csv` — 網: `a, b, strength` (`strong` / `weak`)．
- `artifacts/nodes.csv` — ノードのクラスタ割当: `id, cluster, is_seed`．

試行ごとの値を `metrics.csv` に並べないのは，時間軸を持たない値が並ぶと主キー (`name`, `step`, `step_unit`, `scope`) が全行で同じになり衝突するためである．直近の run は `runvault path --experiment granovetter --latest --subcommand run --standalone` で解決する (`results/latest` は作られない)．

## `ablation` — 辺除去実験

世界を複製して選択した辺を除去し (`remove_edge`)，拡散を再実行して，除去なしベースラインとの到達範囲の差を報告する．論文の主要結果である．

```bash
cargo run --release -- ablation --remove weak \
    --clusters 10 --cluster-size 20 --p-strong 0.6 --p-bridge 0.5 \
    --diffusion si --beta 0.9 --runs 10 --seed 42
```

`--remove` は `none` / `weak` / `strong` / `random` を取る (random は弱紐帯と同数の辺を除去する対照群)．他のフラグは `run` と同様 (`--seed` 既定 42，`--runs` 既定 10)．出力は `run` と同じ構成で，run の `subcommand` が `ablation` になり，メトリクスは **除去後** の網で計算される．各試行の `terminal` 行には除去前の到達割合 `baseline_reach_fraction` が追加で載り，`metrics.csv` には比較そのもの (`mean_baseline_reach_fraction` / `mean_ablated_reach_fraction` / `mean_delta_reach_fraction`) が入る (旧実装はこの 3 つを画面に出すだけだった)．

典型的な結果 (`--remove weak`，8×12 クラスタ，`p_bridge=0.5`，`β=0.9`): ベースライン到達 `1.0` → 弱紐帯除去後 `≈0.125` (8 クラスタ中シードの 1 クラスタ)．`--remove strong` では到達がさらに下がる (`≈0.01`)．これは *すべての* 強紐帯を除去すると疎なブリッジ骨格だけが残り，ブリッジ非端点ノードは伝播経路を失う (強紐帯が担うクラスタ内充填が消える) ためである．解釈は [ユースケース](usecases.ja.md) を参照．

## `sweep` — パラメータスイープ

`p_bridge` (および threshold モデルでは `theta`) を走査し，条件ごとに到達範囲と構造指標を集計する．

```bash
cargo run --release -- sweep \
    --p-bridge-min 0.0 --p-bridge-max 0.5 --p-bridge-step 0.05 \
    --theta-values 0.1,0.2,0.3 \
    --diffusion si --beta 0.5 --runs 10 --seed 42
```

| フラグ | 既定値 | 説明 |
|---|---|---|
| `--p-bridge-min` / `--p-bridge-max` / `--p-bridge-step` | 0.0 / 0.5 / 0.05 | `p_bridge` 走査範囲 |
| `--theta-values` | 0.1,0.2,0.3 | カンマ区切り `θ` 候補 (threshold モデルのみ; `si` では無視) |
| `--clusters` / `--cluster-size` / `--p-strong` | 10 / 20 / 0.6 | 網パラメータ |
| `--diffusion` / `--beta` / `--n-seeds` | si / 0.5 / 1 | 拡散パラメータ |
| `--runs` | 10 | 条件あたり独立試行数 |
| `--max-iterations` | 200 | カスケードラウンド上限 |
| `--seed` | 42 | シード基点 (各試行は独立シードを派生) |
| `--output-dir` | results | 結果ルート (この下に `granovetter/{run_slug}/` を作る) |

各試行は `derive_seed(seed, &[theta.bits, p_bridge.bits, run])` で独立シードを派生させる．出力は sweep 親 run 1 本と，条件 `(p_bridge, θ)` ごとの子 run (`subcommand=sweep-point`) である．走査した値のそれぞれは模型の別々の実行なので子 run に割る．

- 親の `config.json` — 走査グリッドの定義 (`p_bridge_min` / `p_bridge_max` / `p_bridge_step` / `theta_values` ほか)．親自身は指標を持たない．
- 子の `config.json` — その条件そのもの．手で回した `run` と同じ形なので，同じ条件なら `config_hash` が一致する．
- 子の `events.jsonl` — 試行ごと 1 行 (`run` サブコマンドと同じ形)．旧 `sweep_summary.csv` の行に相当する．
- 子の `metrics.csv` — その条件の試行平均．

条件ごとの表はディスクに無い．Python 側は `runvault.read.sweep_events_table` で子 run から組み直す．

`si` では `theta` は単一のプレースホルダ値に固定される (未使用) ため，スイープは `p_bridge` についてのみ走る．

## `reproduce` — 論文の一括再現

論文のヘッドラインとなる定量的主張を 1 コマンドで再現し，観測値 vs 期待値の比較を PASS/off 判定付きで出力する．網生成 + 拡散を内部で決定論的に (seed 固定; subprocess なし) 走らせるため，秒境界の競合は起きない．

```bash
cargo run --release -- reproduce --seed 42 --output-dir results
cargo run --release -- reproduce --quick      # 動作確認用の縮小規模 (小クラスタ / 少試行 / 粗い θ グリッド)
```

| フラグ | 既定値 | 説明 |
|---|---|---|
| `--seed` | 42 | 乱数シード基点 (各試行は独立シードを派生) |
| `--quick` | off | 動作確認用モード (規模縮小; 論文値の検証には使わない) |
| `--output-dir` | results | 結果ルート (この下に `granovetter/{run_slug}/` を作る) |

再現する主張は 2 つ:

- **Claim A — 弱紐帯ブリッジ効果** (1973 ファクト7 + 中心命題)．ベースライン到達は `≈1.0` だが，弱紐帯 (= 局所ブリッジ *すべて*) を除去すると到達はシード所属クラスタ (`≈1/K`) へ崩壊する．一方で *同数* の辺をランダムに除去しても到達は保たれる (`≈1.0`，対照群)．`frac_weak_bridges = 1.0`．PASS の条件はベースライン `≥0.9`，弱除去 `≤1/K + 0.1`，ランダム除去 `≥0.9`，`frac_weak_bridges = 1.0`．
- **Claim B — 閾値カスケードのティッピング** (Granovetter 1978)．一様閾値 `θ` のわずかな上方シフトが，最終カスケードサイズを大域カスケード (`reach ≈1.0`) から局所カスケード (`reach ≈1/K`) へ跳ばせる．遷移は狭い `θ` 帯に集中する．PASS の条件は低 `θ` 側で reach `≥0.9`，高 `θ` 側で reach `≤0.2`，遷移帯幅 `Δθ ≤ 0.07`．

出力 (run ディレクトリ配下):

- `events.jsonl` — 除去方策ごと 1 行 (`x.granovetter1973.ablation_condition`: `unit_id, remove, baseline_reach, removed_reach, delta_reach, frac_weak_bridges`) と，走査した `θ` ごと 1 行 (`x.granovetter1973.threshold_point`: `unit_id, theta, reach, cascade_rounds`)．旧 `claim_a_ablation.csv` / `claim_b_threshold.csv` の中身である．どちらも時間軸を持たないので `metrics.csv` には置けない．
- `metrics.csv` — 両 claim のヘッドラインだけ: `baseline_reach`, `weak_removed_reach`, `strong_removed_reach`, `random_removed_reach`, `frac_weak_bridges`, `theta_low`, `reach_low`, `theta_high`, `reach_high`, `transition_width`．
- `artifacts/reproduce_summary.json` — 網パラメータ，各 claim のパラメータ，観測値 vs 期待値 + `PASS`/`OFF` 判定．許容幅と判定は指標でも論文の報告値でもないので artifacts に置く．

Python ラッパ `uv run granovetter-tools reproduce` はこのサブコマンドを呼び，さらに比較図 (`claim_a_weak_tie_bridges.png` / `claim_b_threshold_tipping.png`) を run の **外** (`{results_root}/granovetter/figures/{run_slug}/`) に描く．`manifest.csv` は `finish()` が確定させるので，走り終わった後に足したファイルを run の中に置くと記録と食い違う．[可視化](visualization.ja.md) を参照．

代表的なフル実行 (`--seed 42`): Claim A はベースライン `1.000` → 弱 `0.100` (`1/K`)，強 `0.006`，ランダム `1.000`，`frac_weak_bridges = 1.000` (PASS)．Claim B は `reach` が `θ=0.07` の `0.990` → `θ=0.10` の `0.151`，`Δθ = 0.030` (PASS)．

---
*This file was generated by Claude Code.*
