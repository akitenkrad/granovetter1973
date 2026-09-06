[English](visualization.md) | **日本語**

# 可視化

Python パッケージ `granovetter-tools` (uv workspace メンバ) は runvault の run ディレクトリを読んで図を生成する．どの run を読むかは `runvault path` が解決するので (`results/` を走査しない)，`runvault` コマンドが PATH にある必要がある．workspace ルートで `uv sync` を一度実行してインストールする．

図は run ディレクトリの **外** (`{results_root}/granovetter/figures/{run_slug}/`) に書く．`manifest.csv` は `finish()` が確定させるので，走り終わった後に足したファイルを run の中に置くと記録と食い違う．

```bash
uv sync
uv run granovetter-tools visualize
uv run granovetter-tools visualize-sweep
uv run granovetter-tools show-experiment-settings
uv run granovetter-tools reproduce
```

CLI は argparse で 4 つのサブコマンドにディスパッチする．サブコマンド以降の引数は対応モジュールへそのまま渡される．

## `visualize` — 網レイアウト・メトリクス

`run` / `ablation` の run から `artifacts/edges.csv` / `artifacts/nodes.csv` と `events.jsonl` (試行ごとの `terminal` 行) を読み，以下を出力する:

- `network_layout.png` — NetworkX spring レイアウト: ノードはクラスタ色で着色 (拡散シードは黄色で強調)，**強紐帯** は細い灰色実線，**弱紐帯** は赤破線．弱紐帯ブリッジがクラスタを束ねている様子が見える．それを除去する (アブレーション結果) とクラスタが分断される．
- `metrics_summary.png` — 試行ごとの 3 パネルバー図: 弱紐帯ブリッジ率 (1.0 に乗るはず)・到達割合・禁制三者率．各々に試行平均を表示する．

```bash
uv run granovetter-tools visualize                      # 直近の run
uv run granovetter-tools visualize --subcommand ablation # 直近の ablation
```

| フラグ | 既定値 | 説明 |
|---|---|---|
| `--results_dir` | (runvault が解決) | run ディレクトリ |
| `--results_root` | results | 結果ルート |
| `--experiment` | granovetter | runvault 上の実験名 |
| `--subcommand` | run | 対象サブコマンド (`run` / `ablation`) |
| `--output_dir` | `figures/{run_slug}` | 図の保存先 |

## `visualize-sweep` — 到達範囲 vs パラメータ

sweep 親 run の子 run 群から試行ごとの行を組み直し (`runvault.read.sweep_events_table`)，以下を出力する:

- `sweep_reach.png` — 到達割合 vs `p_bridge` (threshold モデルでは `θ` ごとに線; 試行平均 ± 標準偏差)．`p_bridge → 0` の `≈ 1/K` から網全体へ立ち上がるティッピング的依存を示す．
- `sweep_structure.png` — 弱紐帯ブリッジ率 (`≈ 1.0` を維持しファクト7 を確認) と禁制三者率の `p_bridge` 依存．

```bash
uv run granovetter-tools visualize-sweep
```

| フラグ | 既定値 | 説明 |
|---|---|---|
| `--sweep_dir` | (runvault が解決) | sweep 親 run のディレクトリ |
| `--results_root` | results | 結果ルート |
| `--experiment` | granovetter | runvault 上の実験名 |
| `--output_dir` | `figures/{run_slug}` | 図の保存先 |

## `show-experiment-settings`

run ディレクトリの `config.json` (封筒．条件は `parameters` の下) を整形表示する．run / ablation か sweep 親かは `run.json` の `subcommand` で判別する．runvault 以前のフラットな `config.json` / `sweep_config.json` も読める．機械可読出力には `--json` を使う．

```bash
uv run granovetter-tools show-experiment-settings
uv run granovetter-tools show-experiment-settings --subcommand sweep
uv run granovetter-tools show-experiment-settings --json
```

## `reproduce` — 論文の一括再現

Rust の `reproduce` サブコマンド ([CLI](cli.ja.md) 参照) を一度呼び，`runvault path` が返した run の `events.jsonl` と `artifacts/reproduce_summary.json` を読んで比較図を run の外へ描く:

- `claim_a_weak_tie_bridges.png` — 除去方策 (`none` / `weak` / `strong` / `random`) ごとの到達割合をバーで示し，`1/K` の局所到達基準線と PASS/off 判定を注釈する．弱バーは `1/K` へ崩壊し，ランダム (対照群) バーは満到達のまま残る．
- `claim_b_threshold_tipping.png` — 到達割合 vs 閾値 `θ` の曲線．ティッピング帯を網掛けし PASS/off 判定を注釈する．`θ` の小さなシフトが到達を大域カスケードから局所カスケードへ落とす．

```bash
uv run granovetter-tools reproduce              # 論文値でフル再現
uv run granovetter-tools reproduce --quick      # 動作確認用の縮小規模
uv run granovetter-tools reproduce --seed 123
uv run granovetter-tools reproduce --skip-build # 事前に cargo build 済みのとき
```

| フラグ | 既定値 | 説明 |
|---|---|---|
| `--output-dir` | results | 結果ルート (この下に `granovetter/{run_slug}/` を作る) |
| `--seed` | 42 | 乱数シード基点 |
| `--quick` | off | 動作確認用モード (規模縮小; 論文値の検証には使わない) |
| `--skip-build` | off | `cargo build --release` をスキップ |
| `--workspace-root` | 推定 | workspace ルートの上書き (未指定時はモジュール位置から推定，または `GRANOVETTER_PROJECT_ROOT`) |

いずれかの claim の判定が `PASS` でなければプロセスは非 0 で終了する．

## フォントについて

スクリプトは日本語ラベル用に `font.family = "Hiragino Sans"` (macOS) を設定する．他プラットフォームでは `visualize.py` / `visualize_sweep.py` 冒頭の `plt.rcParams` 行を，インストール済みの CJK フォントに置き換える．
