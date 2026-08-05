# Architecture and compatibility plan

## Compatibility boundary

The optimized Java build remains the reference until Rust reaches 1.0. Exact
behavior includes more than the final rating:

- producer order and the first accepted hint;
- cell, digit, region and peer iteration order;
- every post-step value grid and 729-character candidate grid;
- ER, EP and ED updates;
- explanation identity and parent order for chaining;
- variant-specific weak links, including anti-knight.

The Rust implementation uses explicit-width primitive identities and arrays.
It does not port Java `Cell`, `Grid.Region`, `BitSet` or `Potential` objects.

Rating and search evolution are separate axes. `RatingMode::{Original,
Revised}` remains closed compatibility data. `SearchPolicy::Compatibility` is
the default frozen Java registry, while `SearchPolicy::Forge` is an opt-in
future-policy boundary that initially delegates to the same registry. See
[`COMPATIBILITY_MODES.md`](COMPATIBILITY_MODES.md) for the complete contract and
the frozen full Revised trace.

## Crate boundaries

```text
sukaku-forge-core
  CandidateMask, PositionMask, CellMask
  Puzzle, Grid, ConstraintTopology, VariantConfig

sukaku-forge-engine
  compact Inference effects and independent Evidence
  explicit ProducerSpec registry, Technique, Rating/ER/EP/ED
  ordered static and dynamic implication kernels and chaining

sukaku-forge
  CLI parsing, stdin/stdout, serate-compatible formatting

sukaku-forge-presentation
  ordered semantic hint views, typed explanations and primitive wire DTOs

sukaku-forge-app
  authoritative session, revisions, retained hints and exact undo/redo

apps/gui
  React/TypeScript chrome and layered SVG renderer
```

Inference effects and explanation evidence remain separate. In particular,
Java's direct Hidden Set and direct Locking hints display candidate removals but
apply only their derived placement. Rust therefore stores an empty application
payload plus tuple/intersection evidence instead of conflating red UI marks
with mutations. Presentation may attach richer text or HTML afterward.

## Versioning

- `0.1.x`: topology/state kernel and ordered direct-technique layer;
- subsequent `0.x`: technique families and oracle coverage;
- `1.0.0`: full supported Java rating/trace parity;
- performance work is accepted only after the relevant exact oracle passes.

## Ordered topology contract

Cell index is `row * 9 + column`. Base visibility scans cells 0 through 80 for
row, column and optional block peers. Additional peers are appended, retaining
the first insertion, in this exact order:

1. window;
2. disjoint group;
3. diagonals;
4. center dot;
5. Girandola;
6. asterisk;
7. anti-Ferz;
8. anti-knight.

Forward peers filter that ordered list by `peer > source`; they are not sorted
again. Chess-only peers contain only chess edges newly added to general
visibility. Historical toroidal and non-consecutive behavior is compatibility
data, not an opportunity for geometric cleanup.

## Ordered producer and port-gap contract

Producer order lives in an explicit registry and is not derived from technique
enum order or numeric rating. The original Java rating table searches:

1. Hidden Single;
2. Direct Pointing/Claiming when blocks are active;
3. Direct Hidden Pair;
4. Naked Single;
5. applicable non-consecutive direct producers;
6. Direct Hidden Triplet.

The revised table moves Naked Single and non-consecutive producers before
locking, followed by the pair and triplet. For compatibility, revised Direct
Hidden Triplet uses the Direct Hidden Pair enable gate, matching the Java
implementation.

All four non-consecutive slots are ported. Modes 1 and 2 use orthogonal
forcing-cell and locked-NC producers; modes 3 and 4 use the corresponding Ferz
geometry. The ports preserve Java's 2.4/2.5 ratings, candidate-cell and
digit/region search order, cyclic endpoint display order, visibility filters,
and the historical toroidal Ferz forcing-cell Wazir-table quirk. Complete
Java-derived paths pin 29 orthogonal hints and 19 cyclic Ferz hints plus their
final candidate states.

After all direct producers, ordinary Pointing/Claiming precedes Generalized
Intersections. Java's default technique profile is topology-sensitive: classic
and Latin configurations enable ordinary Pointing/Claiming and disable
Generalized Intersections, while added region/chess variants do the reverse.
This selection is distinct from whether a producer is compiled or ported.

