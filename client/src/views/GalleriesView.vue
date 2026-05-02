<script setup lang="ts">
import { onMounted } from 'vue'
import { useGalleriesStore } from '../stores/galleries'

const galleries = useGalleriesStore()
onMounted(galleries.load)

async function remove(id: number, title: string) {
  if (!confirm(`Delete gallery "${title}"?`)) return
  await galleries.remove(id)
}
</script>

<template>
  <div class="space-y-4">
    <div class="flex items-center justify-between">
      <h1 class="text-xl font-semibold">Galleries</h1>
      <router-link
        to="/galleries/new"
        class="rounded bg-gray-800 hover:bg-gray-700 text-white px-3 py-1.5 text-sm"
      >
        New gallery
      </router-link>
    </div>
    <div class="bg-white rounded-lg shadow overflow-x-auto">
      <table class="min-w-full text-sm">
        <thead class="bg-gray-100 text-gray-600">
          <tr>
            <th class="text-left px-4 py-2">Title</th>
            <th class="text-left px-4 py-2">Description</th>
            <th class="text-left px-4 py-2">Files</th>
            <th class="px-4 py-2"></th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="g in galleries.items" :key="g.id" class="border-t border-gray-100">
            <td class="px-4 py-2">
              <router-link :to="`/galleries/${g.id}/edit`" class="text-blue-600 hover:underline">
                {{ g.title }}
              </router-link>
            </td>
            <td class="px-4 py-2 text-gray-600">{{ g.description || '—' }}</td>
            <td class="px-4 py-2 text-gray-600">{{ g.file_ids.length }}</td>
            <td class="px-4 py-2 text-right">
              <button class="text-red-600 hover:underline" @click="remove(g.id, g.title)">
                Delete
              </button>
            </td>
          </tr>
          <tr v-if="galleries.items.length === 0">
            <td colspan="4" class="px-4 py-6 text-center text-gray-400">No galleries yet.</td>
          </tr>
        </tbody>
      </table>
    </div>
  </div>
</template>
