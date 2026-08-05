import { X } from 'lucide-react'
import { Fragment } from 'react'
import type { ChainLink, ExplanationInline, HintPresentation, HintView, LinkEndpoint, LinkRole } from '../model'
import { candidateKey, cellName } from '../model'

interface ExplanationProps {
  hint: HintPresentation | null
  view: HintView | null
  applied: boolean
}

const flowClassByRole: Record<LinkRole, string> = {
  'strong-true': '0',
  'strong-false': '1',
  grouped: '2',
  strong: '0',
  weak: '1',
  'grouped-strong': '2',
  implication: '3',
}

const linkLabelByRole: Record<LinkRole, string> = {
  'strong-true': 'Strong link, true branch',
  'strong-false': 'Strong link, false branch',
  grouped: 'Grouped link',
  strong: 'Strong link',
  weak: 'Weak link',
  'grouped-strong': 'Grouped strong link',
  implication: 'Implication',
}

const endpointLabel = (endpoint: LinkEndpoint) => {
  if (endpoint.kind === 'candidate') return `${cellName(endpoint.candidate)}(${endpoint.candidate.digit})`
  if (endpoint.kind === 'cell-center') return cellName(endpoint.cell)
  return `group ${endpoint.members.map((member) => `${cellName(member)}(${member.digit})`).join(', ')}`
}

const connectorLabel = (link: ChainLink) => `${linkLabelByRole[link.role]} from ${endpointLabel(link.from)} ${link.direction === 'both' ? 'in both directions with' : 'to'} ${endpointLabel(link.to)}`

const ChainNode = ({ label, digit, active = false }: { label: string; digit: number; active?: boolean }) => (
  <div className={`chain-node${active ? ' is-active' : ''}`} aria-label={`${label}, candidate ${digit}`}>
    <span>{label}</span>
    <strong>{digit}</strong>
  </div>
)

function renderExplanationInline(inline: ExplanationInline, hint: HintPresentation, key: number) {
  switch (inline.kind) {
    case 'text':
      return <Fragment key={key}>{inline.text}</Fragment>
    case 'technique':
      return <strong key={key} data-explanation-type="technique" data-technique-key={inline.techniqueKey}>{inline.techniqueKey === hint.techniqueKey ? hint.shortName : inline.techniqueKey}</strong>
    case 'cell':
      return <span key={key} data-explanation-type="cell">{cellName(inline.cell)}</span>
    case 'digit':
      return <span key={key} data-explanation-type="digit">{inline.digit}</span>
    case 'region':
      return <span key={key} data-explanation-type="region" data-region-type={inline.regionType} data-region-index={inline.regionIndex}>region {inline.regionType}:{inline.regionIndex + 1}</span>
    case 'candidate':
      return <span key={key} data-explanation-type="candidate">{cellName(inline.candidate)}({inline.candidate.digit})</span>
  }
}

function StructuredExplanation({ hint }: { hint: HintPresentation }) {
  return hint.explanation?.blocks.map((block, blockIndex) => block.kind === 'paragraph' ? (
    <p key={blockIndex}>{block.inlines.map((inline, inlineIndex) => renderExplanationInline(inline, hint, inlineIndex))}</p>
  ) : (
    <ul key={blockIndex}>
      {block.items.map((item, itemIndex) => (
        <li key={itemIndex}>{item.map((inline, inlineIndex) => renderExplanationInline(inline, hint, inlineIndex))}</li>
      ))}
    </ul>
  )) ?? null
}

