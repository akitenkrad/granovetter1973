#!/usr/bin/env python3
"""
visualize_sweep.py — Granovetter (1973) パラメータスイープ結果 可視化スクリプト

sweep 親 run の子 run 群から試行ごとの行を組み直し，
「到達割合 vs 橋渡し率 p_bridge」(θ ごとに線; 閾値モデル時) と，
「禁制三者率 / 弱紐帯ブリッジ率 vs p_bridge」を生成する．到達割合は試行平均±標準偏差．

--sweep_dir を省略すると
`runvault path --experiment granovetter --latest --subcommand sweep`
が返す親 run を対象にする (`runvault` が PATH にある必要がある)．条件ごとの表は
ディスクに無いので，子 run の config.json と events.jsonl から組み直す．

Usage:
    uv run granovetter-tools visualize-sweep
    uv run granovetter-tools visualize-sweep --sweep_dir results/granovetter/sweep_20260831_...

Outputs:
    output_dir/
    ├── sweep_reach.png     ← 到達割合 vs p_bridge (θ ごと)
    └── sweep_structure.png ← 弱紐帯ブリッジ率・禁制三者率 vs p_bridge
"""

from __future__ import annotations

import argparse
import os

import matplotlib.pyplot as plt
import pandas as pd

from runvault.read import figures_dir, runvault_path, sweep_events_table

# --------------------------------------------------------------------------- #
# 日本語フォント設定
# --------------------------------------------------------------------------- #
plt.rcParams["font.family"] = "Hiragino Sans"

# --------------------------------------------------------------------------- #
# カラー設定
# --------------------------------------------------------------------------- #
COLOR_BG = "#FAFAF8"
_THETA_PALETTE = ["#2196F3", "#4CAF50", "#9C27B0", "#FF9800", "#00BCD4", "#E91E63"]


def theta_color(idx: int) -> str:
    return _THETA_PALETTE[idx % len(_THETA_PALETTE)]


# --------------------------------------------------------------------------- #
# データ読み込み・集計
# --------------------------------------------------------------------------- #

def load_summary(sweep_dir: str) -> pd.DataFrame:
    """試行 1 本 1 行の表を子 run から組み直す (旧 sweep_summary.csv 相当)．"""
    return sweep_events_table(sweep_dir, ["theta", "p_bridge"], kind="terminal")


def aggregate(df: pd.DataFrame) -> pd.DataFrame:
    """(theta, p_bridge) ごとに各指標の平均・標準偏差を集計する．"""
    agg = (
        df.groupby(["theta", "p_bridge"])
        .agg(
            reach_mean=("reach_fraction", "mean"),
            reach_std=("reach_fraction", "std"),
            frac_weak_mean=("frac_weak_bridges", "mean"),
            forbidden_mean=("forbidden_triad_rate", "mean"),
        )
        .reset_index()
    )
    agg["reach_std"] = agg["reach_std"].fillna(0.0)
    return agg


# --------------------------------------------------------------------------- #
# 可視化関数
# --------------------------------------------------------------------------- #

def save_reach(agg: pd.DataFrame, out_path: str) -> None:
    """到達割合 vs p_bridge を θ ごとに重ね描く．"""
    fig, ax = plt.subplots(figsize=(9, 6), facecolor=COLOR_BG)
    ax.set_facecolor(COLOR_BG)

    thetas = sorted(agg["theta"].unique())
    for idx, theta in enumerate(thetas):
        sub = agg[agg["theta"] == theta].sort_values("p_bridge")
        c = theta_color(idx)
        label = f"θ={theta:.2f}" if len(thetas) > 1 else "reach"
        ax.errorbar(
            sub["p_bridge"], sub["reach_mean"], yerr=sub["reach_std"],
            color=c, lw=1.8, marker="o", markersize=5, capsize=3,
            label=label, alpha=0.9,
        )

    ax.set_xlabel("クラスタ間橋渡し率 p_bridge")
    ax.set_ylabel("到達割合 (試行平均)")
    ax.set_ylim(-0.02, 1.05)
    ax.set_title("到達割合 vs 橋渡し率 p_bridge\n(p_bridge→0 で到達範囲が急落; 弱紐帯ブリッジの支配性)", fontsize=12)
    ax.legend(fontsize=9)
    ax.grid(True, alpha=0.3)

    fig.tight_layout()
    fig.savefig(out_path, dpi=150, bbox_inches="tight")
    plt.close(fig)
    print(f"  保存: {out_path}")