The next original-rating slots are Naked Pair, Generalized Naked Pair, X-Wing
and Hidden Pair. Revised rating moves Hidden Pair before the two pair producers
and X-Wing. Classic and Latin defaults select the ordinary pair; added
region/chess variants select the generalized pair, whose eliminations use the
complete topology peer relation. Pair and digit combinations retain Java's
increasing numeric bit-mask order rather than being reordered into conventional
lexicographic combinations.

The degree-three tier is also explicit. Original rating searches Naked
Triplet, Generalized Naked Triplet, Swordfish and Hidden Triplet before the
StrongLinks(2) Turbot Fish gate, followed by XY-Wing and XYZ-Wing. Revised
rating searches the two triplets and Hidden Triplet, then the legacy Turbot
Fish producer, and only then Swordfish and the two wings. The legacy Turbot
Fish is ported independently because its region-pair order, grouped geometry,
ring handling, names and ratings are observably different from
`StrongLinks(2)`. Both wing modes are ported with Java's ordered peer scan and
full variant visibility. Unique Rectangle/Loop types 1 through 4 follow them,
using Java's family-labelled DFS, region-parity validity rule, variant deadly-
pattern restrictions and raw `double` ranking. The complete degree-four tier
is now ported: ordinary Naked Quad for classic/Latin profiles, generalized
Naked Quad for added-region or chess variants, then Jellyfish and Hidden Quad
in exact Java order even though their revised ratings are 5.4 and 5.2
respectively. The ordinary/generalized default switch therefore applies
consistently to pairs, triplets and quads. StrongLinks(3) follows in both Java
rating modes with the same original catalog and 5.4–5.7 rating table. The
registered Generalized Naked Quintuplet after it is disabled by both default
profiles. WXYZ-Wing through TUVWXYZ-Wing are now implemented by one
degree-parameterized ALS-wing engine while retaining each Java family's search
limits, ranking, suffix, rating and presentation contract. WXYZ-Wing is the
next active producer and is therefore covered by the complete ordered-registry
replay. Bivalue Universal Grave follows it with exact Type 1 through Type 4
classification, the default and legacy lkSudoku branches, and conservative
chess/non-consecutive deadly-pattern rejection. Its unsuccessful validation
uses primitive candidate masks rather than cloning Java's temporary `Grid`.
StrongLinks(4) follows BUG with the same released catalog, grouped-link and
ring semantics in both rating modes. Its fixed degree-four search retains
Java's family-multiset, QuickPerm direction, global ranking, suffix and
5.8–6.1 rating rules. Aligned Pair Exclusion then preserves Java's raw-cell
pair order, topology-ordered common excluders, ascending value combinations,
and fixed 6.2 rating. It replaces Java's reusable 81-by-81 excluder table with
two cell bitboards and compact pair-local scans. Forcing Chains & Cycles follows
with Java's X-only, Y-only and mixed static implication searches, ordering,
length-based ratings and presentation aliases. In particular, the committed
anti-knight trace's step 36 is the chain producer's 6.6 `Turbot Fish` alias,
not the earlier producer with the same display name. Original and Revised use
the same chaining behavior here. The ordinary rating path retains only compact
inference evidence. A separate presentation search can rerun only the selected
winning root and materialize its ordered ancestor path as a typed,
transport-neutral proof graph. Causes are normalized onto parent edges,
including reversed cycle views, without making the hot rating path retain
losing arenas. TUVWXYZ-Wing follows, so the next
enabled slot is Aligned Triplet Exclusion. Its port preserves Java's raw-cell
Twomutations base-pair order, ordered twin-area tail selection, first-base peer
order for common excluders, descending mixed-radix value enumeration and fixed
7.5 rating. Per-cell excluders and twin/common intersections use compact cell
masks instead of Java's reusable 81-by-81 byte table. Nishio Forcing Chains
follow with Java's assumption-driven dynamic contradiction propagation,
length-based 7.5+ ratings, global hint ranking and source-candidate effect.
Original and Revised share this producer behavior. Multiple Forcing Chains
then intersect Java-ordered candidate branches for cell and region reductions
over the static X/Y implication graph. Level-0 Dynamic Forcing Chains add live
candidate removal and rollback, hidden-parent ancestry, binary contradiction
and double reductions, and dynamic cell and region intersections. Both ports
preserve Java's ON-before-OFF frontier priority, first-path retention, raw
`double` difficulty comparison, complexity tie-breaking and final sort keys.
Dynamic Forcing Chains (+) adds the ordered fixed-point schedule of Locking or
Generalized Intersections, Hidden Pair, ordinary or generalized Naked Pair, and
X-Wing. `FCPlus=1` then appends Java's inert Turbot-Fish gate and the complete
XY-/XYZ-Wing families. `FCPlus=2` further appends Hidden/Naked Triplets,
Swordfish, the inert StrongLinks(3) gate where applicable, and the WXYZ/VWXYZ
families in Java order. Released Java's remaining classic tail casts Aligned
Exclusion, Unique Loop and BUG hints to a parent-provider interface they do not
implement. Compatibility mode therefore returns a typed
`LegacyFcPlusBoundary`/`Incomplete` result at the exact first productive family
rather than silently continuing to a non-Java rating; a future Forge policy may
define repaired parent semantics separately. Nested level 2 appends the complete ranked static Forcing Chains result
set, level 3 appends Multiple Forcing Chains, and level 4 embeds a capped dynamic
producer. Original mode tries caps 0, 1, 2 and 3; Revised deliberately stops at
cap 2, matching Java's registry rather than normalizing the two modes.

