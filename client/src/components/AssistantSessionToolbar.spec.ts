import { describe, it, expect, beforeEach, vi } from 'vitest'
import { mount } from '@vue/test-utils'
import AssistantSessionToolbar from './AssistantSessionToolbar.vue'
import { useAssistantStore } from '../stores/assistant'

// A sub-agent session (#99) runs under the model/profile its parent spawned it
// with, and the server refuses to switch either or to compact it (#101) — so
// the toolbar must not offer a control that can only 4xx (#103).
vi.mock('../stores/assistant', () => ({ useAssistantStore: vi.fn() }))

const useAssistantStoreMock = vi.mocked(useAssistantStore)

function withSession(overrides: Record<string, any>) {
  useAssistantStoreMock.mockReturnValue({
    current: {
      id: 2,
      provider: 'anthropic',
      model: 'sonnet',
      model_id: 1,
      agent_profile: 'build',
      parent_session_id: null,
      enabled_mcp_server_ids: [],
      messages: [{ id: 1 }],
      ...overrides,
    },
    models: [],
    mcpServers: [],
    sending: false,
  } as any)
}

describe('AssistantSessionToolbar', () => {
  beforeEach(() => vi.clearAllMocks())

  it('offers model, profile and compact on a root session', () => {
    withSession({})
    const wrapper = mount(AssistantSessionToolbar)

    expect(wrapper.findAll('select')).toHaveLength(2)
    expect(wrapper.text()).toContain('Compact')
  })

  it('goes read-only on a sub-agent session — no model picker, compact or profile switch', () => {
    withSession({ parent_session_id: 1, agent_profile: 'researcher' })
    const wrapper = mount(AssistantSessionToolbar)

    expect(wrapper.findAll('select')).toHaveLength(0)
    expect(wrapper.text()).not.toContain('Compact')
    // Replaced by a static "what this child runs as" badge.
    expect(wrapper.text()).toContain('🔎 researcher')
    // The MCP and generation pickers are not part of the spawn contract.
    expect(wrapper.text()).toContain('MCP')
    expect(wrapper.text()).toContain('Gen')
  })
})
