"""reproduce_paper.py — Granovetter (1973/1978) 論文主要主張の一括再現スクリプト．

Rust バイナリの `reproduce` サブコマンド (`cargo run --release -- reproduce ...`)
を 1 度呼び出し，生成された `reproduce_<ts>/` の CSV (claim_a_ablation.csv /
claim_b_threshold.csv) と `reproduce_summary.json` を読み込んで比較図 PNG を
`reproduce_<ts>/figures/` に描く．観測値 vs 期待値の PASS/off 判定は Rust 側で
計算済みであり，本スクリプトはそれを図と最終ログに反映する．

再現する主張:

    Claim A : 弱紐帯ブリッジ効果 — 弱紐帯 (= すべての局所ブリッジ) を除去すると
              大域到達 (≈1.0) がシード所属クラスタ (≈1/K) へ崩壊し，同数の辺の
              ランダム除去では崩壊しない (対照群)．frac_weak_bridges = 1.0．
    Claim B : 閾値カスケードのティッピング (Granovetter 1978) — 閾値 θ の小さな
              上方シフトが最終カスケードサイズを大域 (reach≈1.0) から局所
              (reach≈1/K) へ跳ばせる (狭い θ 帯に集中した相転移)．

Usage:
    uv run granovetter-tools reproduce
    uv run granovetter-tools reproduce --quick          # 動作確認用 (規模縮小)
    uv run granovetter-tools reproduce --seed 123
    uv run granovetter-tools reproduce --skip-build     # 事前ビルド済みなら build をスキップ
    uv run granovetter-tools reproduce --output-dir results --workspace-root .
"""

from __future__ import annotations

import argparse
import json
import os
import shutil
import subprocess
import sys
from pathlib import Path

import matplotlib.pyplot as plt
import pandas as pd

# --------------------------------------------------------------------------- #
# 日本語フォント設定
# --------------------------------------------------------------------------- #
plt.rcParams["font.family"] = "Hiragino Sans"

# --------------------------------------------------------------------------- #
# 共通定数 / プロジェクトルート解決
# --------------------------------------------------------------------------- #

# このモジュールは tools/src/granovetter_tools/reproduce_paper.py にある．
# parents[3] が workspace ルート (= cargo workspace ルート)．
# 環境変数 GRANOVETTER_PROJECT_ROOT で上書き可能．
_env_root = os.environ.get("GRANOVETTER_PROJECT_ROOT")
if _env_root:
    PROJECT_ROOT = Path(_env_root).resolve()
else:
    PROJECT_ROOT = Path(__file__).resolve().parents[3]

COLOR_BG = "#FAFAF8"
COLOR_PASS = "#2E7D32"
COLOR_OFF = "#C62828"
_REMOVE_COLOR = {
    "none": "#90A4AE",
    "weak": "#E53935",
    "strong": "#FB8C00",
    "random": "#1E88E5",
}
_REMOVE_LABEL = {
    "none": "除去なし\n(ベースライン)",
    "weak": "弱紐帯除去",
    "strong": "強紐帯除去",
    "random": "ランダム除去\n(対照群)",
}


# --------------------------------------------------------------------------- #
# cargo 呼び出し
# --------------------------------------------------------------------------- #


def ensure_build() -> None:
    """`cargo build --release` を 1 度だけ実行する (失敗時は例外)．"""
    print("=== cargo build --release ===")
    subprocess.run(["cargo", "build", "--release"], cwd=PROJECT_ROOT, check=True)


def run_reproduce_binary(output_dir: Path, seed: int, quick: bool) -> Path:
    """Rust の `reproduce` サブコマンドを呼び出し，生成された reproduce_<ts>/ を返す．

    Rust 側は `<output_dir>/reproduce_<ts>/` を 1 つだけ作るので，呼び出し前後で
    新しく現れた `reproduce_*` ディレクトリ (最大 mtime) を採用する．
    """
    output_dir.mkdir(parents=True, exist_ok=True)
    before = {p.name for p in output_dir.iterdir() if p.is_dir()}

    args = ["reproduce", "--seed", str(seed), "--output-dir", str(output_dir)]
    if quick:
        args.append("--quick")
    cmd = ["cargo", "run", "--release", "--quiet", "--"] + args
    print("=== " + " ".join(cmd) + " ===")
    subprocess.run(cmd, cwd=PROJECT_ROOT, check=True)

    candidates = [
        p
        for p in output_dir.iterdir()
        if p.is_dir() and p.name.startswith("reproduce_") and p.name not in before
    ]
    if not candidates:
        # latest シンボリックリンクを除いた最新 reproduce_* にフォールバック．
        candidates = [
            p
            for p in output_dir.iterdir()
            if p.is_dir() and p.name.startswith("reproduce_") and p.name != "latest"
        ]
    if not candidates:
        raise RuntimeError(
            f"reproduce 呼び出し後に reproduce_<ts> ディレクトリが見つかりません: {output_dir}"
        )
    candidates.sort(key=lambda p: p.stat().st_mtime)
    return candidates[-1]


