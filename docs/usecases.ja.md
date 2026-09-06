[English](usecases.md) | **日本語**

# ユースケース

本プロジェクトは Granovetter (1973)『The Strength of Weak Ties』(統制実験を持たない概念的論文) を，計測可能なエージェントベースモデルに操作化する: 密な強紐帯クラスタを疎な弱紐帯で橋渡しし，その上で情報拡散を走らせる．

## できること

1. **構造命題の検証**: 網を生成し，*すべての局所ブリッジが弱紐帯* であること (`frac_weak_bridges ≈ 1.0`) と禁じられた三者関係が抑制されていることを確認する．強紐帯クラスタは三角形閉包パスでクリーク的にされるので，クラスタ内強紐帯はブリッジになり得ない．

   ```bash
   cargo run --release -- run --clusters 10 --cluster-size 20 \
       --p-strong 0.6 --p-bridge 0.3 --diffusion si --beta 0.5 --seed 42
   uv run granovetter-tools visualize
   ```

2. **アブレーションで「弱紐帯が到達範囲を支配する」を再現**: 弱紐帯を除去すると拡散がシード所属クラスタに崩壊する様子を観察する．これが論文中心のマクロ-from-ミクロ主張である．8 クラスタ × 12 で `p_bridge=0.5`，`β=0.9` のとき，ベースライン到達は `1.0`，弱紐帯除去で `≈0.125` (8 クラスタ中 1 つ) に落ちる．

   ```bash
   cargo run --release -- ablation --remove weak --clusters 8 --cluster-size 12 \
       --p-bridge 0.5 --beta 0.9 --runs 10 --seed 42
   cargo run --release -- ablation --remove strong --clusters 8 --cluster-size 12 \
       --p-bridge 0.5 --beta 0.9 --runs 10 --seed 42
   ```

   示唆に富む注意点: *すべての* 強紐帯を除去すると到達は弱紐帯除去よりさらに下がる (`≈0.01`)．強紐帯はクラスタ内の唯一の辺なので，それを失うとクラスタが充填されず疎なブリッジ骨格だけが残るためである．Granovetter に忠実な読み方は *どのクラスタに到達できるか* の水準にある: 弱紐帯がブリッジであり，`reach_by_strength` はシードからの強紐帯のみ到達が 1 クラスタ内に閉じ，弱紐帯がクラスタ間を運ぶことを確認する．

3. **橋渡し率の走査**: `sweep` で `p_bridge` を走査し，到達範囲が単一クラスタ (`p_bridge → 0`, 到達 `≈ 1/K`) から網全体へと立ち上がるティッピング的依存を観察する．これはコミュニティ動員能力 (論文のウェストエンド対比) の構造的説明に対応する．

   ```bash
   cargo run --release -- sweep --p-bridge-min 0.0 --p-bridge-max 0.5 --p-bridge-step 0.05 \
       --diffusion si --beta 0.5 --runs 10 --seed 42
   uv run granovetter-tools visualize-sweep
   ```

4. **拡散モデルの比較**: `--diffusion threshold --theta 0.1` で SI の代わりに Granovetter (1978) 閾値カスケードを使う．`θ` が高いほどカスケードが立ち上がりにくくなる — 単一の弱紐帯接触では閾値を超えにくく，複雑伝染の議論 (Centola & Macy 2007) に接続する．

## 次に読むもの

- [CLI](cli.ja.md) — `run` / `ablation` / `sweep` のフラグ一覧．
- [可視化](visualization.ja.md) — Python ツールと図の読み方．
- [アーキテクチャ](architecture.ja.md) — 網生成器・拡散メカニズム・socsim 配線・メトリクス．
