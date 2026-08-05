import type { HintPresentation, HintRow } from '../model'
import { cellName } from '../model'

interface HintDetailsProps {
  hint: HintPresentation
  selectedRow: HintRow
}

export function HintDetails({ hint, selectedRow }: HintDetailsProps) {
  const isPrimary = selectedRow.presentation != null
  const affected = hint.placement
    ? `${cellName(hint.placement)} = ${hint.placement.digit}`
    : hint.affects.map((candidate) => `${cellName(candidate)}(${candidate.digit})`).join(', ') || 'None'
  const proofCount = hint.chainCount ?? hint.views.length
  const proofKind = hint.chainCount == null ? 'proof view' : 'proof chain'
  return (
    <section className="hint-details" aria-labelledby="hint-details-title">
      <div className="details-title-row">
        <h2 id="hint-details-title">Hint details</h2>
        <span>Hint rating <strong>{selectedRow.rating ? selectedRow.rating.toFixed(1) : hint.rating.toFixed(1)}</strong></span>
      </div>
      <dl>
        <dt>Technique</dt><dd>{selectedRow.label}</dd>
        <dt>Type</dt><dd>{isPrimary ? hint.type : selectedRow.group ? 'Technique family' : 'Available deductions'}</dd>
        <dt>Affects</dt><dd>{isPrimary ? affected : 'Select an individual hint'}</dd>
        <dt>Proof</dt><dd>{isPrimary ? `${proofCount} ${proofKind}${proofCount === 1 ? '' : 's'}` : '—'}</dd>
      </dl>
    </section>
  )
}
