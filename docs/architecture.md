**English** | [日本語](architecture.ja.md)

# Architecture

## Repository structure

A two-project layout: a Cargo workspace + a uv workspace.

```
granovetter1973/
├── Cargo.toml                 # Cargo workspace root
├── pyproject.toml             # uv workspace root
├── simulation/                # Rust project (granovetter-simulation, lib granovetter_ties)
│   ├── Cargo.toml
│   ├── src/
│   │   ├── main.rs            # CLI (run / ablation / sweep / reproduce)
│   │   ├── lib.rs             # module re-exports for the binary + integration tests
│   │   ├── config.rs          # Config + the experimental condition (runvault parameters), DiffusionModel / RemovePolicy
│   │   ├── world.rs           # socsim WorldState impl (WeakTieWorld); TieStrength edge weight
│   │   ├── network.rs         # clustered weak-tie bridging network generator
│   │   ├── metrics.rs         # bridges / forbidden triad / reach / path length / reach-by-strength
│   │   ├── record.rs          # how a run is recorded to runvault (research metadata, events, aggregates)
│   │   ├── reproduce.rs       # one-shot reproduction of the headline claims (observed vs expected)
│   │   └── simulation.rs      # init_world + ablation + run driver (SimulationBuilder wiring)
│   └── tests/
│       └── integration_test.rs
├── tools/                     # Python project (granovetter-tools)
│   ├── pyproject.toml
│   └── src/granovetter_tools/
│       ├── cli.py                       # unified CLI (granovetter-tools)
│       ├── visualize.py                 # network layout (colored by tie strength) + metrics
│       ├── visualize_sweep.py           # reach vs p_bridge + structural-proposition check
│       └── show_experiment_settings.py  # display run / ablation / sweep settings
└── results/                   # simulation output (gitignored)
```

- `cargo run` launches the `simulation` crate from the workspace root.
- `uv run` invokes the `granovetter-tools` command exposed by the `tools` member of the uv workspace.

## Model on the socsim framework

