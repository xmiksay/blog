<script setup lang="ts">
import { computed, nextTick, onMounted, ref, watch } from 'vue'
import { useAssistantStore } from '../stores/assistant'

const assistant = useAssistantStore()
const draft = ref('')
const messageBox = ref<HTMLDivElement | null>(null)

onMounted(async () => {
  await Promise.all([
    assistant.loadSessions(),
    assistant.loadModels(),
    assistant.loadPermissions(),
    assistant.loadMcpServers(),
  ])
  if (assistant.sessions.length > 0) {
    await select(assistant.sessions[0].id)
  }
})

async function newSession() {
  if (assistant.models.length === 0) {
    alert('Add a provider and a model first under "LLM providers" / "LLM models".')
    return
  }
  const s = await assistant.createSession()
  await select(s.id)
}

async function changeModel(modelId: number) {
  if (!assistant.current) return
  await assistant.updateSession(assistant.current.id, { model_id: modelId })
  await assistant.loadSession(assistant.current.id)
}

const showMcpPicker = ref(false)

async function toggleMcpServer(serverId: number, on: boolean) {
  if (!assistant.current) return
  const current = assistant.current.enabled_mcp_server_ids ?? []
  const next = on
    ? Array.from(new Set([...current, serverId]))
    : current.filter((id) => id !== serverId)
  await assistant.updateSession(assistant.current.id, {
    enabled_mcp_server_ids: next,
  })
  await assistant.loadSession(assistant.current.id)
}

async function select(id: number) {
  await assistant.loadSession(id)
  scrollToBottom()
}

async function send() {
  const text = draft.value.trim()
  if (!text || !assistant.current) return
  draft.value = ''
  await assistant.sendMessage(assistant.current.id, text)
  scrollToBottom()
}

function scrollToBottom() {
  nextTick(() => {
    if (messageBox.value) {
      messageBox.value.scrollTop = messageBox.value.scrollHeight
    }
  })
}

watch(() => assistant.current?.messages.length, scrollToBottom)

async function deleteSession(id: number) {
  if (!confirm('Delete this chat?')) return
  await assistant.deleteSession(id)
  if (!assistant.current && assistant.sessions.length > 0) {
    await select(assistant.sessions[0].id)
  }
}

async function updateTitle() {
  if (!assistant.current) return
  const newTitle = prompt('New title', assistant.current.title)
  if (newTitle && newTitle !== assistant.current.title) {
    await assistant.updateSession(assistant.current.id, { title: newTitle })
    if (assistant.current) assistant.current.title = newTitle
  }
}

const messageList = computed(() => assistant.current?.messages ?? [])

interface ToolCall {
  id: string
  name: string
  args: any
}

function messageText(content: any): string {
  if (!content) return ''
  if (typeof content === 'string') return content
  if (typeof content.text === 'string') return content.text
  if ('text' in content || 'tool_calls' in content || 'decisions' in content) return ''
  return JSON.stringify(content)
}

function toolCalls(content: any): ToolCall[] {
  if (!content || !Array.isArray(content.tool_calls)) return []
  return content.tool_calls.map((tc: any) => ({
    id: tc.id ?? '',
    name: tc.name,
    args: tc.args,
  }))
}

function toolResult(content: any): { tool_call_id?: string; output?: any; is_error?: boolean } {
  return content || {}
}

function requiresApproval(content: any): boolean {
  return Boolean(content?.requires_approval)
}

function decisionFor(content: any, callId: string): boolean | undefined {
  const arr = Array.isArray(content?.decisions) ? content.decisions : []
  const found = arr.find((d: any) => d.tool_call_id === callId)
  return found ? !!found.approve : undefined
}

async function decide(
  messageId: number,
  callId: string,
  approve: boolean,
  remember = false,
) {
  if (!assistant.current) return
  await assistant.approveToolCalls(assistant.current.id, messageId, [
    { tool_call_id: callId, approve, remember },
  ])
  scrollToBottom()
}

async function decideAll(
  messageId: number,
  calls: ToolCall[],
  approve: boolean,
  remember = false,
) {
  if (!assistant.current) return
  await assistant.approveToolCalls(
    assistant.current.id,
    messageId,
    calls.map((c) => ({ tool_call_id: c.id, approve, remember })),
  )
  scrollToBottom()
}
</script>

