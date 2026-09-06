**English** | [日本語](visualization.ja.md)

# Visualization

The Python package `granovetter-tools` (a uv workspace member) reads runvault run directories and produces figures. Which run is resolved by `runvault path` rather than by scanning `results/`, so the `runvault` command has to be on PATH. Install once with `uv sync` at the workspace root.

Figures are written **outside** the run directory, into `{results_root}/granovetter/figures/{run_slug}/`. `manifest.csv` is settled by `finish()`, so a file added after the run ended would contradict the record.

```bash
uv sync
uv run granovetter-tools visualize
uv run granovetter-tools visualize-sweep
uv run granovetter-tools show-experiment-settings
uv run granovetter-tools reproduce
```

The CLI dispatches to one of four subcommands via argparse; arguments after the subcommand are passed straight to the corresponding module.

## `visualize` — network layout & metrics

Reads `artifacts/edges.csv`, `artifacts/nodes.csv` and `events.jsonl` (the per-trial `terminal` lines) from a `run` / `ablation` run and writes:

- `network_layout.png` — a NetworkX spring layout of the network: nodes colored by cluster (the diffusion seed highlighted in yellow), **strong ties** drawn as thin grey solid lines and **weak ties** as red dashed lines. The weak-tie bridges visibly hold the clusters together; removing them (an ablation result) leaves the clusters disconnected.
- `metrics_summary.png` — three per-trial bar panels: weak-bridge fraction (should sit at 1.0), reach fraction, and forbidden-triad rate, each with the trial mean.

```bash
uv run granovetter-tools visualize                       # the latest run
uv run granovetter-tools visualize --subcommand ablation # the latest ablation
```

| Flag | Default | Description |
|---|---|---|
| `--results_dir` | (resolved by runvault) | the run directory |
| `--results_root` | results | the results root |
| `--experiment` | granovetter | the runvault experiment name |
| `--subcommand` | run | which subcommand to take (`run` / `ablation`) |
| `--output_dir` | `figures/{run_slug}` | figure output directory |

## `visualize-sweep` — reach vs parameters

Rebuilds the per-trial rows from a sweep parent's children (`runvault.read.sweep_events_table`) and writes:

- `sweep_reach.png` — reach fraction vs `p_bridge` (one curve per `θ` for the threshold model; trial mean ± std error bars). Reach rises from `≈ 1/K` at `p_bridge → 0` toward full network reach — the tipping-like dependence.
- `sweep_structure.png` — the weak-bridge fraction (should stay `≈ 1.0`, confirming Fact 7) and the forbidden-triad rate vs `p_bridge`.

```bash
uv run granovetter-tools visualize-sweep
```

| Flag | Default | Description |
|---|---|---|
| `--sweep_dir` | (resolved by runvault) | the sweep parent's run directory |
| `--results_root` | results | the results root |
| `--experiment` | granovetter | the runvault experiment name |
| `--output_dir` | `figures/{run_slug}` | figure output directory |

## `show-experiment-settings`

Pretty-prints a run's `config.json` (the envelope; the conditions sit under `parameters`). Whether it is a `run` / `ablation` or a sweep parent is read from `run.json`'s `subcommand`. Layouts that predate runvault (a flat `config.json`, a `sweep_config.json`) are read too. Use `--json` for machine-readable output.

```bash
uv run granovetter-tools show-experiment-settings
uv run granovetter-tools show-experiment-settings --subcommand sweep
uv run granovetter-tools show-experiment-settings --json
```

## `reproduce` — one-shot paper reproduction

Calls the Rust `reproduce` subcommand (see [CLI](cli.md)) once, then reads the `events.jsonl` and `artifacts/reproduce_summary.json` of the run `runvault path` returns, and renders comparison figures outside the run:

- `claim_a_weak_tie_bridges.png` — reach by removal policy (`none` / `weak` / `strong` / `random`) as bars, with the `1/K` local-reach reference line and the PASS/off verdict annotated. The weak bar collapses to `1/K` while the random (control) bar stays at full reach.
- `claim_b_threshold_tipping.png` — reach vs threshold `θ` as a curve, with the tipping band shaded and the PASS/off verdict annotated. A small `θ` shift drops reach from a global cascade to a local one.

```bash
uv run granovetter-tools reproduce              # full paper-value reproduction
uv run granovetter-tools reproduce --quick      # smoke-test scale
uv run granovetter-tools reproduce --seed 123
uv run granovetter-tools reproduce --skip-build # if cargo build was already run
```

| Flag | Default | Description |
|---|---|---|
| `--output-dir` | results | results root (a `granovetter/{run_slug}/` is created beneath it) |
| `--seed` | 42 | RNG seed base |
| `--quick` | off | smoke-test mode (reduced scale; not for paper-value verification) |
| `--skip-build` | off | skip `cargo build --release` |
| `--workspace-root` | inferred | workspace root override (else inferred from the module location, or `GRANOVETTER_PROJECT_ROOT`) |

The process exits non-zero if any claim's verdict is not `PASS`.

## Note on fonts

The scripts set `font.family = "Hiragino Sans"` for Japanese labels (macOS). On other platforms, substitute an installed CJK font in the `plt.rcParams` line at the top of `visualize.py` / `visualize_sweep.py`.