An inner chaining producer must rank all of its results before effect-only
deduplication, because a simpler parentless proof can suppress a later
parent-dependent duplicate. Forge performs the equivalent operation online:
each effect retains only its strictly best Java-ranked proof and its true
discovery ordinal. A retained nested proof is eagerly compacted to its ordered
`FullChain` fingerprint, recursive complexity and state-dependent parent-cause
events, so losing hints cannot pin whole implication arenas. This preserves
Java's parent order and recursive complexity while bounding the level-4 memory
footprint. Static implication nodes also retain causes on weak `off` edges:
Java's bidirectional-cycle reversal can turn such a node `on`, where its cause
becomes an observable parent of a containing nested chain. Compact chaining
removals likewise retain the legacy default-capacity `HashMap` bucket order
when exposed to an advanced outer chain. That iteration order can decide which
simultaneous contradiction is observed first, so compact storage must not
silently replace it with a size-tuned map order.

The remaining alphabet wings occur later in the Java registry: VWXYZ-Wing
follows StrongLinks(4), UVWXYZ-Wing follows Aligned Pair Exclusion, and
TUVWXYZ-Wing follows Forcing Chains & Cycles in the chaining group. All are
now reachable in the complete ordered registry. VWXYZ-Wing and UVWXYZ-Wing
have selected committed-trace occurrences; TUVWXYZ-Wing is executed by the
complete-trace check, but retains direct Java-derived fixtures because neither
committed trace selects it.

An enabled unported producer returns `Incomplete` immediately. Skipping that
slot could select a later inference Java would never reach, so an incomplete
port never emits a canonical `RESULT` or a synthetic “Beyond solver” rating.

## Differential state replay

An 81-character value grid and its 729-character candidate grid are both
required to restore a mid-trace state. A singleton in the candidate text may be
either a solved display slot or an unresolved Naked Single. Snapshot loading
therefore keeps values, clears internal candidates only for solved cells,
preserves every unresolved mask, and rebuilds region caches without performing
initial candidate pruning.

The verifier feeds these states back through the complete currently ported
registry and compares the next rating, description, value grid and candidate
grid with the Java oracle. This currently covers 184 snapshots, including
WXYZ-Wing, two VWXYZ-Wing occurrences, UVWXYZ-Wing, StrongLinks(4), and the
anti-knight trace's selected Forcing Chains & Cycles and Aligned Triplet
Exclusion occurrences, all five selected Nishio snapshots, and later states
reached after the chaining milestones. The seven selected Multiple/Dynamic
Forcing Chain occurrences are classic steps 8–11 and anti-knight steps 42–44.
Revised-mode Nishio and all
seven Multiple/Dynamic chain replays independently verify the same ratings,
descriptions and post-application states. TUVWXYZ-Wing uses direct fixtures
for positive-result coverage because the committed traces only exercise its
empty path.

