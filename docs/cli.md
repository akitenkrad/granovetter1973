**English** | [日本語](cli.ja.md)

# CLI

The Rust binary `granovetter` (run via `cargo run --release -- …`) exposes four subcommands: `run`, `ablation`, `sweep`, and `reproduce`.

## `run` — network generation + diffusion

Generate one weak-tie bridging network and run information diffusion on it.

```bash
cargo run --release -- run \
    --clusters 10 --cluster-size 20 \
    --p-strong 0.6 --p-bridge 0.3 \
    --diffusion si --beta 0.5 --runs 1 --seed 42
```

| Flag | Default | Description |
|---|---|---|
| `--clusters` | 10 | number of clusters `K` |
| `--cluster-size` | 20 | agents per cluster |
| `--p-strong` | 0.6 | intra-cluster strong-tie ER probability (density knob; a ring + triangle closure guarantee connectivity and clique-ish strong ties) |
| `--p-bridge` | 0.3 | per cluster-pair weak-tie bridge probability |
| `--p-weak-intra` | 0.0 | intra-cluster weak-tie probability (continuity approximation) |
| `--diffusion` | si | diffusion model: `si` / `threshold` |
| `--beta` | 0.5 | SI infection probability `β` |
| `--theta` | 0.2 | threshold `θ` (threshold model) |
| `--n-seeds` | 1 | number of diffusion seeds (agents `0..n_seeds`, in cluster 0) |
| `--runs` | 1 | independent trials (one metrics row each) |
| `--max-iterations` | 200 | cascade round limit |
| `--seed` | random | RNG seed base |
| `--output-dir` | results | output base directory |

Outputs (under `results/{YYYYMMDD_HHMMSS}/`):

- `config.json` — the run settings.
- `metrics.csv` — one row per trial: `run, seed, n_agents, n_edges, n_local_bridges, frac_weak_bridges, forbidden_triad_rate, reach_fraction, avg_path_length, largest_component_fraction, reach_strong_only, reach_weak_only, cascade_rounds`.
- `edges.csv` — the network: `a, b, strength` (`strong` / `weak`).
- `nodes.csv` — node cluster assignment: `id, cluster, is_seed`.

A `results/latest` symlink is refreshed to point at the new directory.

## `ablation` — edge-removal experiment

Clone the world, remove the selected edges (`remove_edge`), re-run diffusion, and report the reach delta against the no-removal baseline. This is the paper's headline result.

```bash
cargo run --release -- ablation --remove weak \
    --clusters 10 --cluster-size 20 --p-strong 0.6 --p-bridge 0.5 \
    --diffusion si --beta 0.9 --runs 10 --seed 42
```

`--remove` takes `none` / `weak` / `strong` / `random` (random removes the same number of edges as there are weak ties, as a control). The other flags match `run` (with `--seed` defaulting to 42 and `--runs` to 10). Outputs are the same layout as `run`, with `config.json` recording `command: "ablation"` and the metrics computed on the **ablated** network.

