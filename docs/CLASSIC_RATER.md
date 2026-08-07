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

This historical source oracle is not the
[updated SudokuMonster v1.18.1 release](https://github.com/SudokuMonster/SukakuExplainer)
used as the Java performance comparator, and it is not PGExplainer. PG replaces
the original level-4/level-5 nested tail with three capped level-4 producers
and predates the 2022 chaining-order changes, so it remains only a performance
comparator.

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

GUI version tags (`v<version>`) attach the portable rater builds to the same
[GitHub Release](https://github.com/SlowLarry/sukaku-forge/releases) as the
desktop packages. Rater-only milestones use `rater-v<version>` and publish the
same portable archives without rebuilding a functionally unchanged GUI:

- `sukaku-forge-rate-<version>-x86_64-pc-windows-msvc.zip`;
- `sukaku-forge-rate-<version>-x86_64-unknown-linux-musl.tar.gz`, containing a
  statically linked Linux x64 binary;
- `sukaku-forge-rate-<version>-SHA256SUMS.txt`, covering both archives.

Each archive includes the executable, `LICENSE` and this usage document.

Build the portable maximum-optimization profile:

```sh
make build-rater
```

Or build for the current host CPU:

```sh
make build-rater-native
```

The native target sets `target-cpu=native` and may use instructions available
only on the build host. It is intended for local rating and benchmarks, never
for release distribution.

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

## Performance benchmark

Performance comparisons use the exact released SudokuMonster v1.18.1 JAR,
not the historical SE 1.2.1 correctness oracle. Fetch and verify that artifact
before running the guarded benchmark harness:

```sh
make fetch-sudokumonster-v118
python3 scripts/benchmark-classic-rater.py \
  --post-rater target/rater-native/rater/sukaku-forge-rate \
  --sudokumonster-v118-jar
```

The pin records tag `v1.18.1`, commit
`362854eea4e983017726d406ac9ee8a28909bcc7`, released-JAR size and SHA-256,
and the Java runtime fingerprint. The harness starts fresh processes, pins
both programs to one logical CPU when `taskset` is available, and explicitly
passes Java `--threads=1`. Java is an unfrozen comparator because its updated
technique schedule can legitimately rate a puzzle differently from the
corrected SE 1.2.1-derived headless product.

The retained 0.5.0 one-shot on the guarded 11.8 case measured 33.57 seconds
for the native rater and 761.22 seconds for SudokuMonster v1.18.1. Both emitted
`11.8/1.2/1.2`, giving a completed `22.7×` ratio for that single case. Each
engine received one fresh process without warm-up on the same pinned Xeon
8370C CPU; this result is not presented as a corpus average.

### 0.6.0 headless optimization measurements

The Java comparison above remains the external SudokuMonster v1.18.1
benchmark. The optimized 0.6.0 code candidate used a preserved native 0.5.0
Rust binary as its baseline, so these numbers measure changes within the
focused headless rater and must not be read as new Rust-versus-Java results.
The isolated and protected candidate artifact was built immediately before the
metadata bump and therefore still identified itself as 0.5.0. The protected
11.8 case was not rerun after that metadata-only change; the hardest-ten batch
was repeated with the final self-identifying 0.6.0 binary.

Four changes were evaluated while preserving the SE ordering and exact grid
state semantics:

- Chain searches now defer `Inference` payload construction and candidate
  removal allocation until the final winning candidate is known. This removed
  912 allocations on the 10.5 case; its measured runtime was neutral.
- The rating session shares exact-state negative forcing-chain cache entries
  and reusable scratch capacity across solver steps. The focused measurement
  improved from 3.559 to 2.644 seconds (`25.7%`) while retaining about 0.57 MiB
  of negative-cache key payload at the observed capacity; hash-table metadata
  and reusable branch arenas are additional memory.
- Nested scans retain a separate removal-event cursor for each inner family
  and revisit only affected houses and digits, without changing Java's family
  or traversal order. This improved the matched measurement from 2.617 to
  2.528 seconds (`3.4%`).
- A fixed 9×9 Classic candidate cache is the safe first phase of a dedicated
  grid representation. It measured about `0.3%` faster on the 9.8 case and
  `1.6%` faster on the 10.5 case.

The combined native build rated the first (hardest) ten entries of the
retained corpus in one reusable process. The preserved 0.5.0 binary took
121.453 seconds and the final 0.6.0 binary took 102.688 seconds in one
same-core batch per binary, a `15.4%` reduction; all ten ER/EP/ED results were
identical between the two builds. The pre-bump protected 11.8 one-shot
moved from 32.891 to 28.949 seconds (`12.0%`) and both builds emitted
`11.8/1.2/1.2`. These are matched Rust-to-Rust measurements, separate from the
retained 33.57-versus-761.22-second Java comparison above.

### Retained hard-puzzle corpus

`scripts/hard-puzzle-corpus.json` is an ordered snapshot of the public
[hard-puzzle Google Sheet](https://docs.google.com/spreadsheets/d/1t-PsJT-pKGQEWjSbbNBXzLcxb5Inmooszntu9ZVCW_M/edit?gid=0#gid=0).
It retains all 972 unique `expanded-minlex` strings and their published
ER/EP/ED ratings, but none of the sheet's dates, solutions or auxiliary
columns. Source and normalized-case SHA-256 digests make accidental corpus
changes visible. The sheet does not state a data license; the snapshot remains
explicitly attributed third-party benchmark data, and the project's LGPL
license should not be read as licensing that corpus.

Rate an ordered slice in one reusable process and optionally retain a JSON
report:

```sh
python3 scripts/benchmark-hard-corpus.py \
  --rater target/rater-native/rater/sukaku-forge-rate \
  --start 1 --limit 10 \
  --json-out target/benchmarks/hard-corpus-first10.json
```

Published ratings are comparison data rather than a pass/fail oracle: the
corrected rater, its default-off uniqueness techniques, and later source
versions can intentionally produce different ratings.

## Optimization policy

The dedicated path reuses one immutable topology per batch, freezes a static
producer array instead of allocating/gating the general registry each step,
passes the top-level grid directly into the rating loop, and tracks only three
numeric ratings without allocating technique names. Hidden/set family
traversal and Locking family pairs use static tables and lazy stack-only
iterators rather than allocating order vectors on every probe. The 0.6.0
changes add winner-only chain payload materialization, rating-session scratch
and exact-negative cache reuse, delta-aware nested scans, and the first fixed
Classic grid cache described above.

Further work may deepen the dedicated Classic grid representation and replace
more implication construction, workspaces and caches. Each change remains
behind the corrected corpus plus its pinned SE 1.2.1 baselines and is measured
independently in portable and `target-cpu=native` builds. Multithreaded root
search remains deferred.

The protected 11.8 puzzle is never part of ordinary tests or benchmarks. It
retains the existing explicit major-milestone, one-run, one-copy gate.