The former maximal-prefix check is now a full solve-trace check. It compares
all 72 classic steps and the 8.9/1.5/1.5 `RESULT`, plus all 114 anti-knight
steps and the 8.3/1.2/1.2 `RESULT`, including each result technique identity.
Separate compact trace contracts pin the complete normal 9.6, 9.8, 10.4 and
10.5 benchmark paths by rating, step count, final state and full trace digest.
Focused direct fixtures pin DFC+ and nested levels 2 through 4, including the
public non-protected level-3 11.0/complexity-116 and level-4
11.7/complexity-207 hints. The protected community 11.8 benchmark remains
double-gated. Its first explicitly authorized single-copy run produced
`11.8/1.2/1.2` in pinned Java, optimized Java and Rust, at 422.367s, 132.326s
and 69.719s respectively.

Protected full-trace verification is isolated in
`scripts/verify-protected-trace.py` and is never called by the normal
verification target. Its command line must contain exactly one selection of
`user_extreme_major_milestone_probe` plus `--allow-major-milestone`; duplicate
or mixed selections are rejected. Before solving it checks the original JAR
against the frozen oracle SHA-256. It then launches one compact trace process
per engine in the fixed original-Java, optimized-Java, Rust order, with no
retries, parallelism or temporary trace files. The version-1 parser compares
the canonical records directly as well as `RESULT`, rating, final grid, final
state digest and whole-trace digest. The first authorized run matched all 133
records across the three engines and froze whole-trace digest
`594d23c1329c5ad8c9a884a093a72d2b7c1d981936137ee050f034d01d4c2ef7`;
the verifier enforces that contract on future explicitly authorized runs.

APE has no selected occurrence in the committed solve traces, so its producer
is additionally checked against all 272 available paired value/candidate
states in both rating modes. The 544 Java/Rust comparisons match exactly for
presence, rating, names, description and post-application state. Restoring
both halves of the snapshot is essential: candidates-only import promotes
some singleton pencilmarks heuristically and is not the same mid-trace state.
The same distinction matters for ATE: candidates-only reconstruction produces
four spurious anti-knight hits at post-steps 79 through 82, while exact paired
state restoration correctly returns no ATE inference there. A separate
272-state-by-two-rating-mode differential compares the direct Java and Rust
ATE producers on exact paired states: all 544 searches match in presence,
identity, application state, invalid-combination order, duplicate markers and
first locking cells (106 hits per mode).

## Optimization policy during the port

The port uses clear local improvements when they naturally replace legacy
allocation-heavy structures, but systematic profiling and tuning are deferred
until 1.0 parity. Current examples are sparse typed removal payloads instead of
`HashMap<Cell, BitSet>`, two-word peer intersections instead of copied cell
sets, cached region-position masks, and an incrementally maintained cell mask
for each candidate digit. Generalized sets intersect those masks directly, fish
evidence uses a fixed-capacity ordered cell sequence instead of a heap-allocated
object collection, and set/fish combinations use a direct fixed-cardinality mask
iterator instead of scanning all 512 masks. StrongLinks(3) enumerates only the
valid colexicographic family multisets instead of filtering thousands of binary
combinations. Every such change remains behind
exact trace and cache-equivalence tests.

## GUI decision

The shared GUI is React/TypeScript with a layered SVG board. It consumes a
versioned primitive application-port contract and never receives mutable Rust
engine objects. The authoritative Rust session retains opaque hints, validates
expected revisions, applies server-owned inferences and returns complete
snapshots after every mutation.

The browser adapter runs the same dispatcher in a Web Worker backed by
WebAssembly. The desktop adapter exposes that dispatcher through Tauri and runs
solver work outside the UI thread. Platform adapters contain no session rules;
the React controller is dependency-injected against one asynchronous
`ApplicationPort`. See [`GUI_ARCHITECTURE.md`](GUI_ARCHITECTURE.md).
