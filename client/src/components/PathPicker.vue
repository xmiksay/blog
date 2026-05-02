<script setup lang="ts">
import { ref, computed } from 'vue'
import {
  pathsApi,
  type FolderEntry,
  type LeafEntry,
  type PathNamespace,
} from '../api/paths'

const props = withDefaults(
  defineProps<{
    modelValue: string
    namespace?: PathNamespace
    placeholder?: string
    readonly?: boolean
    id?: string
  }>(),
  {
    namespace: 'all',
    placeholder: 'section/page',
    readonly: false,
  },
)

const emit = defineEmits<{
  (e: 'update:modelValue', value: string): void
}>()

defineOptions({ name: 'PathPicker' })

const value = computed({
  get: () => props.modelValue,
  set: (v: string) => emit('update:modelValue', v),
})

interface Suggestion {
  name: string
  isFolder: boolean
  folder?: FolderEntry
  leaf?: LeafEntry
}

const inputEl = ref<HTMLInputElement | null>(null)
const open = ref(false)
const cursor = ref(0)
const suggestions = ref<Suggestion[]>([])
let fetchSeq = 0
let debounceHandle: number | null = null

function splitAtLastSlash(v: string): [string, string] {
  const i = v.lastIndexOf('/')
  if (i < 0) return ['', v]
  return [v.slice(0, i + 1), v.slice(i + 1)]
}

const noMatchHint = computed(() => {
  if (!open.value || suggestions.value.length > 0) return null
  const [, tail] = splitAtLastSlash(value.value)
  if (!tail) return null
  return `No match — "${tail}" will be a new entry`
})

function scheduleFetch() {
  if (debounceHandle !== null) window.clearTimeout(debounceHandle)
  debounceHandle = window.setTimeout(fetchSuggestions, 150)
}

async function fetchSuggestions() {
  if (props.readonly) return
  const [head, tail] = splitAtLastSlash(value.value)
  const seq = ++fetchSeq
  try {
    const res = await pathsApi.children({
      namespace: props.namespace,
      prefix: head,
      limit: 200,
    })
    if (seq !== fetchSeq) return
    const tailLower = tail.toLowerCase()
    const list: Suggestion[] = []
    for (const f of res.folders) {
      if (!tail || f.name.toLowerCase().startsWith(tailLower)) {
        list.push({ name: f.name, isFolder: true, folder: f })
      }
    }
    for (const l of res.leaves) {
      if (!tail || l.name.toLowerCase().startsWith(tailLower)) {
        list.push({ name: l.name, isFolder: false, leaf: l })
      }
    }
    suggestions.value = list
    cursor.value = 0
  } catch {
    suggestions.value = []
  }
}

function onInput() {
  open.value = true
  scheduleFetch()
}

function onFocus() {
  open.value = true
  fetchSuggestions()
}

function onBlur() {
  window.setTimeout(() => {
    open.value = false
  }, 150)
}

function applySuggestion(idx: number) {
  const s = suggestions.value[idx]
  if (!s) return
  const [head] = splitAtLastSlash(value.value)
  if (s.isFolder) {
    value.value = head + s.name + '/'
    cursor.value = 0
    scheduleFetch()
  } else {
    value.value = head + s.name
    open.value = false
  }
}

function onKeydown(e: KeyboardEvent) {
  if (!open.value && (e.key === 'ArrowDown' || e.key === 'Tab')) {
    open.value = true
    fetchSuggestions()
    if (e.key === 'Tab') return
  }
  if (!open.value || suggestions.value.length === 0) return
  switch (e.key) {
    case 'ArrowDown':
      e.preventDefault()
      cursor.value = (cursor.value + 1) % suggestions.value.length
      break
    case 'ArrowUp':
      e.preventDefault()
      cursor.value =
        (cursor.value - 1 + suggestions.value.length) % suggestions.value.length
      break
    case 'Tab':
    case 'Enter':
      e.preventDefault()
      applySuggestion(cursor.value)
      break
    case 'Escape':
      open.value = false
      break
  }
}

const modalOpen = ref(false)
const browsePrefix = ref('')
const browseFolders = ref<FolderEntry[]>([])
const browseLeaves = ref<LeafEntry[]>([])
const browseLoading = ref(false)