export function Explanation({ hint, view, applied }: ExplanationProps) {
  if (!hint || !view) {
    return (
      <section className="explanation-panel" aria-labelledby="explanation-title">
        <div className="explanation-copy">
          <div className="section-heading compact-heading">
            <div>
              <h2 id="explanation-title">Explanation</h2>
              <p>No individual hint selected</p>
            </div>
          </div>
          <p>Select an individual deduction in the hint browser to inspect its proof, board marks, and effects.</p>
        </div>
        <div className="chain-workbench" aria-label="No proof loaded">
          <div className="chain-workbench-title">
            <span>No hint selected</span>
            <small>No proof loaded</small>
          </div>
          <div className="chain-flow">Choose a hint with an available presentation.</div>
        </div>
        <aside className="elimination-list">
          <h3>Eliminations</h3>
          <div>None selected</div>
        </aside>
      </section>
    )
  }

  const digit = hint.affects[0]?.digit ?? view.chainCells[0]?.digit
  const chainViews = hint.views.filter((hintView) => hintView.links.length > 0 && hintView.chainCells.length > 0)
  const affectedCandidates = hint.affects.map((candidate) => `${cellName(candidate)}(${candidate.digit})`).join(', ')
  const proofCount = hint.chainCount ?? hint.views.length
  const proofKind = hint.chainCount == null ? 'view' : 'chain'

  return (
    <section className="explanation-panel" aria-labelledby="explanation-title">
      <div className="explanation-copy">
        <div className="section-heading compact-heading">
          <div>
            <h2 id="explanation-title">Explanation</h2>
            <p>Why this deduction works</p>
          </div>
          {applied && <span className="applied-label">Applied</span>}
        </div>
        {hint.explanation ? <StructuredExplanation hint={hint} /> : (
          <>
            <p>This is a <strong>{hint.shortName}</strong>{digit ? <> on digit <strong>{digit}</strong></> : null}.</p>
            {chainViews.length > 0 && (
              <p className="chain-text">
                {chainViews.map((chainView, index) => (
                  <Fragment key={chainView.id}>
                    <span>{chainView.label}</span>
                    {chainView.chainCells.map((candidate) => `${cellName(candidate)}(${candidate.digit})`).join(' → ')}
                    {index < chainViews.length - 1 && <br />}
                  </Fragment>
                ))}
              </p>
            )}
            <p>{proofCount} proof {proofKind}{proofCount === 1 ? '' : 's'} support this {hint.type.toLocaleLowerCase()}.</p>
            <p>{hint.affects.length} candidate elimination{hint.affects.length === 1 ? '' : 's'}: {affectedCandidates || 'none'}.</p>
          </>
        )}
      </div>
      <div className="chain-workbench">
        <div className="chain-workbench-title">
          <span>{view.label}</span>
          <small>{view.links.length} links{digit ? ` · digit ${digit}` : ''}</small>
        </div>
        <div className="chain-flow" aria-label={`${view.label} diagram`}>
          {view.chainCells.map((candidate, index, cells) => {
            const link = view.links[index]
            return (
              <div className="chain-flow-step" key={`${candidate.row}-${candidate.col}-${candidate.digit}`}>
                <ChainNode label={cellName(candidate)} digit={candidate.digit} active={index === 0 || index === cells.length - 1} />
                {link && (
                  <span
                    className={`flow-link flow-link--${flowClassByRole[link.role]}`}
                    data-link-id={link.id}
                    data-link-role={link.role}
                    data-link-direction={link.direction}
                    aria-label={connectorLabel(link)}
                    title={connectorLabel(link)}
                  >
                    {link.direction === 'both' ? '↔' : '→'}
                  </span>
                )}
              </div>
            )
          })}
          {view.chainCells.length === 0 && view.links.length > 0 && (
            <div className="chain-edge-list">
              {view.links.map((link) => (
                <div
                  className="chain-edge"
                  key={link.id}
                  data-link-id={link.id}
                  data-link-role={link.role}
                  data-link-direction={link.direction}
                  aria-label={connectorLabel(link)}
                  title={connectorLabel(link)}
                >
                  <span>{endpointLabel(link.from)}</span>
                  <strong className={`flow-link flow-link--${flowClassByRole[link.role]}`}>{link.direction === 'both' ? '↔' : '→'}</strong>
                  <span>{endpointLabel(link.to)}</span>
                </div>
              ))}
            </div>
          )}
          {view.chainCells.length === 0 && view.links.length === 0 && <span>No chain path in this view.</span>}
        </div>
        <div className="chain-legend">
          <span><i className="legend-line strong" /> Strong link (true)</span>
          <span><i className="legend-line false" /> Weak link</span>
          <span><i className="legend-line grouped" /> Grouped strong{digit ? ` (digit ${digit})` : ''}</span>
          <span><i className="legend-line implication" /> Implication</span>
        </div>
      </div>
      <aside className="elimination-list">
        <h3>{hint.placement ? 'Placement' : 'Eliminations'}</h3>
        {hint.placement ? (
          <div><span>{cellName(hint.placement)} = {hint.placement.digit}</span></div>
        ) : hint.affects.map((candidate) => (
          <div key={candidateKey(candidate)}><X aria-hidden="true" /> <span>{cellName(candidate)} ({candidate.digit})</span></div>
        ))}
      </aside>
    </section>
  )
}
