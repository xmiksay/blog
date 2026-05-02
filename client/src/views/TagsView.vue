<script setup lang="ts">
import { onMounted, ref } from 'vue'
import { useTagsStore } from '../stores/tags'

const tags = useTagsStore()
onMounted(tags.load)

const newName = ref('')
const newDescription = ref('')

async function add() {
  if (!newName.value.trim()) return
  await tags.create({ name: newName.value.trim(), description: newDescription.value || null })
  newName.value = ''
  newDescription.value = ''
}

async function remove(id: number, name: string) {
  if (!confirm(`Delete tag "${name}"?`)) return
  await tags.remove(id)
}

async function rename(id: number) {
  const tag = tags.items.find((t) => t.id === id)
  if (!tag) return
  const newName = prompt('Rename tag', tag.name)
  if (!newName || newName === tag.name) return
  await tags.update(id, { name: newName, description: tag.description })
}
</script>

<template>
  <div class="space-y-4">
    <h1 class="text-xl font-semibold">Tags</h1>
    <form class="bg-white shadow rounded p-3 flex gap-2 items-end" @submit.prevent="add">
      <label class="flex-1">
        <span class="text-xs text-gray-500">Name</span>
        <input v-model="newName" class="mt-1 w-full rounded border border-gray-300 px-2 py-1.5" />
      </label>
      <label class="flex-1">
        <span class="text-xs text-gray-500">Description</span>
        <input v-model="newDescription" class="mt-1 w-full rounded border border-gray-300 px-2 py-1.5" />
      </label>
      <button class="rounded bg-gray-800 hover:bg-gray-700 text-white px-3 py-1.5 text-sm">Add</button>
    </form>
    <div class="bg-white rounded-lg shadow overflow-x-auto">
      <table class="min-w-full text-sm">
        <thead class="bg-gray-100 text-gray-600">
          <tr>
            <th class="text-left px-4 py-2">Name</th>
            <th class="text-left px-4 py-2">Description</th>
            <th class="px-4 py-2"></th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="t in tags.items" :key="t.id" class="border-t border-gray-100">
            <td class="px-4 py-2">{{ t.name }}</td>
            <td class="px-4 py-2 text-gray-600">{{ t.description || '—' }}</td>
            <td class="px-4 py-2 text-right space-x-3">
              <button class="text-blue-600 hover:underline" @click="rename(t.id)">Rename</button>
              <button class="text-red-600 hover:underline" @click="remove(t.id, t.name)">Delete</button>
            </td>
          </tr>
          <tr v-if="tags.items.length === 0">
            <td colspan="3" class="px-4 py-6 text-center text-gray-400">No tags yet.</td>
          </tr>
        </tbody>
      </table>
    </div>
  </div>
</template>
