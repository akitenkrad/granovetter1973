#!/usr/bin/env python3
"""
visualize.py — Granovetter (1973) 弱紐帯ブリッジ網 再現実験 可視化スクリプト

run ディレクトリの artifacts/edges.csv・artifacts/nodes.csv と events.jsonl を読み，
網レイアウト図 (クラスタごとに着色したノード，強紐帯=実線・弱紐帯=赤破線) と，
試行ごとの指標バー図を生成する．局所ブリッジ (= 弱紐帯) を強調表示する．

--results_dir を省略すると
`runvault path --experiment granovetter --latest --subcommand run --standalone`
が返す run ディレクトリを対象にする (`runvault` が PATH にある必要がある)．
試行ごとの指標は metrics.csv ではなく events.jsonl の terminal 行にある
(時間軸を持たない値を metrics.csv に並べると主キーが衝突するため)．

Usage:
    uv run granovetter-tools visualize
    uv run granovetter-tools visualize --subcommand ablation
    uv run granovetter-tools visualize --results_dir results/granovetter/run_20260831_...
    uv run granovetter-tools visualize --output_dir out

Outputs:
    output_dir/
    ├── network_layout.png   ← クラスタ着色 + 強/弱紐帯の網レイアウト
    └── metrics_summary.png  ← 弱紐帯ブリッジ率・到達割合・禁制三者率の試行集計
"""

from __future__ import annotations

import argparse
import os

import matplotlib.pyplot as plt
import networkx as nx
import numpy as np
import pandas as pd

from runvault.read import artifacts_dir, events_table, figures_dir, runvault_path

# --------------------------------------------------------------------------- #
# 日本語フォント設定
# --------------------------------------------------------------------------- #
plt.rcParams["font.family"] = "Hiragino Sans"

# --------------------------------------------------------------------------- #
# カラー設定
# --------------------------------------------------------------------------- #
COLOR_BG = "#FAFAF8"
COLOR_STRONG = "#90A4AE"  # 強紐帯 (実線・薄灰)
COLOR_WEAK = "#E53935"    # 弱紐帯 (破線・赤; ブリッジ)
COLOR_SEED = "#FFC107"    # シードノード
_CLUSTER_PALETTE = [
    "#2196F3", "#4CAF50", "#9C27B0", "#FF9800", "#00BCD4",
    "#795548", "#607D8B", "#E91E63", "#3F51B5", "#8BC34A",
    "#FF5722", "#009688", "#673AB7", "#CDDC39", "#F44336",
]


def cluster_color(c: int) -> str:
    return _CLUSTER_PALETTE[c % len(_CLUSTER_PALETTE)]


# --------------------------------------------------------------------------- #
# データ読み込み
# --------------------------------------------------------------------------- #

def load_edges(path: str) -> pd.DataFrame:
    if not os.path.exists(path):
        raise FileNotFoundError(f"edges.csv が見つかりません: {path}")
    return pd.read_csv(path)


def load_nodes(path: str) -> pd.DataFrame | None:
    if not os.path.exists(path):
        return None
    return pd.read_csv(path)


def load_trials(run_dir: str) -> pd.DataFrame | None:
    """試行ごとの指標を events.jsonl の terminal 行から読む．"""
    if not os.path.exists(os.path.join(run_dir, "events.jsonl")):
        return None
    return events_table(run_dir, kind="terminal")


def build_graph(edges: pd.DataFrame, nodes: pd.DataFrame | None) -> nx.Graph:
    g = nx.Graph()
    if nodes is not None:
        for _, row in nodes.iterrows():
            g.add_node(
                int(row["id"]),
                cluster=int(row["cluster"]),
                is_seed=bool(int(row["is_seed"])),
            )
    for _, row in edges.iterrows():
        g.add_edge(int(row["a"]), int(row["b"]), strength=str(row["strength"]))
    return g


# --------------------------------------------------------------------------- #
# 可視化関数
# --------------------------------------------------------------------------- #

def save_network_layout(g: nx.Graph, out_path: str, subtitle: str = "") -> None:
    """クラスタ着色 + 強/弱紐帯の網レイアウトを保存する．"""
    fig, ax = plt.subplots(figsize=(10, 8), facecolor=COLOR_BG)
    ax.set_facecolor(COLOR_BG)

    # spring レイアウト (弱紐帯ブリッジでクラスタが緩やかに分離する)．
    pos = nx.spring_layout(g, seed=42, k=0.5 / max(np.sqrt(g.number_of_nodes()), 1))

    # 辺を強度別に描く．
    strong_edges = [(u, v) for u, v, d in g.edges(data=True) if d.get("strength") == "strong"]
    weak_edges = [(u, v) for u, v, d in g.edges(data=True) if d.get("strength") == "weak"]
    nx.draw_networkx_edges(g, pos, edgelist=strong_edges, edge_color=COLOR_STRONG, width=0.8, alpha=0.6, ax=ax)
    nx.draw_networkx_edges(
        g, pos, edgelist=weak_edges, edge_color=COLOR_WEAK, width=1.8,
        style="dashed", alpha=0.9, ax=ax,
    )

    # ノードをクラスタ色で描く (シードは黄色で強調)．
    node_colors = []
    node_sizes = []
    edgecolors = []
    for n, d in g.nodes(data=True):
        if d.get("is_seed"):
            node_colors.append(COLOR_SEED)
            node_sizes.append(120)
            edgecolors.append("#333333")
        else:
            node_colors.append(cluster_color(d.get("cluster", 0)))
            node_sizes.append(50)
            edgecolors.append("none")
    nx.draw_networkx_nodes(
        g, pos, node_color=node_colors, node_size=node_sizes,
        edgecolors=edgecolors, linewidths=1.0, ax=ax,
    )

    title = "弱紐帯ブリッジ網 (クラスタ着色 / 赤破線=弱紐帯ブリッジ / 黄=シード)"
    if subtitle:
        title += f"\n{subtitle}"
    ax.set_title(title, fontsize=12)
    ax.axis("off")

    fig.tight_layout()
    fig.savefig(out_path, dpi=150, bbox_inches="tight")
    plt.close(fig)
    print(f"  保存: {out_path}")