The engine is built on [rs-social-simulation-tools](https://github.com/akitenkrad/rs-social-simulation-tools) (socsim) — a git dependency, commit pinned in `Cargo.lock`. This is a **pure network model with no spatial grid**, so it uses `socsim-core` (traits), `socsim-engine` (Simulation / Builder), and `socsim-net` (the network layer) — **no `socsim-grid`**.

### Weighted edges instead of a side-table (deviation from the design doc)

The design doc (§4.3) predates a socsim upgrade and proposed holding tie strength in a `BTreeMap<(AgentId, AgentId), TieStrength>` side-table because the old `SocialNetwork` was `UnGraph<AgentId, ()>` (no edge payload). That workaround is now obsolete: socsim-net on `main` provides a generic weighted network, so **the edge weight *is* the tie strength**:

- `WeightedNetwork<TieStrength>` (= `Network<TieStrength, Undirected>`) — `add_edge_weighted(a, b, TieStrength)`, `edge_weight(a, b) -> Option<&TieStrength>`, `weighted_edges() -> (a, b, &TieStrength)`, `remove_edge(a, b)`.
- Analysis helpers (socsim issue #20): `local_bridges()` / `is_local_bridge(a, b)`, `average_path_length()`, `component_membership()` / `largest_component_size()`, `connected_components()`, `edge_count()`.
- Hot-loop neighbour access: `neighbors_iter` (zero-allocation).

`WeakTieWorld` therefore has **no tie side-table**; it holds the `WeightedNetwork<TieStrength>`, a `cluster_of` map, the `informed` diffusion state, the diffusion `seeds`, and an `n_informed_history`. It is `#[derive(Clone)]` for snapshots and for the ablation (which clones the world, removes edges, and re-runs).

The socsim APIs used: `WorldState` (`agent_ids` sorted / `clock` / `clock_mut`), `Mechanism` + `Phase::Interaction`, `RandomActivationScheduler`, `StepContext::request_stop` / `Simulation::run_observed` / `StepContext::scratch`, `SimRng` / `derive_seed`.

## The weak-tie bridging network generator (`network.rs`)

The paper's structure is operationalized as clustered communities with sparse weak bridges:

1. **Strong-tie clusters.** `K` clusters of `cluster_size` agents each. Within a cluster: a strong-tie ring (guarantees connectivity) + Erdős–Rényi strong edges at probability `p_strong` (a density knob).
2. **Triangle closure.** A closure pass adds strong edges so that **every intra-cluster strong tie lies in a triangle** (shares a neighbour). This makes strong ties clique-ish, so no intra-cluster edge is a local bridge — exactly the design's "strong ties cannot be bridges". As a consequence **every local bridge is an inter-cluster weak tie** and `frac_weak_bridges == 1.0` holds robustly under the default generator.
3. **Weak-tie bridges.** For each pair of clusters, with probability `p_bridge`, add one weak tie between two uniformly chosen members.
4. Optionally a few intra-cluster weak ties (`p_weak_intra`, default 0.0).

## The diffusion mechanism (synchronous rounds)

`DiffusionMechanism` fires in `Phase::Interaction` and updates **synchronously**: it snapshots the informed set at the start of the round, computes new activations from that snapshot, then applies them all at once (so a node informed mid-round does not infect until the next round). This makes the round count a proxy for path length (the paper's "social distance = shortest path").

- **SI** (`--diffusion si`): an uninformed node becomes informed if any informed neighbour infects it with probability `beta` (drawn from `ctx.rng`).
- **Threshold** (`--diffusion threshold`): an uninformed node activates when its fraction of informed neighbours `≥ theta` (Granovetter 1978 threshold cascade).

Convergence: `request_stop()` on cascade saturation (0 new informed) or full reach. The `RandomActivationScheduler` shuffles the activation order each round; with synchronous updates the order does not change the result, but it is kept for an event-driven extension.

## RNG streams

A single root seed is split into independent, labelled streams (socsim convention): `derive_seed(root, &[0])` = world init (network generation + seed selection), `derive_seed(root, &[1])` = engine / scheduler (= SI infection draws), `derive_seed(root, &[2])` = ablation random-removal. Each `sweep` / multi-run trial derives its own root via `derive_seed(seed, &[...])`, so trials are reproducible and uncorrelated.

## Metrics

| Metric | Definition | Paper correspondence |
|---|---|---|
| `frac_weak_bridges` | fraction of `local_bridges()` whose edge weight is `Weak` | Fact 7 "all bridges are weak ties" |
| `n_local_bridges` | number of local bridges (`d > 2` if removed) | §4.3 local-bridge proposition |
| `forbidden_triad_rate` | fraction of strong 2-paths (A–B, A–C strong) whose B–C edge is absent | Fact 6 (Davis) |
| `reach_fraction` | informed fraction at saturation | Fact 2, "more people reached through weak ties" |
| `avg_path_length` | mean shortest path (`average_path_length()`) | Fact 3 chain length |
| `reach_by_strength` | structural reach traversing only Strong vs only Weak edges from the seed | Rapoport–Horvath |
| `largest_component_fraction` | largest connected component size / n | network reach / fragmentation |
| `cascade_rounds` | rounds to saturation | path-length proxy |

`reach_by_strength` uses socsim-net's `reachable_from(seed, |w| *w == strength)` (a BFS over the edge-weight-filtered subgraph). Since `reachable_from` includes the seed itself, the seed set is subtracted to keep the established "reached count excludes the seed" definition. All structural metrics now use socsim-net helpers directly.

## Reproducibility & determinism

For a given seed the whole pipeline (network generation, ablation, diffusion) is deterministic — the integration test asserts identical reach and history across two runs with the same seed.

## Paper reproduction (`reproduce`)

`simulation/src/reproduce.rs` drives a one-shot reproduction of the paper's headline quantitative claims directly through the library API (no subprocess): **Claim A** (the weak-tie bridge effect — removing the weak ties, which are all of the local bridges, collapses reach to `1/K`, while removing the same number of random edges does not) and **Claim B** (Granovetter 1978 threshold-cascade tipping — a small upward shift in the threshold jumps the final cascade size from global to local). It averages reach over seeded trials and writes the per-removal-policy and per-`θ` observations to `events.jsonl`, both claims' headline numbers to `metrics.csv`, and each claim's parameters plus the observed-vs-expected comparison with a `PASS`/`OFF` verdict to `artifacts/reproduce_summary.json`. The Python `granovetter-tools reproduce` calls this subcommand and renders the comparison figures. See the [CLI](cli.md) and [Visualization](visualization.md) docs.

## References

- Granovetter, M. S. (1973). The Strength of Weak Ties. *American Journal of Sociology*, 78(6), 1360–1380. DOI: 10.1086/225469.
- Granovetter, M. S. (1978). Threshold Models of Collective Behavior. *American Journal of Sociology*, 83(6), 1420–1443. (formulation basis for the threshold cascade)
- Centola, D., & Macy, M. (2007). Complex Contagions and the Weakness of Long Ties. *AJS*, 113(3), 702–734.
- Davis, J. A. (1970). Clustering and Hierarchy in Interpersonal Relations. *ASR*, 35(5), 843–851. (empirical support for the forbidden triad)

---
*This file was generated by Claude Code.*