<template>
  <div class="flex h-[calc(100vh-8rem)] md:h-[calc(100vh-3rem)] gap-4">
    <aside
      class="w-full md:w-64 bg-white rounded-lg shadow flex-col"
      :class="assistant.current ? 'hidden md:flex' : 'flex'"
    >
      <div class="p-3 border-b flex items-center justify-between">
        <h2 class="font-semibold">Chats</h2>
        <button
          class="text-sm rounded bg-gray-800 hover:bg-gray-700 text-white px-2 py-1"
          @click="newSession"
        >
          New
        </button>
      </div>
      <div class="flex-1 overflow-y-auto">
        <button
          v-for="s in assistant.sessions"
          :key="s.id"
          class="w-full text-left px-3 py-2 border-b border-gray-100 hover:bg-gray-50 flex justify-between items-start gap-2"
          :class="assistant.current?.id === s.id ? 'bg-gray-100' : ''"
          @click="select(s.id)"
        >
          <div class="flex-1 min-w-0">
            <div class="text-sm font-medium truncate">{{ s.title }}</div>
            <div class="text-xs text-gray-500">{{ s.model }}</div>
          </div>
          <button
            class="text-xs text-gray-400 hover:text-red-500 shrink-0"
            @click.stop="deleteSession(s.id)"
            title="Delete"
          >
            ×
          </button>
        </button>
        <div v-if="assistant.sessions.length === 0" class="p-4 text-sm text-gray-400">
          No chats yet.
        </div>
      </div>
    </aside>

    <section
      class="flex-1 bg-white rounded-lg shadow flex-col min-w-0"
      :class="!assistant.current ? 'hidden md:flex' : 'flex'"
    >
      <header v-if="assistant.current" class="p-3 border-b flex items-center justify-between gap-2">
        <div class="flex items-center gap-2 min-w-0">
          <button
            type="button"
            class="md:hidden p-1 rounded hover:bg-gray-100 text-gray-600"
            aria-label="Back to chats"
            @click="assistant.current = null"
          >
            <svg xmlns="http://www.w3.org/2000/svg" class="h-5 w-5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 19l-7-7 7-7" />
            </svg>
          </button>
          <button
            class="text-left hover:underline truncate font-semibold"
            @click="updateTitle"
            :title="assistant.current.title"
          >
            {{ assistant.current.title }}
          </button>
        </div>
        <div class="text-xs text-gray-500 flex items-center gap-2">
          <select
            class="border rounded px-2 py-1 text-xs"
            :value="assistant.current.model_id ?? ''"
            @change="changeModel(Number(($event.target as HTMLSelectElement).value))"
          >
            <option v-if="!assistant.current.model_id" :value="''" disabled>
              {{ assistant.current.provider }} / {{ assistant.current.model }}
            </option>
            <option v-for="m in assistant.models" :key="m.id" :value="m.id">
              {{ m.label }} ({{ m.provider_label }})
            </option>
          </select>
          <div class="relative">
            <button
              type="button"
              class="border rounded px-2 py-1 text-xs hover:bg-gray-50"
              @click="showMcpPicker = !showMcpPicker"
              :title="'MCP servers active in this chat'"
            >
              MCP
              <span class="ml-1 inline-block min-w-[1rem] text-center rounded bg-gray-100 px-1">
                {{ (assistant.current.enabled_mcp_server_ids ?? []).length }}/{{
                  assistant.mcpServers.length
                }}
              </span>
            </button>
            <div
              v-if="showMcpPicker"
              class="absolute right-0 mt-1 w-64 bg-white border rounded shadow-lg z-10 p-2 space-y-1"
            >
              <div v-if="assistant.mcpServers.length === 0" class="text-xs text-gray-500 p-1">
                No MCP servers registered.
              </div>
              <label
                v-for="srv in assistant.mcpServers"
                :key="srv.id"
                class="flex items-center gap-2 text-xs p-1 hover:bg-gray-50 rounded cursor-pointer"
                :class="srv.enabled ? '' : 'opacity-50'"
              >
                <input
                  type="checkbox"
                  :checked="(assistant.current.enabled_mcp_server_ids ?? []).includes(srv.id)"
                  :disabled="!srv.enabled"
                  @change="toggleMcpServer(srv.id, ($event.target as HTMLInputElement).checked)"
                />
                <span class="flex-1 truncate">{{ srv.name }}</span>
                <span v-if="!srv.enabled" class="text-gray-400">(off)</span>
              </label>
            </div>
          </div>
        </div>
      </header>

      <div ref="messageBox" class="flex-1 overflow-y-auto p-4 space-y-3">
        <template v-for="m in messageList" :key="m.id">
          <div v-if="m.role === 'user'" class="flex justify-end">
            <div class="max-w-2xl whitespace-pre-wrap rounded-lg px-3 py-2 bg-blue-600 text-white">
              {{ messageText(m.content) }}
            </div>
          </div>
          <div v-else-if="m.role === 'assistant'" class="space-y-1">
            <div
              v-if="messageText(m.content)"
              class="max-w-2xl whitespace-pre-wrap rounded-lg px-3 py-2 bg-gray-100 text-gray-900"
            >
              {{ messageText(m.content) }}
            </div>
            <div
              v-for="tc in toolCalls(m.content)"
              :key="tc.id"
              class="text-xs border-l-2 pl-2 ml-2 font-mono space-y-1"
              :class="
                decisionFor(m.content, tc.id) === false
                  ? 'border-red-300 text-red-700'
                  : decisionFor(m.content, tc.id) === true
                  ? 'border-emerald-300 text-emerald-700'
                  : 'border-amber-300 text-gray-500'
              "
            >
              <div>→ {{ tc.name }}({{ JSON.stringify(tc.args) }})</div>
              <div
                v-if="requiresApproval(m.content) && decisionFor(m.content, tc.id) === undefined"
                class="flex gap-2 not-italic"
              >
                <button
                  class="px-2 py-0.5 rounded bg-emerald-600 text-white text-xs hover:bg-emerald-500"
                  :disabled="assistant.sending"
                  @click="decide(m.id, tc.id, true)"
                >
                  Approve
                </button>
                <button
                  class="px-2 py-0.5 rounded border border-emerald-600 text-emerald-700 text-xs hover:bg-emerald-50"
                  :disabled="assistant.sending"
                  :title="`Always allow ${tc.name} — creates a permission rule`"
                  @click="decide(m.id, tc.id, true, true)"
                >
                  Always allow
                </button>
                <button
                  class="px-2 py-0.5 rounded bg-red-600 text-white text-xs hover:bg-red-500"
                  :disabled="assistant.sending"
                  @click="decide(m.id, tc.id, false)"
                >
                  Reject
                </button>
                <button
                  class="px-2 py-0.5 rounded border border-red-600 text-red-700 text-xs hover:bg-red-50"
                  :disabled="assistant.sending"
                  :title="`Always reject ${tc.name} — creates a deny rule`"
                  @click="decide(m.id, tc.id, false, true)"
                >
                  Always reject
                </button>
              </div>
            </div>
            <div
              v-if="requiresApproval(m.content) && toolCalls(m.content).length > 1"
              class="ml-2 mt-1 flex gap-2"
            >
              <button
                class="text-xs px-2 py-0.5 rounded border border-emerald-600 text-emerald-700 hover:bg-emerald-50"
                :disabled="assistant.sending"
                @click="decideAll(m.id, toolCalls(m.content), true)"
              >
                Approve all
              </button>
              <button
                class="text-xs px-2 py-0.5 rounded border border-emerald-700 text-emerald-800 hover:bg-emerald-50"
                :disabled="assistant.sending"
                title="Always allow every tool in this batch — creates permission rules"
                @click="decideAll(m.id, toolCalls(m.content), true, true)"
              >
                Always allow all
              </button>
              <button
                class="text-xs px-2 py-0.5 rounded border border-red-600 text-red-700 hover:bg-red-50"
                :disabled="assistant.sending"
                @click="decideAll(m.id, toolCalls(m.content), false)"
              >
                Reject all
              </button>
              <button
                class="text-xs px-2 py-0.5 rounded border border-red-700 text-red-800 hover:bg-red-50"
                :disabled="assistant.sending"
                title="Always reject every tool in this batch — creates deny rules"
                @click="decideAll(m.id, toolCalls(m.content), false, true)"
              >
                Always reject all
              </button>
            </div>
          </div>
          <div v-else-if="m.role === 'tool_result'" class="text-xs ml-2">
            <details
              :open="toolResult(m.content).is_error"
              class="border-l-2 pl-2 font-mono whitespace-pre-wrap"
              :class="toolResult(m.content).is_error ? 'border-red-400 text-red-700' : 'border-emerald-400 text-gray-600'"
            >
              <summary class="cursor-pointer">
                {{ toolResult(m.content).is_error ? '✗ tool error' : '✓ tool result' }}
              </summary>
              <pre class="mt-1">{{ messageText(toolResult(m.content).output) }}</pre>
            </details>
          </div>
          <div v-else-if="m.role === 'error'" class="text-sm text-red-600">
            error: {{ messageText(m.content) }}
          </div>
        </template>
        <div v-if="assistant.sending" class="text-xs text-gray-500">thinking…</div>
      </div>

      <footer v-if="assistant.current" class="p-3 border-t">
        <form class="flex gap-2" @submit.prevent="send">
          <textarea
            v-model="draft"
            rows="2"
            class="flex-1 border rounded p-2 text-sm"
            placeholder="Type a message…  (Cmd+Enter to send)"
            :disabled="assistant.sending"
            @keydown.meta.enter.prevent="send"
            @keydown.ctrl.enter.prevent="send"
          ></textarea>
          <button
            type="submit"
            class="rounded bg-gray-800 hover:bg-gray-700 text-white px-4 py-2 text-sm disabled:opacity-50"
            :disabled="assistant.sending || draft.trim() === ''"
          >
            Send
          </button>
        </form>
      </footer>

      <div v-if="!assistant.current" class="flex-1 flex items-center justify-center text-gray-500">
        Pick a chat or start a new one.
      </div>
    </section>
  </div>
</template>