const breadcrumb = computed(() => {
  const items: { label: string; prefix: string }[] = [
    { label: '/ root', prefix: '' },
  ]
  if (!browsePrefix.value) return items
  const parts = browsePrefix.value.split('/').filter(Boolean)
  let acc = ''
  for (const p of parts) {
    acc += p + '/'
    items.push({ label: p, prefix: acc })
  }
  return items
})

async function loadBrowse(prefix: string) {
  browseLoading.value = true
  browsePrefix.value = prefix
  try {
    const res = await pathsApi.children({
      namespace: props.namespace,
      prefix,
      limit: 500,
    })
    browseFolders.value = res.folders
    browseLeaves.value = res.leaves
  } catch {
    browseFolders.value = []
    browseLeaves.value = []
  } finally {
    browseLoading.value = false
  }
}

function openBrowse() {
  modalOpen.value = true
  loadBrowse('')
}

function browseDrill(folder: FolderEntry) {
  loadBrowse(browsePrefix.value + folder.name + '/')
}

function browsePickFolder() {
  value.value = browsePrefix.value
  modalOpen.value = false
  window.setTimeout(() => {
    inputEl.value?.focus()
    open.value = true
    fetchSuggestions()
  }, 0)
}

function browsePickLeaf(leaf: LeafEntry) {
  value.value = browsePrefix.value + leaf.name
  modalOpen.value = false
}
</script>