Typical result (`--remove weak`, 8×12 clusters, `p_bridge=0.5`, `β=0.9`): baseline reach `1.0` → weak-removed `≈0.125` (the seed's one cluster out of eight). With `--remove strong` reach drops even further (`≈0.01`), because removing *all* strong ties leaves only the sparse bridge skeleton, and most non-bridge nodes then have no incident edge to spread through — the within-cluster fill-in that strong ties provide is gone. See [Use cases](usecases.md) for the interpretation.

## `sweep` — parameter sweep

Sweep `p_bridge` (and `theta`, for the threshold model) and aggregate reach and the structural metrics per condition.

```bash
cargo run --release -- sweep \
    --p-bridge-min 0.0 --p-bridge-max 0.5 --p-bridge-step 0.05 \
    --theta-values 0.1,0.2,0.3 \
    --diffusion si --beta 0.5 --runs 10 --seed 42
```

| Flag | Default | Description |
|---|---|---|
| `--p-bridge-min` / `--p-bridge-max` / `--p-bridge-step` | 0.0 / 0.5 / 0.05 | `p_bridge` sweep range |
| `--theta-values` | 0.1,0.2,0.3 | comma-separated `θ` candidates (threshold model only; ignored under `si`) |
| `--clusters` / `--cluster-size` / `--p-strong` | 10 / 20 / 0.6 | network parameters |
| `--diffusion` / `--beta` / `--n-seeds` | si / 0.5 / 1 | diffusion parameters |
| `--runs` | 10 | independent trials per condition |
| `--max-iterations` | 200 | cascade round limit |
| `--seed` | 42 | seed base (each trial derives an independent seed) |
| `--output-dir` | results | output base directory |

Each trial derives an independent seed via `derive_seed(seed, &[theta.bits, p_bridge.bits, run])`. Outputs (under `results/{YYYYMMDD_HHMMSS}_sweep/`):

- `sweep_config.json` — the sweep settings.
- `sweep_summary.csv` — one row per `(p_bridge, theta, run)`: `p_bridge, theta, run, seed, reach_fraction, frac_weak_bridges, forbidden_triad_rate, avg_path_length, largest_component_fraction, cascade_rounds`.

Under `si`, `theta` is fixed to a single placeholder value (it is unused), so the sweep runs only over `p_bridge`.

## `reproduce` — one-shot paper reproduction

Reproduce the paper's headline quantitative claims in one command, emitting an observed-vs-expected comparison with PASS/off verdicts. It runs the network + diffusion internally (deterministic, seeded; no subprocess), so there are no timing races.

```bash
cargo run --release -- reproduce --seed 42 --output-dir results
cargo run --release -- reproduce --quick      # smoke-test scale (smaller clusters / fewer trials / coarse θ grid)
```

| Flag | Default | Description |
|---|---|---|
| `--seed` | 42 | RNG seed base (each trial derives an independent seed) |
| `--quick` | off | smoke-test mode (reduced scale; not for paper-value verification) |
| `--output-dir` | results | output base directory (a `reproduce_{YYYYMMDD_HHMMSS}/` is created beneath it) |

Two claims are reproduced:

- **Claim A — the weak-tie bridge effect** (1973 fact 7 + the central proposition). Baseline reach is `≈1.0`; removing the weak ties (which are *all* of the local bridges) collapses reach to the seed's own cluster (`≈1/K`), whereas removing the *same number* of edges at random leaves reach intact (`≈1.0`, the control). `frac_weak_bridges = 1.0`. PASS requires baseline `≥0.9`, weak-removed `≤1/K + 0.1`, random-removed `≥0.9`, and `frac_weak_bridges = 1.0`.
- **Claim B — threshold-cascade tipping** (Granovetter 1978). A small upward shift in the uniform threshold `θ` jumps the final cascade size from a global cascade (`reach ≈1.0`) to a local one (`reach ≈1/K`); the transition is concentrated in a narrow `θ` band. PASS requires a low-`θ` reach `≥0.9`, a high-`θ` reach `≤0.2`, and a transition width `Δθ ≤ 0.07`.

Outputs (under `results/reproduce_{YYYYMMDD_HHMMSS}/`):

- `claim_a_ablation.csv` — per removal policy (`none` / `weak` / `strong` / `random`): `remove, baseline_reach, removed_reach, delta_reach, frac_weak_bridges`.
- `claim_b_threshold.csv` — per swept `θ`: `theta, reach, cascade_rounds`.
- `reproduce_summary.json` — the network parameters, each claim's params, and the observed-vs-expected comparison with the `PASS`/`OFF` verdict.

The Python wrapper `uv run granovetter-tools reproduce` calls this subcommand and additionally renders comparison figures (`figures/claim_a_weak_tie_bridges.png`, `figures/claim_b_threshold_tipping.png`) into the same `reproduce_{ts}/` directory; see [Visualization](visualization.md).

A representative full run (`--seed 42`): Claim A baseline `1.000` → weak `0.100` (`1/K`), strong `0.006`, random `1.000`, `frac_weak_bridges = 1.000` (PASS); Claim B `reach` `0.990` at `θ=0.07` → `0.151` at `θ=0.10`, `Δθ = 0.030` (PASS).

---
*This file was generated by Claude Code.*
