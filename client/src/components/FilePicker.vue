<script setup lang="ts">
import { onMounted } from 'vue'
import { useFilesStore } from '../stores/files'

const props = withDefaults(
  defineProps<{ excludeIds: number[]; mimePrefix?: string }>(),
  { mimePrefix: undefined },
)
const emit = defineEmits<{ pick: [id: number]; close: [] }>()

const files = useFilesStore()
onMounted(() => files.load(props.mimePrefix))
</script>

<template>
  <div class="fixed inset-0 bg-black/40 flex items-center justify-center z-50" @click.self="emit('close')">
    <div class="bg-white rounded-lg shadow-lg max-w-3xl w-full max-h-[80vh] flex flex-col">
      <div class="flex justify-between items-center px-4 py-3 border-b border-gray-200">
        <h2 class="font-medium">Pick a file</h2>
        <button class="text-gray-500 hover:text-gray-900" @click="emit('close')">×</button>
      </div>
      <div class="overflow-auto p-3 grid grid-cols-3 sm:grid-cols-4 gap-2">
        <button
          v-for="f in files.items.filter((x) => !excludeIds.includes(x.id))"
          :key="f.id"
          class="text-left bg-gray-50 hover:bg-gray-100 rounded overflow-hidden"
          @click="emit('pick', f.id)"
        >
          <div class="aspect-square bg-gray-200 flex items-center justify-center">
            <img
              v-if="f.has_thumbnail"
              :src="`/obrazky/${f.id}/nahled`"
              :alt="f.title"
              class="object-cover w-full h-full"
              loading="lazy"
            />
            <span v-else class="text-xs text-gray-400 p-1 text-center break-all">{{ f.mimetype }}</span>
          </div>
          <div class="p-1 text-xs truncate" :title="f.title">{{ f.title }}</div>
        </button>
        <p v-if="files.items.length === 0" class="text-gray-400 col-span-full">No files yet.</p>
      </div>
    </div>
  </div>
</template>
