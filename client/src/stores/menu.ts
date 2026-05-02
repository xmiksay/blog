import { defineStore } from 'pinia'
import { ref } from 'vue'
import { api, apiVoid } from '../api'
import type { MenuItem } from '../types'

export interface MenuInput {
  title: string
  path: string
  markdown: string
  order_index: number
  private: boolean
}

export const useMenuStore = defineStore('menu', () => {
  const items = ref<MenuItem[]>([])

  async function load() {
    items.value = await api<MenuItem[]>('/api/menu')
  }

  async function create(input: MenuInput) {
    const created = await api<MenuItem>('/api/menu', {
      method: 'POST',
      body: JSON.stringify(input),
    })
    items.value.push(created)
    items.value.sort((a, b) => a.order_index - b.order_index)
  }

  async function update(id: number, input: MenuInput) {
    const updated = await api<MenuItem>(`/api/menu/${id}`, {
      method: 'PUT',
      body: JSON.stringify(input),
    })
    const idx = items.value.findIndex((m) => m.id === id)
    if (idx !== -1) items.value[idx] = updated
    items.value.sort((a, b) => a.order_index - b.order_index)
  }

  async function remove(id: number) {
    await apiVoid(`/api/menu/${id}`, { method: 'DELETE' })
    items.value = items.value.filter((m) => m.id !== id)
  }

  return { items, load, create, update, remove }
})