def save_structure(agg: pd.DataFrame, out_path: str) -> None:
    """弱紐帯ブリッジ率・禁制三者率 vs p_bridge を描く (θ で平均)．"""
    # θ で平均してから p_bridge 依存を描く．
    by_pb = (
        agg.groupby("p_bridge")
        .agg(frac_weak=("frac_weak_mean", "mean"), forbidden=("forbidden_mean", "mean"))
        .reset_index()
        .sort_values("p_bridge")
    )

    fig, ax = plt.subplots(figsize=(9, 6), facecolor=COLOR_BG)
    ax.set_facecolor(COLOR_BG)
    ax.plot(by_pb["p_bridge"], by_pb["frac_weak"], color="#E53935", lw=2, marker="o",
            label="弱紐帯ブリッジ率 (≈1.0 が命題)")
    ax.plot(by_pb["p_bridge"], by_pb["forbidden"], color="#9C27B0", lw=2, marker="s",
            label="禁制三者率")
    ax.axhline(1.0, color="#888888", lw=0.8, ls="--")
    ax.set_xlabel("クラスタ間橋渡し率 p_bridge")
    ax.set_ylabel("割合")
    ax.set_ylim(-0.02, 1.05)
    ax.set_title("構造命題の確認: すべての局所ブリッジは弱紐帯 (論文ファクト7)", fontsize=12)
    ax.legend(fontsize=9)
    ax.grid(True, alpha=0.3)

    fig.tight_layout()
    fig.savefig(out_path, dpi=150, bbox_inches="tight")
    plt.close(fig)
    print(f"  保存: {out_path}")


# --------------------------------------------------------------------------- #
# メイン
# --------------------------------------------------------------------------- #

def parse_args(argv: list[str] | None = None) -> argparse.Namespace:
    p = argparse.ArgumentParser(
        prog="granovetter-tools visualize-sweep",
        description="Granovetter パラメータスイープ結果 可視化スクリプト",
    )
    p.add_argument(
        "--sweep_dir", "--sweep-dir", default=None,
        help="sweep 親 run のディレクトリ (省略時は runvault が返す直近の完了 sweep)",
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
        "--output_dir", "--output-dir", default=None,
        help="図の保存先ディレクトリ (default: run の外の figures/{run_slug})",
    )
    return p.parse_args(argv)


def main(argv: list[str] | None = None) -> None:
    args = parse_args(argv)

    sweep_dir = args.sweep_dir
    if sweep_dir is None:
        sweep_dir = runvault_path(args.experiment, args.results_root, subcommand="sweep")

    # 図は run が終わった後に作るものなので run ディレクトリの外に置く．
    out_dir = args.output_dir if args.output_dir else figures_dir(sweep_dir)
    os.makedirs(out_dir, exist_ok=True)

    print("=== Granovetter スイープ可視化 ===")
    print(f"スイープ: {sweep_dir}")
    print(f"出力先:   {out_dir}")
    print("---------------------------------")

    print("[1/2] 子 run から条件ごとの試行を組み直し中 ...")
    df = load_summary(sweep_dir)
    agg = aggregate(df)
    print(f"      θ {df['theta'].nunique()} 値 × p_bridge {df['p_bridge'].nunique()} 値")

    print("[2/2] 図を保存中 ...")
    save_reach(agg, os.path.join(out_dir, "sweep_reach.png"))
    save_structure(agg, os.path.join(out_dir, "sweep_structure.png"))

    print("---------------------------------")
    print("完了．出力ファイル一覧:")
    for f in sorted(os.listdir(out_dir)):
        size_kb = os.path.getsize(os.path.join(out_dir, f)) / 1024
        print(f"  {f:35s} ({size_kb:6.1f} KB)")


if __name__ == "__main__":
    main()
