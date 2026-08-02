import { describe, it, expect, beforeEach, vi } from 'vitest'
import { ref, type Ref } from 'vue'
import { useLiveTurns } from './assistantLiveTurns'
import type { AssistantSessionDetail, WsEnvelope } from '../types'

// `useLiveTurns` registers its handler via `useWsStore().on('assistant', ...)`
// at call time — stub that store to capture the handler so tests can feed it
// synthetic envelopes directly, the same shape `ws_bridge.rs` sends over the
// wire (mirrors `assistant.spec.ts`'s approach for the same seam).
let wsHandler: ((envelope: WsEnvelope) => void) | undefined
vi.mock('./ws', () => ({
  useWsStore: () => ({
    on: (_topic: string, handler: (envelope: WsEnvelope) => void) => {
      wsHandler = handler
      return () => {}
    },
  }),
}))

function envelope(event: string, payload: Record<string, any>): WsEnvelope {
  return { topic: 'assistant', event, payload }
}

describe('useLiveTurns — sub-agent routing', () => {
  let current: Ref<AssistantSessionDetail | null>
  let sending: Ref<boolean>
  let loadSession: ReturnType<typeof vi.fn<(id: number) => Promise<AssistantSessionDetail>>>

  beforeEach(() => {
    vi.clearAllMocks()
    wsHandler = undefined
    current = ref<AssistantSessionDetail | null>(null)
    sending = ref(false)
    loadSession = vi.fn().mockResolvedValue(undefined)
  })

  function setup() {
    return useLiveTurns(current, sending, loadSession)
  }

  // ---- sub-agent routing ----

  it('session_started with agent_session_id creates a liveSubAgents entry, not the root live turn', () => {
    const { live, liveSubAgents } = setup()
    wsHandler!(
      envelope('session_started', {
        db_session_id: 1,
        agent_session_id: 'child-1',
        profile: 'researcher',
      }),
    )
    expect(live.value).toBeNull()
    expect(liveSubAgents.value['child-1']).toMatchObject({
      agentSessionId: 'child-1',
      dbSessionId: 1,
      profile: 'researcher',
      text: '',
    })
  })

  it('accumulates sub-agent text_delta/tool_call independently of the root turn', () => {
    const { live, liveSubAgents } = setup()
    wsHandler!(envelope('text_delta', { db_session_id: 1, text: 'root' }))
    wsHandler!(
      envelope('text_delta', { db_session_id: 1, agent_session_id: 'child-1', text: 'child' }),
    )
    wsHandler!(
      envelope('tool_call', {
        db_session_id: 1,
        agent_session_id: 'child-1',
        request_id: 'c1',
        tool: 'page_search',
        input: '{"q":"x"}',
      }),
    )

    expect(live.value).toMatchObject({ sessionId: 1, text: 'root' })
    expect(liveSubAgents.value['child-1'].text).toBe('child')
    expect(liveSubAgents.value['child-1'].toolCalls[0]).toMatchObject({
      id: 'c1',
      name: 'page_search',
      args: { q: 'x' },
    })
  })

  it('sub-agent tool_output marks the matching child tool call done', () => {
    const { liveSubAgents } = setup()
    wsHandler!(
      envelope('tool_call', {
        db_session_id: 1,
        agent_session_id: 'child-1',
        request_id: 'c1',
        tool: 'page_search',
        input: '{}',
      }),
    )
    wsHandler!(
      envelope('tool_output', {
        db_session_id: 1,
        agent_session_id: 'child-1',
        request_id: 'c1',
        output: 'found',
      }),
    )
    expect(liveSubAgents.value['child-1'].toolCalls[0]).toMatchObject({
      status: 'done',
      output: 'found',
    })
  })

  it('sub-agent done drops the entry and refetches when it belongs to the current session', () => {
    current.value = { id: 1 } as AssistantSessionDetail
    const { liveSubAgents } = setup()
    wsHandler!(
      envelope('session_started', { db_session_id: 1, agent_session_id: 'child-1' }),
    )
    wsHandler!(envelope('done', { db_session_id: 1, agent_session_id: 'child-1' }))

    expect(liveSubAgents.value['child-1']).toBeUndefined()
    expect(loadSession).toHaveBeenCalledWith(1)
  })

  it('sub-agent done for a non-current session drops the entry without refetching', () => {
    current.value = { id: 2 } as AssistantSessionDetail
    const { liveSubAgents } = setup()
    wsHandler!(
      envelope('session_started', { db_session_id: 1, agent_session_id: 'child-1' }),
    )
    wsHandler!(envelope('done', { db_session_id: 1, agent_session_id: 'child-1' }))

    expect(liveSubAgents.value['child-1']).toBeUndefined()
    expect(loadSession).not.toHaveBeenCalled()
  })

  // ---- child_db_session_id (#102): additive, never a repurposed db_session_id ----

  it('keeps dbSessionId on the parent while recording the child row as childDbSessionId', () => {
    const { liveSubAgents } = setup()
    wsHandler!(
      envelope('session_started', {
        db_session_id: 1,
        agent_session_id: 'child-1',
        child_db_session_id: 5,
        profile: 'researcher',
      }),
    )
    wsHandler!(
      envelope('text_delta', {
        db_session_id: 1,
        agent_session_id: 'child-1',
        child_db_session_id: 5,
        text: 'child',
      }),
    )

    // `dbSessionId` must stay the parent — `AssistantView.vue` filters the
    // inline running-sub-agent card on it.
    expect(liveSubAgents.value['child-1']).toMatchObject({
      dbSessionId: 1,
      childDbSessionId: 5,
      text: 'child',
    })
  })

  it('backfills childDbSessionId from a later event when the first one lacked it', () => {
    const { liveSubAgents } = setup()
    wsHandler!(envelope('session_started', { db_session_id: 1, agent_session_id: 'child-1' }))
    expect(liveSubAgents.value['child-1'].childDbSessionId).toBeUndefined()

    wsHandler!(
      envelope('text_delta', {
        db_session_id: 1,
        agent_session_id: 'child-1',
        child_db_session_id: 5,
        text: 'child',
      }),
    )
    expect(liveSubAgents.value['child-1'].childDbSessionId).toBe(5)
  })

  it('sub-agent done still refetches the parent when the parent is the open session', () => {
    current.value = { id: 1 } as AssistantSessionDetail
    const { liveSubAgents } = setup()
    wsHandler!(
      envelope('session_started', {
        db_session_id: 1,
        agent_session_id: 'child-1',
        child_db_session_id: 5,
      }),
    )
    wsHandler!(
      envelope('done', { db_session_id: 1, agent_session_id: 'child-1', child_db_session_id: 5 }),
    )

    expect(liveSubAgents.value['child-1']).toBeUndefined()
    expect(loadSession).toHaveBeenCalledTimes(1)
    expect(loadSession).toHaveBeenCalledWith(1)
  })

  it('sub-agent done refetches the child when the child itself is the open session', () => {
    current.value = { id: 5 } as AssistantSessionDetail
    setup()
    wsHandler!(
      envelope('session_started', {
        db_session_id: 1,
        agent_session_id: 'child-1',
        child_db_session_id: 5,
      }),
    )
    wsHandler!(
      envelope('done', { db_session_id: 1, agent_session_id: 'child-1', child_db_session_id: 5 }),
    )

    expect(loadSession).toHaveBeenCalledTimes(1)
    expect(loadSession).toHaveBeenCalledWith(5)
  })

  it('sub-agent done refetches nobody when neither the parent nor the child is open', () => {
    current.value = { id: 9 } as AssistantSessionDetail
    setup()
    wsHandler!(
      envelope('done', { db_session_id: 1, agent_session_id: 'child-1', child_db_session_id: 5 }),
    )

    expect(loadSession).not.toHaveBeenCalled()
  })

  // ---- resolveLiveToolCall: the click-triggered path (#stuck approval fix) ----

  it('resolveLiveToolCall marks a root call done without output, simulating the click path', () => {
    const { live, resolveLiveToolCall } = setup()
    wsHandler!(
      envelope('tool_request', {
        db_session_id: 1,
        request_id: 'c1',
        tool: 'page_delete',
        input: '{"id":1}',
      }),
    )
    expect(live.value!.toolCalls[0].status).toBe('requires_approval')

    resolveLiveToolCall('c1')

    expect(live.value!.toolCalls[0].status).toBe('done')
    expect(live.value!.toolCalls[0].output).toBeUndefined()
  })

  it('a tool_output arriving after resolveLiveToolCall fills in the output idempotently', () => {
    const { live, resolveLiveToolCall } = setup()
    wsHandler!(
      envelope('tool_request', {
        db_session_id: 1,
        request_id: 'c1',
        tool: 'page_delete',
        input: '{"id":1}',
      }),
    )

    resolveLiveToolCall('c1')
    expect(live.value!.toolCalls[0].status).toBe('done')
    expect(live.value!.toolCalls[0].output).toBeUndefined()

    wsHandler!(envelope('tool_output', { db_session_id: 1, request_id: 'c1', output: 'deleted' }))

    expect(live.value!.toolCalls[0]).toMatchObject({ status: 'done', output: 'deleted' })
  })

  it('resolveLiveToolCall targets the right sub-agent bucket when given an agentSessionId', () => {
    const { liveSubAgents, resolveLiveToolCall } = setup()
    wsHandler!(
      envelope('tool_request', {
        db_session_id: 1,
        agent_session_id: 'child-1',
        request_id: 'c1',
        tool: 'page_search',
        input: '{}',
      }),
    )
    expect(liveSubAgents.value['child-1'].toolCalls[0].status).toBe('requires_approval')

    resolveLiveToolCall('c1', 'child-1')

    expect(liveSubAgents.value['child-1'].toolCalls[0].status).toBe('done')
    expect(liveSubAgents.value['child-1'].toolCalls[0].output).toBeUndefined()
  })

  it('resolveLiveToolCall is a no-op when the call id is not found', () => {
    const { live, resolveLiveToolCall } = setup()
    expect(() => resolveLiveToolCall('missing')).not.toThrow()
    expect(live.value).toBeNull()
  })
})
