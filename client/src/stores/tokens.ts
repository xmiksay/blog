import { defineStore } from 'pinia'
import { ref } from 'vue'
import { api, apiVoid } from '../api'
import type { TokenCreated, TokenSummary } from '../types'

export const useTokensStore = defineStore('tokens', () => {
  const items = ref<TokenSummary[]>([])

  async function load() {
    items.value = await api<TokenSummary[]>('/api/tokens')
  }

  async function create(label: string | null): Promise<TokenCreated> {
    const created = await api<TokenCreated>('/api/tokens', {
      method: 'POST',
      body: JSON.stringify({ label }),
    })
    items.value.unshift({
      id: created.id,
      label: created.label,
      is_service: created.is_service,
      expires_at: created.expires_at,
    })
    return created
  }

  async function remove(id: number) {
    await apiVoid(`/api/tokens/${id}`, { method: 'DELETE' })
    items.value = items.value.filter((t) => t.id !== id)
  }

  return { items, load, create, remove }
})
