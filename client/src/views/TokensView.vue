<script setup lang="ts">
import { onMounted, ref } from 'vue'
import { useTokensStore } from '../stores/tokens'

const tokens = useTokensStore()
onMounted(tokens.load)

const newLabel = ref('')
const justCreated = ref<{ id: number; nonce: string } | null>(null)

async function create() {
  const created = await tokens.create(newLabel.value || null)
  justCreated.value = { id: created.id, nonce: created.nonce }
  newLabel.value = ''
}

async function remove(id: number, label: string | null) {
  if (!confirm(`Revoke token "${label ?? id}"?`)) return
  await tokens.remove(id)
  if (justCreated.value?.id === id) justCreated.value = null
}
</script>

<template>
  <div class="space-y-4">
    <h1 class="text-xl font-semibold">Service tokens</h1>

    <form class="bg-white shadow rounded p-3 flex gap-2 items-end" @submit.prevent="create">
      <label class="flex-1">
        <span class="text-xs text-gray-500">Label</span>
        <input v-model="newLabel" class="mt-1 w-full rounded border border-gray-300 px-2 py-1.5" />
      </label>
      <button class="rounded bg-gray-800 hover:bg-gray-700 text-white px-3 py-1.5 text-sm">
        Create
      </button>
    </form>

    <div
      v-if="justCreated"
      class="bg-yellow-50 border border-yellow-300 rounded p-3 text-sm"
    >
      <p class="font-medium mb-1">New token (copy now — this is the only time you'll see it):</p>
      <code class="block bg-white p-2 rounded break-all">{{ justCreated.nonce }}</code>
    </div>

    <div class="bg-white rounded-lg shadow overflow-x-auto">
      <table class="min-w-full text-sm">
        <thead class="bg-gray-100 text-gray-600">
          <tr>
            <th class="text-left px-4 py-2">Label</th>
            <th class="text-left px-4 py-2">Expires</th>
            <th class="px-4 py-2"></th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="t in tokens.items" :key="t.id" class="border-t border-gray-100">
            <td class="px-4 py-2">{{ t.label || `#${t.id}` }}</td>
            <td class="px-4 py-2 text-gray-500">{{ t.expires_at || 'never' }}</td>
            <td class="px-4 py-2 text-right">
              <button class="text-red-600 hover:underline" @click="remove(t.id, t.label)">
                Revoke
              </button>
            </td>
          </tr>
          <tr v-if="tokens.items.length === 0">
            <td colspan="3" class="px-4 py-6 text-center text-gray-400">No service tokens.</td>
          </tr>
        </tbody>
      </table>
    </div>
  </div>
</template>
