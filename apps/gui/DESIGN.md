# GUI design-system inventory

This vertical slice implements the accepted 1580 × 1000 layout and design-token
contract. Renderer and accessibility tests pin the code-native structure.

## Color system

- Application and panel surfaces are true white (`#ffffff`). The small title-bar field uses cool slate `#f7f9fc`; no warm/off-white substitution is allowed.
- Primary text is ink slate `#101828`, secondary text `#475467`, and quiet text `#667085`.
- Dividers use `#d9e0ea`; control borders use `#cfd7e4`.
- Primary action/selection is cobalt `#0b63f6`; its quiet fill is `#eaf2ff`.
- Semantic Sudoku overlays are role-based: positive green `#12933b`, negative/elimination red `#e22b35`, auxiliary blue `#0b70c9`, grouped orange `#ed8b00`, and selected cobalt `#1769ff`. Technique data never supplies literal colors.
- Permanent classic regions use a crisp dark boundary. The topology model also supports overlay regions and paths, but the fixture intentionally stays Classic so its givens and exact candidate masks remain honest. Hint-region fills are a separate, more saturated SVG layer.

## Type and icon system

- Inter is the UI/chrome and content family. Tabular numeric columns opt into tabular figures.
- Product name: 19 px / 700. Section title: 15 px / 700. Body: 13 px / 1.55. Controls: 13 px / 500. Captions/status: 12 px.
- Lucide is the sole general UI icon family: 18 px, 1.8 px stroke, round caps/joins. The Forge brand mark is the only custom icon.

## Geometry and density

- Spacing scale: 4, 6, 8, 12, 16, 20, 24, 32 px.
- Title bar 42 px; main toolbar 54 px; ordinary controls 34–36 px; status bar 34 px.
- Panels are open rails separated by 1 px dividers. Controls use 5–7 px radii; panels do not become floating rounded cards.
- Shadows are limited to the app frame, focused board cell, and popup-like controls. The core workspace is flat.
- Board labels sit outside a square SVG. Fine grid lines are 1 px and classic block boundaries are 3 px. Board values are 40 px and candidates 17 px at the 900-unit design scale.

## SVG board layer order

1. True-white board paper and permanent topology region washes, when supplied.
2. Semantic hint-region backgrounds.
3. Semantic cell fills/outlines.
4. Fine grid and topology-supplied region/path boundaries (classic boxes in this fixture).
5. Placed values.
6. Strong/weak/grouped links and arrowheads.
7. Candidates and their semantic halos, kept above links for legibility.
8. Keyboard/pointer selection and transparent hit target.

At narrower sizes the inspector stacks below the board, then the explanation workbench becomes a vertical reading order. No second mobile GUI or alternate component tree is introduced.