<template>
  <div class="relative">
    <div class="flex gap-2">
      <input
        ref="inputEl"
        v-model="value"
        type="text"
        class="flex-1 rounded border border-gray-300 px-2 py-1.5"
        :placeholder="placeholder"
        :readonly="readonly"
        :id="id"
        autocomplete="off"
        spellcheck="false"
        @input="onInput"
        @focus="onFocus"
        @blur="onBlur"
        @keydown="onKeydown"
      />
      <button
        v-if="!readonly"
        type="button"
        class="rounded border border-gray-300 px-3 py-1.5 text-sm hover:border-gray-500"
        title="Browse existing folders"
        @click="openBrowse"
      >
        Browse
      </button>
    </div>

    <ul
      v-if="open && !readonly && (suggestions.length > 0 || noMatchHint)"
      class="absolute left-0 right-0 z-20 mt-1 max-h-72 overflow-y-auto rounded border border-gray-300 bg-white py-1 shadow-lg"
    >
      <li
        v-for="(s, i) in suggestions"
        :key="(s.isFolder ? 'f:' : 'l:') + s.name"
        class="grid cursor-pointer grid-cols-[1.2rem_1fr_auto] items-center gap-2 px-2 py-1 text-sm"
        :class="{ 'bg-gray-100': i === cursor }"
        @mousedown.prevent="applySuggestion(i)"
        @mouseenter="cursor = i"
      >
        <span class="text-center text-gray-400">{{ s.isFolder ? '▸' : '·' }}</span>
        <span class="truncate">
          {{ s.name }}<span v-if="s.isFolder" class="text-gray-400">/</span>
        </span>
        <span class="flex items-center gap-1 whitespace-nowrap text-xs text-gray-500">
          <template v-if="s.isFolder">
            <span
              v-if="s.folder!.page_count"
              class="rounded-full border border-gray-300 px-1.5 text-[0.65rem] uppercase"
            >
              p {{ s.folder!.page_count }}
            </span>
            <span
              v-if="s.folder!.gallery_count"
              class="rounded-full border border-gray-300 px-1.5 text-[0.65rem] uppercase"
            >
              g {{ s.folder!.gallery_count }}
            </span>
            <span
              v-if="s.folder!.file_count"
              class="rounded-full border border-gray-300 px-1.5 text-[0.65rem] uppercase"
            >
              f {{ s.folder!.file_count }}
            </span>
          </template>
          <template v-else>
            <span class="rounded-full border border-gray-300 px-1.5 text-[0.65rem] uppercase">
              {{ s.leaf!.namespace }}
            </span>
            <span v-if="s.leaf!.title" class="text-gray-400">{{ s.leaf!.title }}</span>
          </template>
        </span>
      </li>
      <li
        v-if="suggestions.length === 0 && noMatchHint"
        class="px-2 py-1 text-sm italic text-gray-500"
      >
        {{ noMatchHint }}
      </li>
    </ul>

    <div
      v-if="modalOpen"
      class="fixed inset-0 z-50 flex items-center justify-center bg-black/50"
      @click.self="modalOpen = false"
    >
      <div
        class="flex max-h-[80vh] w-[min(640px,92vw)] flex-col overflow-hidden rounded-lg border border-gray-200 bg-white"
        role="dialog"
        aria-label="Browse paths"
      >
        <header class="flex items-center justify-between border-b border-gray-200 px-4 py-2">
          <h3 class="text-base font-medium">Browse</h3>
          <button
            class="text-2xl leading-none text-gray-500 hover:text-gray-800"
            type="button"
            @click="modalOpen = false"
          >
            ×
          </button>
        </header>

        <nav class="flex flex-wrap items-center gap-1 border-b border-gray-200 px-4 py-2 text-sm">
          <template v-for="(c, i) in breadcrumb" :key="c.prefix">
            <button
              type="button"
              class="px-1 text-blue-600 hover:underline"
              :class="{ 'cursor-default font-semibold text-gray-800 hover:no-underline': c.prefix === browsePrefix }"
              @click="loadBrowse(c.prefix)"
            >
              {{ c.label }}
            </button>
            <span v-if="i < breadcrumb.length - 1" class="text-gray-400">/</span>
          </template>
          <button
            type="button"
            class="ml-auto rounded border border-blue-600 px-2 py-0.5 text-xs text-blue-600 hover:bg-blue-600 hover:text-white"
            :title="'Use ' + (browsePrefix || '/') + ' as the prefix'"
            @click="browsePickFolder"
          >
            Use this folder
          </button>
        </nav>

        <div class="overflow-y-auto p-2">
          <p v-if="browseLoading" class="px-2 py-2 text-sm text-gray-500">Loading…</p>
          <p
            v-else-if="browseFolders.length === 0 && browseLeaves.length === 0"
            class="px-2 py-2 text-sm text-gray-500"
          >
            Empty folder.
          </p>
          <ul v-else class="m-0 list-none p-0">
            <li
              v-for="f in browseFolders"
              :key="'f:' + f.name"
              class="grid cursor-pointer grid-cols-[1.2rem_1fr_auto] items-center gap-2 rounded px-2 py-1.5 hover:bg-gray-100"
              @click="browseDrill(f)"
            >
              <span class="text-center text-gray-400">▸</span>
              <span class="truncate">
                {{ f.name }}<span class="text-gray-400">/</span>
              </span>
              <span class="flex items-center gap-1 whitespace-nowrap text-xs text-gray-500">
                <span
                  v-if="f.page_count"
                  class="rounded-full border border-gray-300 px-1.5 text-[0.65rem] uppercase"
                >
                  p {{ f.page_count }}
                </span>
                <span
                  v-if="f.gallery_count"
                  class="rounded-full border border-gray-300 px-1.5 text-[0.65rem] uppercase"
                >
                  g {{ f.gallery_count }}
                </span>
                <span
                  v-if="f.file_count"
                  class="rounded-full border border-gray-300 px-1.5 text-[0.65rem] uppercase"
                >
                  f {{ f.file_count }}
                </span>
              </span>
            </li>
            <li
              v-for="l in browseLeaves"
              :key="'l:' + l.namespace + ':' + l.name"
              class="grid cursor-pointer grid-cols-[1.2rem_1fr_auto] items-center gap-2 rounded px-2 py-1.5 hover:bg-gray-100"
              @click="browsePickLeaf(l)"
            >
              <span class="text-center text-gray-400">·</span>
              <span class="truncate">{{ l.name }}</span>
              <span class="flex items-center gap-1 whitespace-nowrap text-xs text-gray-500">
                <span class="rounded-full border border-gray-300 px-1.5 text-[0.65rem] uppercase">
                  {{ l.namespace }}
                </span>
                <span v-if="l.title" class="text-gray-400">{{ l.title }}</span>
              </span>
            </li>
          </ul>
        </div>
      </div>
    </div>
  </div>
</template>
