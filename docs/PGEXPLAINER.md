# PGExplainer benchmark comparator

[PGExplainer](https://github.com/1to9only/PGExplainer) is a small,
rating-oriented extraction of the older Sudoku Explainer/serate codebase. Forge
pins commit `2f356d6cffbe45e1e7525c2df9ff383b861ada2d` and treats it as a historical
performance comparator, not as another behavioral oracle.

## Build

Clone the pinned source next to this repository, check out the exact commit,
and run:

```sh
make build-pgexplainer
```

The build helper compiles with Java 8 bytecode compatibility and creates a
deterministic `target/pgexplainer/PGExplainer.jar`. The benchmark refuses any
artifact whose size or SHA-256 differs from `scripts/pgexplainer.json`. A
modular JDK with the compiler and jar modules is required to build it.

## Benchmark boundary

PGExplainer accepts classic 9x9 puzzles and reports ER/EP/ED, but it does not
support Sukaku Explainer's variant flags or the Forge `STEP`/`RESULT` trace
contract. The harness therefore:

- runs PG only for cases with no variant arguments;
- validates its rating before timing a normal case;
- keeps protected trace verification limited to the three trace-capable
  engines;
- uses one PG copy by default because its hard-chain solves are expensive;
- labels it `pg-upstream-parallel` because the shipped chaining search creates
  worker threads and has no single-thread switch.

Its wall time is not an apples-to-apples single-thread speedup comparison with
the pinned Java, optimized Java, or Forge rows. The protected 11.8 probe made
that especially clear: Forge completed the measured run in 88.051 seconds in
that session, while PG was still running after more than 14 minutes and was
terminated. This result is a reason to defer parallel root-search experiments,
not evidence that parallelism is intrinsically ineffective.

The useful implementation lesson is narrower: PG collects per-cell chaining
work concurrently but publishes results in deterministic cell order. A future
Forge experiment could preserve that ordering with a bounded worker pool, but
must account for Forge's shared inner-chain cache and considerably heavier
nested-search state. The small CLI and rating-only extraction also reinforce
the planned separate, aggressively optimized classic-9x9 headless build.
