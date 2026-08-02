<script setup lang="ts">
import { computed, nextTick, onMounted, ref, watch } from 'vue'
import { useAssistantStore } from '../stores/assistant'
import { renderMarkdown } from '../composables/useMarkdown'
import AssistantMessageContent from '../components/AssistantMessageContent.vue'
import LiveToolCallList from '../components/LiveToolCallList.vue'
import LiveSubAgentTurnCard from '../components/LiveSubAgentTurn.vue'
import AssistantSessionToolbar from '../components/AssistantSessionToolbar.vue'
import AssistantSessionTree from '../components/AssistantSessionTree.vue'
import { firstRootSession } from '../composables/useSessionTree'

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
  // The first *root*, not `sessions[0]`: since #99 a sub-agent is a session
  // row too, so the API's first entry can be a child — opening one by default
  // would land the user in a read-only sub-transcript.
  const first = firstRootSession(assistant.sessions)
  if (first) await select(first.id)
})

async function newSession() {
  if (assistant.models.length === 0) {
    alert('Add a provider and a model first under "LLM providers" / "LLM models".')
    return
  }
  const s = await assistant.createSession()
  await select(s.id)
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
  if (!assistant.current) {
    const next = firstRootSession(assistant.sessions)
    if (next) await select(next.id)
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

// The turn streaming live over WS for the open session, if any — see
// `LiveTurn`'s doc in types.ts. `null` once it settles (`done`/`error`),
// at which point `messageList` (from the REST refetch `stores/assistant.ts`
// triggers) is the authoritative view again.
const liveTurn = computed(() => {
  const turn = assistant.live
  return turn && assistant.current?.id === turn.sessionId ? turn : null
})

watch(() => [liveTurn.value?.text, liveTurn.value?.toolCalls.length], scrollToBottom)

// Sub-agents (`researcher`/`page-writer`) currently streaming for the open
// session — see `LiveSubAgentTurn`'s doc in types.ts. Filtered the same way
// `liveTurn` is (by the currently open session's id), since a child can
// outlive the root's own `live` turn and keeps its entry independently.
// `dbSessionId` is the **root's** row, so matching `childDbSessionId` too
// (#102) is what keeps the child's *own* session view live instead of blank
// until its turn settles and the REST refetch lands.
const liveSubAgentsForCurrent = computed(() => {
  const openId = assistant.current?.id
  // Explicit null-check: `childDbSessionId` is optional, so a bare
  // `=== assistant.current?.id` would match every card once nothing is open.
  if (openId == null) return []
  return Object.values(assistant.liveSubAgents).filter(
    (turn) => turn.dbSessionId === openId || turn.childDbSessionId === openId,
  )
})

watch(
  () => liveSubAgentsForCurrent.value.map((t) => t.text.length + t.toolCalls.length).join(','),
  scrollToBottom,
)
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
      <AssistantSessionTree
        :sessions="assistant.sessions"
        :current-id="assistant.current?.id ?? null"
        @select="select"
        @delete="deleteSession"
      />
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
        <AssistantSessionToolbar @compacted="scrollToBottom" />
      </header>

      <div ref="messageBox" class="flex-1 overflow-y-auto p-4 space-y-3">
        <AssistantMessageContent
          v-for="m in messageList"
          :key="m.id"
          :role="m.role"
          :content="m.content"
          :message-id="m.id"
          @decided="scrollToBottom"
          @select-session="select"
        />
        <div v-if="liveTurn" class="space-y-1">
          <div
            v-if="liveTurn.retrying"
            class="inline-flex items-center gap-1 rounded-full bg-amber-100 text-amber-800 text-xs px-2 py-0.5"
          >
            model stalled — retrying…
          </div>
          <div
            v-if="liveTurn.reasoning"
            class="max-w-2xl rounded-lg px-3 py-2 bg-gray-50 text-gray-500 text-xs italic whitespace-pre-wrap"
          >
            {{ liveTurn.reasoning }}
          </div>
          <div
            v-if="liveTurn.text"
            class="assistant-markdown max-w-2xl rounded-lg px-3 py-2 bg-gray-100 text-gray-900"
            v-html="renderMarkdown(liveTurn.text)"
          ></div>
          <LiveToolCallList
            :tool-calls="liveTurn.toolCalls"
            :session-id="liveTurn.sessionId"
            @decided="scrollToBottom"
          />
        </div>
        <LiveSubAgentTurnCard
          v-for="turn in liveSubAgentsForCurrent"
          :key="turn.agentSessionId"
          :turn="turn"
          @decided="scrollToBottom"
        />
        <div v-if="assistant.sending && !liveTurn" class="text-xs text-gray-500">thinking…</div>
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