# --------------------------------------------------------------------------- #
# 描画
# --------------------------------------------------------------------------- #


def _verdict_color(verdict: str) -> str:
    return COLOR_PASS if verdict.upper() == "PASS" else COLOR_OFF


def render_claim_a(run_dir: Path, summary: dict, figures_dir: Path) -> Path:
    """Claim A: 除去方策ごとの到達割合バー + 1/K 期待線 + 判定注釈．"""
    df = pd.read_csv(run_dir / "claim_a_ablation.csv")
    claim = summary["claim_a"]
    verdict = next(
        c["verdict"] for c in summary["claims"] if c["id"] == "claim_a_weak_tie_bridges"
    )
    expected_local = float(claim["expected_local_reach"])

    fig, ax = plt.subplots(figsize=(9, 6), facecolor=COLOR_BG)
    ax.set_facecolor(COLOR_BG)

    order = ["none", "weak", "strong", "random"]
    df = df.set_index("remove").reindex(order).reset_index()
    xs = range(len(df))
    colors = [_REMOVE_COLOR.get(r, "#777777") for r in df["remove"]]
    ax.bar(xs, df["removed_reach"], color=colors, alpha=0.9)
    for x, v in zip(xs, df["removed_reach"]):
        ax.text(x, v + 0.02, f"{v:.3f}", ha="center", fontsize=10)

    ax.axhline(
        expected_local,
        color="#333333",
        lw=1.2,
        ls="--",
        label=f"局所到達期待 1/K = {expected_local:.3f}",
    )
    ax.axhline(1.0, color="#888888", lw=0.8, ls=":")
    ax.set_xticks(list(xs))
    ax.set_xticklabels([_REMOVE_LABEL.get(r, r) for r in df["remove"]], fontsize=10)
    ax.set_ylabel("到達割合 (試行平均)")
    ax.set_ylim(0, 1.1)
    ax.set_title(
        "Claim A: 弱紐帯ブリッジ効果 — 弱紐帯除去で到達が 1/K へ崩壊\n"
        f"(p_bridge={claim['p_bridge']}, β={claim['beta']}, {claim['runs']} 試行, "
        f"frac_weak_bridges={claim['frac_weak_bridges']:.3f})",
        fontsize=12,
    )
    ax.legend(fontsize=9, loc="center right")
    ax.grid(True, alpha=0.3, axis="y")
    ax.text(
        0.02,
        0.96,
        f"判定: {verdict}",
        transform=ax.transAxes,
        fontsize=13,
        fontweight="bold",
        color=_verdict_color(verdict),
        va="top",
    )

    fig.tight_layout()
    out_path = figures_dir / "claim_a_weak_tie_bridges.png"
    fig.savefig(out_path, dpi=150, bbox_inches="tight")
    plt.close(fig)
    print(f"  保存: {out_path}")
    return out_path


def render_claim_b(run_dir: Path, summary: dict, figures_dir: Path) -> Path:
    """Claim B: reach vs θ 曲線 + ティッピング帯の強調 + 判定注釈．"""
    df = pd.read_csv(run_dir / "claim_b_threshold.csv").sort_values("theta")
    claim = summary["claim_b"]
    verdict = next(
        c["verdict"] for c in summary["claims"] if c["id"] == "claim_b_threshold_tipping"
    )
    theta_low = float(claim["theta_low"])
    theta_high = float(claim["theta_high"])

    fig, ax = plt.subplots(figsize=(9, 6), facecolor=COLOR_BG)
    ax.set_facecolor(COLOR_BG)

    ax.plot(
        df["theta"],
        df["reach"],
        color="#6A1B9A",
        lw=2.2,
        marker="o",
        markersize=6,
        label="最終カスケードサイズ (reach)",
    )
    # ティッピング帯を強調する．
    lo, hi = min(theta_low, theta_high), max(theta_low, theta_high)
    ax.axvspan(lo, hi, color="#FFD54F", alpha=0.3, label=f"ティッピング帯 Δθ={hi - lo:.3f}")
    ax.axhline(0.9, color="#2E7D32", lw=0.8, ls="--", alpha=0.7)
    ax.axhline(0.2, color="#C62828", lw=0.8, ls="--", alpha=0.7)

    ax.set_xlabel("一様閾値 θ")
    ax.set_ylabel("到達割合 (試行平均)")
    ax.set_ylim(-0.02, 1.05)
    ax.set_title(
        "Claim B: 閾値カスケードのティッピング (Granovetter 1978)\n"
        f"θ の小さな上方シフトで大域→局所カスケードへ跳ぶ "
        f"(p_bridge={claim['p_bridge']}, n_seeds={claim['n_seeds']}, {claim['runs']} 試行)",
        fontsize=12,
    )
    ax.legend(fontsize=9)
    ax.grid(True, alpha=0.3)
    ax.text(
        0.02,
        0.10,
        f"判定: {verdict}",
        transform=ax.transAxes,
        fontsize=13,
        fontweight="bold",
        color=_verdict_color(verdict),
        va="bottom",
    )

    fig.tight_layout()
    out_path = figures_dir / "claim_b_threshold_tipping.png"
    fig.savefig(out_path, dpi=150, bbox_inches="tight")
    plt.close(fig)
    print(f"  保存: {out_path}")
    return out_path


