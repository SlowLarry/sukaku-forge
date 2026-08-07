# Corrected SE 1.2.1-derived classic rater

`sukaku-forge-rate` is a separate headless product for maximum-throughput
rating of ordinary 9×9 Sudoku. It deliberately does not expose variants,
Revised ratings, hint presentation, all-hints search, sessions, Tauri or WASM.

## Baseline and correctness policy

The source oracle is the pristine `dclamage/SukakuExplainer` tag `v1.2.1.2`:

- commit `a4cdac080393a5a17147ab5794a35ed98a5ef2d2`;
- tree `e48ef86e71237faedc37f80639348b3933dc60b5`;
- application version 1.2.1 and bundled serate source version 1.2.1.3.

There is no trustworthy historical JAR in that source history. The test
oracle is therefore a deterministic build of the pinned tree: the exact
recorded OpenJDK 17.0.19 runtime build, Java 8 bytecode, 90 classes, 163,427
bytes, SHA-256
`6bc0cebb8bf89563d97ee1f7f0525c4fd021cdf0f19a0dc05b55d769f2bb4797`.
Its metadata and a deliberately non-protected baseline corpus live in
`scripts/se121-oracle.json` and `scripts/se121-classic-corpus.json`. The
corpus includes a uniqueness-dependent BUG solve, a 7.2 static forcing chain,
an 8.9 solve beginning with Aligned Pair Exclusion, and 9.3/9.8 Dynamic
Forcing Chain (+) solves; it is not limited to the cheap direct-technique
prefix. Each oracle puzzle runs in a fresh JVM so identity hashes cannot leak
between corpus entries.

A separate slow, still non-protected corpus pins AI Escargot at
`10.5/1.2/1.2`, reaching the original nested-chain tail without making the
ordinary differential gate take several minutes. It is never run by normal
workspace verification.

This is not the later `v1.18.1-rangsk` oracle used by the general Forge
compatibility engine, and it is not PGExplainer. PG replaces the original
level-4/level-5 nested tail with three capped level-4 producers and predates
the 2022 chaining-order changes, so it remains only a performance comparator.

The SE 1.2.1-derived registry contains 30 producers. It excludes Strong
Links, alphabet wings, generalized/variant rules and Revised mode, and retains
exactly one nested producer at each level 2, 3, 4 and 5. The ordinary rater
filters the two uniqueness-dependent producers—Unique Loops and BUG—out of
that registry. Pass `--allow-uniqueness` to restore their original positions
when rating puzzles that are known to have exactly one solution. When the old
solver cannot find a hint, ER becomes 20.0 while EP and ED retain their earlier
values.

Correctness takes precedence over reproducing confirmed defects. The focused
rater currently applies these deliberate corrections:

- a nested advanced family whose deductions are already present no longer
  prevents the solver from trying the next inner family;
- nested advanced removals are enqueued in deterministic row-major cell and
  ascending-digit order, following the corrected ordering in
  [1to9only/SukakuExplainer `22f2520`](https://github.com/1to9only/SukakuExplainer/commit/22f252019b96ed4d735db711d295a3d3d8112e3b);
- when uniqueness is explicitly enabled, Unique Loops and BUG use the later
  fixes from [Unique Loop commit `130f0de`](https://github.com/dclamage/SukakuExplainer/commit/130f0de200215377cc685fd5ba869730dc59f919)
  and [BUG commit `c09b7b5`](https://github.com/dclamage/SukakuExplainer/commit/c09b7b5ca0738192f51278fb71926268bfea3092).

We intentionally retain the cumulative SE 1.2.1 nested schedule. The
[1to9only `10a3252` change](https://github.com/1to9only/SukakuExplainer/commit/10a325298db96295fbc6a3ddb14a297e91f95655)
made inner levels mutually exclusive and warned that ratings may change, but
it does not fix the general stale-result pruning defect and can discard valid
lower-family implications.

SE 1.2.1 used identity-hashed `Cell` objects in one nested-chain removal map.
That makes even nested inference order dependent on the exact JVM and process
history. The baseline gate therefore runs the pinned runtime with one fresh
JVM per puzzle. Corpus entries record the pristine oracle rating and the
corrected expected rating separately when a bug fix intentionally changes
ER/EP/ED. Expanding that corpus—especially for nested levels 2–5—is a release
gate. Deterministic step/effect/state traces remain stronger regression gates
on fixtures where the old oracle is stable.

## Usage

Build the portable maximum-optimization profile:

```sh
make build-rater
```

Or build for the current host CPU:

```sh
make build-rater-native
```

The binary accepts one positional 81-character grid or a batch of nonempty
stdin lines. `.` and `0` are empty cells. Its default output uses serate's
layout:

```text
ER/EP/ED
```

Use `--format='%g ED=%r/%p/%d'` when the input grid should be echoed. Supported
substitutions are `%g`, `%r`, `%p`, `%d` and `%%`.

Unique Loops (including Unique Rectangles) and Bivalue Universal Grave (BUG)
assume that the puzzle has exactly one solution. They are disabled by default:

```sh
sukaku-forge-rate --allow-uniqueness PUZZLE81
```

The flag restores the two families' positions in the old schedule, but uses
their corrected implementations rather than knowingly restoring the old
bugs. Other contradiction and forcing-chain techniques do not rely on a
uniqueness assumption.

The supported input contract covers unsolved 81-character value grids. The old
serate formatter emitted a Java numeric-conversion artifact for an already
solved grid; this focused product normalizes that non-rating case to
`0.0/0.0/0.0`.

Build and run the source-oracle gate with a checkout that contains the pinned
commit:

```sh
make build-se121-oracle SE121_SOURCE=../SukakuExplainer
make verify-se121-rater
make verify-se121-rater-slow
```

The verifier passes `--allow-uniqueness` because the pristine Java registry
always enabled those families. It validates Java against
`expected_oracle_rating` (or `expected_rating` when identical) and corrected
Rust against `expected_rating`, making every intentional rating divergence
explicit. The oracle build refuses a different source tree, archive, JDK,
class count, JAR size or JAR hash. It is not part of normal `make verify`, so
contributors do not silently depend on a sibling Java checkout.

## Optimization policy

The dedicated path already reuses one immutable topology per batch, freezes a
static producer array instead of allocating/gating the general registry each
step, passes the top-level grid directly into the rating loop, and tracks only
three numeric ratings without allocating technique names. Hidden/set family
traversal and Locking family pairs use static tables and lazy stack-only
iterators rather than allocating order vectors on every probe.

Further work may replace the general grid/topology, inference payloads,
implication construction, workspaces and caches freely. Each change remains
behind the corrected corpus plus its pinned SE 1.2.1 baselines and is measured
independently in portable and `target-cpu=native` builds. Multithreaded root
search remains deferred.

The protected 11.8 puzzle is never part of ordinary tests or benchmarks. It
retains the existing explicit major-milestone, one-run, one-copy gate.
