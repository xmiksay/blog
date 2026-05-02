<script setup lang="ts">
import { onMounted } from 'vue'
import { useFilesStore } from '../stores/files'
import FileUploader from '../components/FileUploader.vue'

const files = useFilesStore()
onMounted(() => files.load())

async function remove(id: number, title: string) {
  if (!confirm(`Delete "${title}"?`)) return
  await files.remove(id)
}
</script>

<template>
  <div class="space-y-4">
    <div class="flex items-center justify-between">
      <h1 class="text-xl font-semibold">Files</h1>
    </div>
    <FileUploader />
    <div class="grid grid-cols-2 sm:grid-cols-3 lg:grid-cols-4 gap-3">
      <div
        v-for="f in files.items"
        :key="f.id"
        class="bg-white rounded shadow overflow-hidden"
      >
        <div class="aspect-square bg-gray-100 flex items-center justify-center">
          <img
            v-if="f.has_thumbnail"
            :src="`/obrazky/${f.id}/nahled`"
            :alt="f.title"
            class="object-cover w-full h-full"
            loading="lazy"
          />
          <div v-else class="text-xs text-gray-400 p-2 text-center break-all">
            {{ f.mimetype }}
          </div>
        </div>
        <div class="p-2 text-sm">
          <div class="truncate font-medium" :title="f.title">{{ f.title }}</div>
          <div class="text-xs text-gray-500 truncate">{{ formatSize(f.size_bytes) }}</div>
          <div class="mt-2 flex justify-between text-xs">
            <router-link :to="`/files/${f.id}/edit`" class="text-blue-600 hover:underline">
              Edit
            </router-link>
            <button class="text-red-600 hover:underline" @click="remove(f.id, f.title)">
              Delete
            </button>
          </div>
        </div>
      </div>
      <p v-if="files.items.length === 0" class="text-gray-400 col-span-full">No files yet.</p>
    </div>
  </div>
</template>

<script lang="ts">
function formatSize(n: number): string {
  if (n < 1024) return `${n} B`
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`
  return `${(n / 1024 / 1024).toFixed(1)} MB`
}
export default { name: 'FilesView' }
</script>
