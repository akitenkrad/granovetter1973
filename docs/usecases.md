**English** | [日本語](usecases.ja.md)

# Use cases

This project operationalizes Granovetter's (1973) *The Strength of Weak Ties* — a conceptual paper with no controlled experiment — into a measurable agent-based model: dense strong-tie clusters bridged sparsely by weak ties, with information diffusion on top.

## What you can do

1. **Verify the structural propositions.** Generate the network and confirm that *all local bridges are weak ties* (`frac_weak_bridges ≈ 1.0`) and that the forbidden triad is suppressed. The strong-tie clusters are made clique-ish by a triangle-closure pass, so no intra-cluster strong tie can be a bridge.

   ```bash
   cargo run --release -- run --clusters 10 --cluster-size 20 \
       --p-strong 0.6 --p-bridge 0.3 --diffusion si --beta 0.5 --seed 42
   uv run granovetter-tools visualize
   ```

2. **Reproduce "weak ties dominate reach" via ablation.** Remove the weak ties and watch diffusion collapse to the seed's own cluster; this is the paper's central macro-from-micro claim. With `p_bridge=0.5`, `β=0.9` over 8 clusters of 12, the baseline reach is `1.0` and weak-removal drops it to `≈0.125` (one cluster out of eight).

   ```bash
   cargo run --release -- ablation --remove weak --clusters 8 --cluster-size 12 \
       --p-bridge 0.5 --beta 0.9 --runs 10 --seed 42
   cargo run --release -- ablation --remove strong --clusters 8 --cluster-size 12 \
       --p-bridge 0.5 --beta 0.9 --runs 10 --seed 42
   ```

   Note an instructive subtlety: removing *all* strong ties drops reach below even the weak-removal case (`≈0.01`), because the strong ties are the only intra-cluster edges — without them the clusters cannot fill in and only the sparse bridge skeleton remains. The faithful Granovetter reading is at the level of *which clusters are reachable*: weak ties are the bridges, and `reach_by_strength` confirms that strong-only reach from the seed stays inside one cluster while weak ties carry information across clusters.

3. **Sweep the bridging rate.** Run `sweep` over `p_bridge` and watch reach rise from a single cluster (`p_bridge → 0`, reach `≈ 1/K`) toward full network reach — the tipping-like dependence that explains community mobilization capacity (the paper's West End contrast).

   ```bash
   cargo run --release -- sweep --p-bridge-min 0.0 --p-bridge-max 0.5 --p-bridge-step 0.05 \
       --diffusion si --beta 0.5 --runs 10 --seed 42
   uv run granovetter-tools visualize-sweep
   ```

4. **Compare diffusion models.** Switch `--diffusion threshold --theta 0.1` to use the Granovetter (1978) threshold cascade instead of SI. Higher `θ` makes cascades harder to ignite — a single weak-tie contact rarely crosses the threshold, which touches the complex-contagion critique (Centola & Macy 2007).

## Where to go next

- [CLI](cli.md) — the full flag reference for `run`, `ablation`, and `sweep`.
- [Visualization](visualization.md) — the Python tools and how to read the figures.
- [Architecture](architecture.md) — the network generator, the diffusion mechanism, the socsim wiring, and the metrics.
