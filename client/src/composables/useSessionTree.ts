// Folds the flat `GET /api/assistant/sessions` list into the spawn tree the
// sidebar renders (#103). Since #99 a sub-agent is an `assistant_sessions` row
// of its own carrying `parent_session_id`, so the list mixes roots and
// children — and a root's spawn budget is 16 per turn, so a research-heavy
// chat contributes dozens of child rows that only read as noise flat.
//
// Pure functions over a plain array: no store access, no reactivity, so the
// whole thing is unit-testable and the component decides what to re-derive.
import type { AssistantSession } from '../types'

export interface SessionTreeNode {
  session: AssistantSession
  /** 0 for a row rendered at root level; +1 per nesting step. */
  depth: number
  children: SessionTreeNode[]
}

/**
 * Roots ordered `updated_at DESC`, children `created_at ASC` within a parent
 * (a sub-agent transcript reads spawn-order, a chat list reads most-recent).
 * Ties break on `id` so the order is stable across refetches.
 *
 * A row whose `parent_session_id` names a session **not in the list** is an
 * orphan — it renders at root level rather than disappearing, since the
 * alternative is a chat the user can no longer reach at all. Cycles (a
 * corrupt/edited parent chain) are broken the same way: any row whose parent
 * is also its own descendant stays at root level, so every input row appears
 * exactly once.
 */
export function buildSessionTree(sessions: AssistantSession[]): SessionTreeNode[] {
  const byId = new Map<number, AssistantSession>()
  for (const s of sessions) byId.set(s.id, s)

  const nodes = new Map<number, SessionTreeNode>()
  for (const s of sessions) nodes.set(s.id, { session: s, depth: 0, children: [] })

  const roots: SessionTreeNode[] = []
  for (const s of sessions) {
    const node = nodes.get(s.id)
    if (!node) continue
    const parentId = s.parent_session_id
    const parent = parentId != null ? nodes.get(parentId) : undefined
    if (parent && parentId != null && !hasAncestor(byId, parentId, s.id)) {
      parent.children.push(node)
    } else {
      roots.push(node)
    }
  }

  roots.sort(byUpdatedDesc)
  assignDepth(roots, 0)
  return roots
}

/**
 * Visible rows, in render order: a node's children are included only while the
 * node is expanded. Flattening here (rather than a recursive component) keeps
 * the sidebar a single `v-for`, which keeps tab order and the mobile layout
 * exactly as they were before the tree.
 */
export function flattenSessionTree(
  nodes: SessionTreeNode[],
  isExpanded: (id: number) => boolean,
): SessionTreeNode[] {
  const out: SessionTreeNode[] = []
  const walk = (level: SessionTreeNode[]) => {
    for (const node of level) {
      out.push(node)
      if (node.children.length > 0 && isExpanded(node.session.id)) walk(node.children)
    }
  }
  walk(nodes)
  return out
}

/**
 * The session to open when nothing is selected yet. Deliberately not
 * `sessions[0]`: that is whatever the API sorted first, which since #99 can be
 * a sub-agent — a read-only child is a nonsense default view. Falls back to the
 * first row of the tree when the list is *only* orphaned children.
 */
export function firstRootSession(sessions: AssistantSession[]): AssistantSession | null {
  const tree = buildSessionTree(sessions)
  const root = tree.find((n) => n.session.parent_session_id == null)
  return root?.session ?? tree[0]?.session ?? null
}

/** Ids from `id` up to its outermost ancestor, so the sidebar can reveal a child. */
export function ancestorIds(sessions: AssistantSession[], id: number): number[] {
  const byId = new Map(sessions.map((s) => [s.id, s]))
  const out: number[] = []
  const seen = new Set<number>()
  let cur = byId.get(id)?.parent_session_id ?? null
  while (cur != null && !seen.has(cur)) {
    seen.add(cur)
    out.push(cur)
    cur = byId.get(cur)?.parent_session_id ?? null
  }
  return out
}

// True when `ancestorId` sits on `id`'s own parent chain — i.e. linking `id`
// under it would close a loop. `seen` bounds the walk so a pre-existing cycle
// in the data terminates instead of hanging the sidebar.
function hasAncestor(
  byId: Map<number, AssistantSession>,
  id: number,
  ancestorId: number,
): boolean {
  const seen = new Set<number>()
  let cur: number | null = id
  while (cur != null && !seen.has(cur)) {
    if (cur === ancestorId) return true
    seen.add(cur)
    cur = byId.get(cur)?.parent_session_id ?? null
  }
  return false
}

function assignDepth(nodes: SessionTreeNode[], depth: number) {
  for (const node of nodes) {
    node.depth = depth
    node.children.sort(byCreatedAsc)
    assignDepth(node.children, depth + 1)
  }
}

function byUpdatedDesc(a: SessionTreeNode, b: SessionTreeNode): number {
  const cmp = b.session.updated_at.localeCompare(a.session.updated_at)
  return cmp !== 0 ? cmp : b.session.id - a.session.id
}

function byCreatedAsc(a: SessionTreeNode, b: SessionTreeNode): number {
  const cmp = a.session.created_at.localeCompare(b.session.created_at)
  return cmp !== 0 ? cmp : a.session.id - b.session.id
}
