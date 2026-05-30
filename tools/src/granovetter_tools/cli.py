"""granovetter-tools — Granovetter (1973) 弱紐帯ブリッジ網 ツール統合 CLI．

Usage:
    granovetter-tools visualize [...]
    granovetter-tools visualize-sweep [...]
    granovetter-tools show-experiment-settings [...]

各サブコマンドに続く引数は，対応するモジュールの argparse がそのまま受け取る．
サブコマンドレベルで `--help` を付けると，そのサブコマンド自身のヘルプが表示される．

dispatcher の組み立ては共有ヘルパ `socsim_tools.cli.build_dispatcher` に委譲する
(prog 名・サブコマンド・ヘルプ文・argv ルーティングは従来と同一)．可視化/設定表示の
実体 (visualize / visualize_sweep / show_experiment_settings) は repo 固有のまま．
"""

from __future__ import annotations

from socsim_tools.cli import build_dispatcher

main = build_dispatcher(
    prog="granovetter-tools",
    description="Granovetter (1973) The Strength of Weak Ties 可視化・分析ツール",
    subcommands={
        "visualize": (
            "単一実行結果 (網レイアウト + 拡散ラウンド) の可視化",
            "granovetter_tools.visualize:main",
        ),
        "visualize-sweep": (
            "スイープ結果 (到達割合 vs パラメータ) の可視化",
            "granovetter_tools.visualize_sweep:main",
        ),
        "show-experiment-settings": (
            "実行結果ディレクトリの設定 (config.json / sweep_config.json) の表示",
            "granovetter_tools.show_experiment_settings:main",
        ),
        "reproduce": (
            "論文 (1973/1978) 主要主張の一括再現 (観測値 vs 期待値 + PASS/off 判定)",
            "granovetter_tools.reproduce_paper:main",
        ),
    },
)


if __name__ == "__main__":
    main()
