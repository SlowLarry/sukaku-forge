# Compatibility rating and search modes

Forge keeps rating-table compatibility and search-policy evolution on separate
configuration axes. This prevents a future search improvement from silently
changing either community metric.

## Configuration model

`RatingMode` remains the closed compatibility enum:

- `Original` is the default historical Sukaku Explainer table and producer
  order used as a community metric;
- `Revised` is Java's optional revised table and producer order.

`SearchPolicy` is orthogonal:

- `Compatibility` is the default and freezes the selected Java-compatible
  rating mode;
- `Forge` is the opt-in boundary for future search changes.

At introduction, `Forge` delegates to exactly the same registry as
`Compatibility`. Consequently all four combinations are defined and the two
entries in each row currently have identical traces:

| Rating mode | Compatibility policy | Forge policy (initially) |
| --- | --- | --- |
| Original | frozen legacy behavior | identical to Original compatibility |
| Revised | frozen revised behavior | identical to Revised compatibility |

Future technique, ordering, tie-break or cache-policy experiments must branch
on `SearchPolicy::Forge`. They must not add a `Forge` member to `RatingMode` or
modify an existing `Original`/`Revised` branch. If Forge eventually needs its
own displayed score, that score remains Forge-owned rather than redefining an
SE rating.

## Command line

The canonical policy selections are:

```sh
--search-policy=compatibility
--search-policy=forge
```

`--forge` is the short convenience spelling. The existing `--revised`,
`--revised-rating=1` and Java-compatible `--revisedRating=1` spellings continue
to select only the Revised rating mode. For example, `--forge --revised`
selects the Forge policy with the Revised table; it does not mutate Revised
compatibility behavior. Repeated or conflicting search-policy selections are
rejected.

## Frozen Revised full trace

The non-protected `classic_dynamic_forcing_chain` case is the first complete
Revised-mode trace contract. A single capture matched every one of its 72
canonical `STEP` records and its `RESULT` across the pinned original Java JAR,
the optimized Java build and Rust:

- rating: `8.9/1.5/1.5`;
- whole-trace SHA-256:
  `1d22000f59ad2837570bbc17f1298bdfcd9c79483fc2a97eb099651c8d8570a6`;
- final-state SHA-256:
  `bfde6161d2ed4952149e73e22a4cf8eb786cbdb94aadda7302eb1bcdfcf97be7`;
- final grid:
  `183594762526378149479261583231689457865743291794125836648917325312856974957432618`.

The full-trace digest is distinct from the same puzzle's Original-mode digest,
so this is a mode-sensitive contract rather than another check of the default
path. The normal verification target replays Rust once against the frozen
Java-derived contract:

```sh
python3 scripts/verify-revised-trace.py
```

When the pinned original and optimized Java artifacts are available, the
explicit cross-engine form reruns each engine once and compares every canonical
record directly before checking the frozen contract:

```sh
python3 scripts/verify-revised-trace.py --cross-engine
```

This fixture is short and non-protected. It never selects or runs the protected
11.8 milestone case.
