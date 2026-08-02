import { describe, it, expect } from 'vitest'
import {
  ancestorIds,
  buildSessionTree,
  firstRootSession,
  flattenSessionTree,
} from './useSessionTree'
import type { AssistantSession } from '../types'

function session(
  id: number,
  overrides: Partial<AssistantSession> = {},
): AssistantSession {
  return {
    id,
    title: `chat ${id}`,
    provider: 'anthropic',
    model: 'sonnet',
    model_id: 1,
    enabled_mcp_server_ids: [],
    temperature: null,
    reasoning_effort: null,
    max_output_tokens: null,
    thinking_budget_tokens: null,
    agent_profile: 'build',
    parent_session_id: null,
    root_engine_session_id: 'u1:root',
    created_at: '2026-08-01T10:00:00Z',
    updated_at: '2026-08-01T10:00:00Z',
    ...overrides,
  }
}

const ids = (nodes: { session: AssistantSession }[]) => nodes.map((n) => n.session.id)

describe('buildSessionTree', () => {
  it('nests children under their parent and leaves the roots at depth 0', () => {
    const tree = buildSessionTree([
      session(1),
      session(2, { parent_session_id: 1, agent_profile: 'researcher' }),
      session(3),
    ])

    expect(ids(tree)).toEqual([3, 1])
    expect(tree.map((n) => n.depth)).toEqual([0, 0])
    expect(ids(tree[1].children)).toEqual([2])
    expect(tree[1].children[0].depth).toBe(1)
  })

  it('orders roots by updated_at DESC and children by created_at ASC', () => {
    const tree = buildSessionTree([
      session(1, { updated_at: '2026-08-01T09:00:00Z' }),
      session(2, { updated_at: '2026-08-01T12:00:00Z' }),
      session(11, { parent_session_id: 2, created_at: '2026-08-01T12:30:00Z' }),
      session(12, { parent_session_id: 2, created_at: '2026-08-01T12:10:00Z' }),
      session(13, { parent_session_id: 2, created_at: '2026-08-01T12:20:00Z' }),
    ])

    expect(ids(tree)).toEqual([2, 1])
    expect(ids(tree[0].children)).toEqual([12, 13, 11])
  })

  it('renders an orphan at root level rather than dropping it', () => {
    // The parent row is not in the list (deleted, or not yet fetched) — the
    // child must stay reachable.
    const tree = buildSessionTree([
      session(1),
      session(9, { parent_session_id: 404, agent_profile: 'researcher' }),
    ])

    expect(ids(tree).sort()).toEqual([1, 9])
    expect(tree.find((n) => n.session.id === 9)!.depth).toBe(0)
  })

  it('nests a grandchild at depth 2 (a sub-agent spawning its own sub-agent)', () => {
    const tree = buildSessionTree([
      session(1),
      session(2, { parent_session_id: 1 }),
      session(3, { parent_session_id: 2 }),
    ])

    const child = tree[0].children[0]
    expect(child.session.id).toBe(2)
    expect(ids(child.children)).toEqual([3])
    expect(child.children[0].depth).toBe(2)
  })

  it('breaks a parent cycle instead of hanging, keeping every row exactly once', () => {
    const tree = buildSessionTree([
      session(1, { parent_session_id: 2 }),
      session(2, { parent_session_id: 1 }),
      session(3, { parent_session_id: 3 }),
    ])

    const flat = flattenSessionTree(tree, () => true)
    expect(ids(flat).sort()).toEqual([1, 2, 3])
  })

  it('places every input row exactly once for a mixed list', () => {
    const input = [
      session(1),
      session(2, { parent_session_id: 1 }),
      session(3, { parent_session_id: 2 }),
      session(4, { parent_session_id: 77 }),
      session(5),
    ]

    const flat = flattenSessionTree(buildSessionTree(input), () => true)
    expect(flat).toHaveLength(input.length)
  })
})

describe('flattenSessionTree', () => {
  const input = [
    session(1, { updated_at: '2026-08-01T12:00:00Z' }),
    session(2, { parent_session_id: 1 }),
    session(3, { parent_session_id: 2 }),
    session(4, { updated_at: '2026-08-01T09:00:00Z' }),
  ]

  it('hides children of a collapsed node', () => {
    const flat = flattenSessionTree(buildSessionTree(input), () => false)
    expect(ids(flat)).toEqual([1, 4])
  })

  it('reveals only the expanded branch, in render order', () => {
    const expanded = new Set([1])
    const flat = flattenSessionTree(buildSessionTree(input), (id) => expanded.has(id))
    expect(ids(flat)).toEqual([1, 2, 4])

    expanded.add(2)
    const deeper = flattenSessionTree(buildSessionTree(input), (id) => expanded.has(id))
    expect(ids(deeper)).toEqual([1, 2, 3, 4])
  })
})

describe('firstRootSession', () => {
  it('skips a sub-agent the API happened to sort first', () => {
    const sessions = [
      session(9, { parent_session_id: 1, updated_at: '2026-08-01T14:00:00Z' }),
      session(1, { updated_at: '2026-08-01T13:00:00Z' }),
    ]
    expect(firstRootSession(sessions)?.id).toBe(1)
  })

  it('falls back to the first row when the list holds only orphans', () => {
    expect(firstRootSession([session(9, { parent_session_id: 404 })])?.id).toBe(9)
  })

  it('returns null for an empty list', () => {
    expect(firstRootSession([])).toBeNull()
  })
})

describe('ancestorIds', () => {
  it('walks a grandchild up to its root', () => {
    const sessions = [
      session(1),
      session(2, { parent_session_id: 1 }),
      session(3, { parent_session_id: 2 }),
    ]
    expect(ancestorIds(sessions, 3)).toEqual([2, 1])
    expect(ancestorIds(sessions, 1)).toEqual([])
  })

  it('terminates on a cycle', () => {
    const sessions = [
      session(1, { parent_session_id: 2 }),
      session(2, { parent_session_id: 1 }),
    ]
    expect(ancestorIds(sessions, 1).sort()).toEqual([1, 2])
  })
})
