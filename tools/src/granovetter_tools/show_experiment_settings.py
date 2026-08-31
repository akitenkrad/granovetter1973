"""granovetter-tools show-experiment-settings — 実行結果の設定表示．

runvault の run ディレクトリの config.json (封筒．条件は `parameters` の下) を読み，
実行時に使われた全パラメータを整形表示する．run / ablation か sweep 親かは
run.json の subcommand で判別する．runvault 以前のフラットな config.json /
sweep_config.json も読める．

--results-dir を省略すると
`runvault path --experiment granovetter --latest --subcommand run --standalone`
が返す run ディレクトリを対象にする (`runvault` が PATH にある必要がある)．

Usage:
    granovetter-tools show-experiment-settings
    granovetter-tools show-experiment-settings --subcommand sweep
    granovetter-tools show-experiment-settings --results-dir results/granovetter/run_20260831_...
    granovetter-tools show-experiment-settings --json

run 設定テーブルは共有ヘルパ `socsim_tools` に委譲する (出力はバイト等価)．
本論文は非 LLM のため run_metadata ブロックは無い．sweep 設定テーブル (複合行 p_bridge
走査 / θ 候補連結) と `--json` の `kind` フィールドは granovetter 固有なので本モジュールに残す．
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

from runvault.read import config_parameters, load_run_meta, runvault_path
from socsim_tools.io import load_config, resolve_results_dir
from socsim_tools.settings import render_run_config

# config キー → 表示ラベル (右コロン位置を揃えるため空白パディング済み)．
# render_run_config が `f"{label}: {value}"` で整形するため，ラベルは末尾の
# `: ` を含めず，従来の run レンダラと同じ桁揃えになるようパディングする．
FIELD_LABELS = {
    "clusters": "クラスタ数 K       ",
    "cluster_size": "クラスタサイズ     ",
    "n_agents": "総エージェント数   ",
    "p_strong": "強紐帯密度 p_strong",
    "p_bridge": "橋渡し率 p_bridge  ",
    "p_weak_intra": "クラスタ内弱紐帯   ",
    "diffusion": "拡散モデル         ",
    "beta": "SI 感染確率 β      ",
    "theta": "閾値 θ             ",
    "remove": "除去対象 (remove)  ",
    "n_seeds": "シード数 n_seeds   ",
    "runs": "試行数 runs        ",
    "max_iterations": "最大反復           ",
    "seed": "乱数シード基点     ",
}


def render_sweep_config(cfg: dict, source: Path) -> str:
    """sweep 設定テーブルを整形する (granovetter 固有; 複合行 + θ 候補連結)．"""
    lines: list[str] = []
    lines.append("=" * 70)
    lines.append("実行設定 (sweep)")
    lines.append("=" * 70)
    lines.append(f"設定ファイル: {source}")
    lines.append("-" * 70)
    lines.append(
        f"p_bridge 走査      : {cfg.get('p_bridge_min', '-')}:{cfg.get('p_bridge_max', '-')}:{cfg.get('p_bridge_step', '-')}"
    )
    thetas = cfg.get("theta_values", [])
    lines.append(f"θ 候補             : {', '.join(map(str, thetas)) if thetas else '-'}")
    lines.append(f"クラスタ数 K       : {cfg.get('clusters', '-')}")
    lines.append(f"クラスタサイズ     : {cfg.get('cluster_size', '-')}")
    lines.append(f"強紐帯密度 p_strong: {cfg.get('p_strong', '-')}")
    lines.append(f"拡散モデル         : {cfg.get('diffusion', '-')}")
    lines.append(f"SI 感染確率 β      : {cfg.get('beta', '-')}")
    lines.append(f"シード数 n_seeds   : {cfg.get('n_seeds', '-')}")
    lines.append(f"試行数 runs        : {cfg.get('runs', '-')}")
    lines.append(f"最大反復           : {cfg.get('max_iterations', '-')}")
    lines.append(f"乱数シード基点     : {cfg.get('seed', '-')}")
    lines.append("=" * 70)
    return "\n".join(lines)


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(
        prog="granovetter-tools show-experiment-settings",
        description=__doc__,
        formatter_class=argparse.RawDescriptionHelpFormatter,
    )
    parser.add_argument(
        "--results-dir", "--results_dir",
        default=None,
        help="run ディレクトリ (省略時は runvault が返す直近の完了 run)",
    )
    parser.add_argument(
        "--results-root", "--results_root",
        default="results",
        help="結果ルート (default: results)",
    )
    parser.add_argument(
        "--experiment",
        default="granovetter",
        help="runvault 上の実験名 (default: granovetter)",
    )
    parser.add_argument(
        "--subcommand",
        default="run",
        help="対象サブコマンド: run / ablation / sweep / reproduce (default: run)",
    )
    parser.add_argument(
        "--json",
        action="store_true",
        help="表ではなく JSON 形式で出力する．",
    )
    args = parser.parse_args(argv)

    if args.results_dir is None:
        # sweep 親は子と subcommand が違うので standalone は要らない．
        standalone = args.subcommand != "sweep"
        try:
            results_dir = Path(
                runvault_path(
                    args.experiment, args.results_root,
                    subcommand=args.subcommand, standalone=standalone,
                )
            )
        except SystemExit as exc:
            print(f"エラー: {exc}", file=sys.stderr)
            return 1
    else:
        results_dir = resolve_results_dir(args.results_dir)
    if not results_dir.exists():
        print(f"エラー: ディレクトリが存在しません: {results_dir}", file=sys.stderr)
        return 1

    meta = load_run_meta(results_dir, required=False)
    if meta is not None:
        cfg = config_parameters(results_dir)
        cfg_path = results_dir / "config.json"
        subcommand = str(meta["subcommand"])
    else:
        # runvault 以前のフラットな出力 (config.json / sweep_config.json)．
        try:
            cfg, cfg_path = load_config(results_dir)
        except FileNotFoundError as exc:
            print(f"エラー: {exc}", file=sys.stderr)
            return 1
        subcommand = str(cfg.get("command", "run")) if cfg_path.name == "config.json" else "sweep"
    kind = "sweep" if subcommand == "sweep" else "run"

    if args.json:
        payload = {"source": str(cfg_path), "kind": kind, "config": cfg}
        print(json.dumps(payload, indent=2, ensure_ascii=False))
    elif kind == "run":
        print(render_run_config(cfg, cfg_path, FIELD_LABELS, title=f"実行設定 ({subcommand})"))
    else:
        print(render_sweep_config(cfg, cfg_path))
    return 0


if __name__ == "__main__":
    sys.exit(main())