# --------------------------------------------------------------------------- #
# 実行ドライバ
# --------------------------------------------------------------------------- #


def reproduce(output_root: Path, seed: int, quick: bool, skip_build: bool) -> dict:
    """Rust reproduce を呼び出し，CSV/JSON を読んで比較図を描く．"""
    print("=== Granovetter (1973/1978) 論文主要主張の一括再現 ===")
    print(f"    出力ルート : {output_root}")
    print(f"    seed       : {seed}")
    print(f"    quick      : {quick}")
    print("-------------------------------------------")

    if not skip_build:
        ensure_build()

    run_dir = run_reproduce_binary(output_root, seed=seed, quick=quick)
    summary_path = run_dir / "reproduce_summary.json"
    with summary_path.open() as f:
        summary = json.load(f)

    figures_dir = run_dir / "figures"
    figures_dir.mkdir(parents=True, exist_ok=True)

    print("--- 比較図を描画中 ---")
    fig_a = render_claim_a(run_dir, summary, figures_dir)
    fig_b = render_claim_b(run_dir, summary, figures_dir)
    summary["figure_paths"] = [str(fig_a), str(fig_b)]

    # figure_paths を反映したサマリを書き戻す (再現結果のインデックス)．
    with summary_path.open("w") as f:
        json.dump(summary, f, indent=2, ensure_ascii=False)

    print("-------------------------------------------")
    print("主張ごとの判定:")
    for c in summary["claims"]:
        print(f"  [{c['verdict']:>4}] {c['id']}")
        print(f"         期待: {c['expectation']}")
        print(f"         観測: {c['observed']}")
    print(f"サマリ → {summary_path}")
    print(f"図一覧 → {figures_dir}")
    for f in sorted(figures_dir.iterdir()):
        if f.is_file():
            size_kb = f.stat().st_size / 1024
            print(f"    {f.name:40s} ({size_kb:6.1f} KB)")

    return summary


# --------------------------------------------------------------------------- #
# CLI
# --------------------------------------------------------------------------- #


def parse_args(argv: list[str] | None = None) -> argparse.Namespace:
    p = argparse.ArgumentParser(
        prog="granovetter-tools reproduce",
        description=__doc__,
        formatter_class=argparse.RawDescriptionHelpFormatter,
    )
    p.add_argument(
        "--output-dir",
        "--output_dir",
        default="results",
        help="結果出力ルート (workspace ルートからの相対パス; default: results)",
    )
    p.add_argument(
        "--workspace-root",
        "--workspace_root",
        default=None,
        help=(
            "workspace ルート (絶対パス)．未指定時は本モジュール位置から推定する "
            "(環境変数 GRANOVETTER_PROJECT_ROOT でも上書き可)．"
        ),
    )
    p.add_argument("--seed", type=int, default=42, help="乱数シード基点 (default: 42)")
    p.add_argument(
        "--quick",
        action="store_true",
        help="簡略化モード (規模縮小; 動作確認用)．",
    )
    p.add_argument(
        "--skip-build",
        "--skip_build",
        action="store_true",
        help="cargo build --release をスキップ (事前にビルド済みのとき)．",
    )
    return p.parse_args(argv)


def main(argv: list[str] | None = None) -> int:
    args = parse_args(argv)

    global PROJECT_ROOT
    if args.workspace_root:
        PROJECT_ROOT = Path(args.workspace_root).resolve()

    if shutil.which("cargo") is None:
        print(
            "エラー: cargo コマンドが見つかりません．Rust toolchain をインストールしてください．",
            file=sys.stderr,
        )
        return 2

    output_root = Path(args.output_dir)
    if not output_root.is_absolute():
        output_root = PROJECT_ROOT / output_root

    try:
        summary = reproduce(
            output_root=output_root,
            seed=args.seed,
            quick=args.quick,
            skip_build=args.skip_build,
        )
    except Exception as e:  # noqa: BLE001
        print(f"エラー: 再現実行に失敗しました: {e}", file=sys.stderr)
        return 1

    # 1 つでも OFF があれば非 0 を返す．
    n_off = sum(1 for c in summary["claims"] if c.get("verdict", "").upper() != "PASS")
    return 0 if n_off == 0 else 1


if __name__ == "__main__":
    sys.exit(main())
