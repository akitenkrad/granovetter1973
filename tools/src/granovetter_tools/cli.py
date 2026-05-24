"""granovetter-tools — Granovetter (1973) 弱紐帯ブリッジ網 ツール統合 CLI．

Usage:
    granovetter-tools visualize [...]
    granovetter-tools visualize-sweep [...]
    granovetter-tools show-experiment-settings [...]

各サブコマンドに続く引数は，対応するモジュールの argparse がそのまま受け取る．
サブコマンドレベルで `--help` を付けると，そのサブコマンド自身のヘルプが表示される．
"""

from __future__ import annotations

import argparse
import sys


def main(argv: list[str] | None = None) -> None:
    parser = argparse.ArgumentParser(
        prog="granovetter-tools",
        description="Granovetter (1973) The Strength of Weak Ties 可視化・分析ツール",
    )
    subparsers = parser.add_subparsers(dest="command", required=True)
    subparsers.add_parser(
        "visualize", help="単一実行結果 (網レイアウト + 拡散ラウンド) の可視化", add_help=False
    )
    subparsers.add_parser(
        "visualize-sweep", help="スイープ結果 (到達割合 vs パラメータ) の可視化", add_help=False
    )
    subparsers.add_parser(
        "show-experiment-settings",
        help="実行結果ディレクトリの設定 (config.json / sweep_config.json) の表示",
        add_help=False,
    )

    argv = sys.argv[1:] if argv is None else argv
    if not argv or argv[0] in {"-h", "--help"}:
        parser.parse_args(argv)
        return

    command = argv[0]
    rest = argv[1:]
    if command == "visualize":
        from granovetter_tools.visualize import main as run_main
        run_main(rest)
    elif command == "visualize-sweep":
        from granovetter_tools.visualize_sweep import main as run_main
        run_main(rest)
    elif command == "show-experiment-settings":
        from granovetter_tools.show_experiment_settings import main as run_main
        run_main(rest)
    else:
        # 未知のコマンドは argparse のエラーメッセージに委ねる
        parser.parse_args(argv)


if __name__ == "__main__":
    main()