def save_metrics_summary(df: pd.DataFrame, out_path: str) -> None:
    """試行ごとの主要指標をバー表示する (破線は試行平均)．"""
    fig, axes = plt.subplots(1, 3, figsize=(14, 4.5), facecolor=COLOR_BG)
    fig.suptitle("Granovetter 弱紐帯ブリッジ網 — メトリクス試行集計", fontsize=13)

    metrics = [
        ("frac_weak_bridges", "弱紐帯ブリッジ率", "#E53935", (0, 1.05)),
        ("reach_fraction", "到達割合", "#2196F3", (0, 1.05)),
        ("forbidden_triad_rate", "禁制三者率", "#9C27B0", None),
    ]
    for ax, (col, label, color, ylim) in zip(axes, metrics):
        ax.set_facecolor(COLOR_BG)
        vals = df[col].to_numpy() if col in df.columns else np.array([])
        if vals.size:
            ax.bar(range(len(vals)), vals, color=color, alpha=0.85)
            ax.axhline(float(np.mean(vals)), color="#333333", lw=1.0, ls="--",
                       label=f"平均 {np.mean(vals):.3f}")
            ax.legend(fontsize=9)
        ax.set_xlabel("試行 (run)")
        ax.set_ylabel(label)
        ax.set_title(label)
        if ylim:
            ax.set_ylim(*ylim)
        ax.grid(True, alpha=0.3, axis="y")

    fig.tight_layout()
    fig.savefig(out_path, dpi=150, bbox_inches="tight")
    plt.close(fig)
    print(f"  保存: {out_path}")


# --------------------------------------------------------------------------- #
# メイン
# --------------------------------------------------------------------------- #

def parse_args(argv: list[str] | None = None) -> argparse.Namespace:
    p = argparse.ArgumentParser(
        prog="granovetter-tools visualize",
        description="Granovetter 弱紐帯ブリッジ網 可視化スクリプト",
    )
    p.add_argument(
        "--results_dir", "--results-dir", default=None,
        help="run ディレクトリ (省略時は runvault が返す直近の完了 run)",
    )
    p.add_argument(
        "--results_root", "--results-root", default="results",
        help="結果ルート (default: results)",
    )
    p.add_argument(
        "--experiment", default="granovetter",
        help="runvault 上の実験名 (default: granovetter)",
    )
    p.add_argument(
        "--subcommand", default="run",
        help="対象サブコマンド: run / ablation (default: run)",
    )
    p.add_argument(
        "--output_dir", "--output-dir", default=None,
        help="図の保存先ディレクトリ (default: run の外の figures/{run_slug})",
    )
    return p.parse_args(argv)


def main(argv: list[str] | None = None) -> None:
    args = parse_args(argv)

    results_dir = args.results_dir
    if results_dir is None:
        results_dir = runvault_path(
            args.experiment, args.results_root,
            subcommand=args.subcommand, standalone=True,
        )

    artifacts = artifacts_dir(results_dir)
    edges_path = os.path.join(artifacts, "edges.csv")
    nodes_path = os.path.join(artifacts, "nodes.csv")
    # 図は run が終わった後に作るものなので run ディレクトリの外に置く
    # (manifest.csv は finish() が確定させるので，後から足すと食い違う)．
    out_dir = args.output_dir if args.output_dir else figures_dir(results_dir)

    os.makedirs(out_dir, exist_ok=True)

    print("=== Granovetter 弱紐帯ブリッジ網 可視化 ===")
    print(f"run:      {results_dir}")
    print(f"辺リスト: {edges_path}")
    print(f"ノード:   {nodes_path}")
    print(f"出力先:   {out_dir}")
    print("-------------------------------------------")

    print("[1/2] 網レイアウトを描画中 ...")
    edges = load_edges(edges_path)
    nodes = load_nodes(nodes_path)
    g = build_graph(edges, nodes)
    n_weak = sum(1 for _, _, d in g.edges(data=True) if d.get("strength") == "weak")
    save_network_layout(
        g, os.path.join(out_dir, "network_layout.png"),
        subtitle=f"{g.number_of_nodes()} ノード / {g.number_of_edges()} 辺 (弱紐帯 {n_weak} 本)",
    )

    print("[2/2] 試行ごとの指標を描画中 ...")
    df_m = load_trials(results_dir)
    if df_m is not None and not df_m.empty:
        save_metrics_summary(df_m, os.path.join(out_dir, "metrics_summary.png"))
    else:
        print("  events.jsonl が無いか terminal 行が空のためスキップ")

    print("-------------------------------------------")
    print("完了．出力ファイル一覧:")
    for f in sorted(os.listdir(out_dir)):
        size_kb = os.path.getsize(os.path.join(out_dir, f)) / 1024
        print(f"  {f:35s} ({size_kb:6.1f} KB)")


if __name__ == "__main__":
    main()
