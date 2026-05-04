import { defineStore } from 'pinia'
import { ref } from 'vue'
import { api, apiVoid } from '../api'
import type { UserSummary } from '../types'

export const useUsersStore = defineStore('users', () => {
  const items = ref<UserSummary[]>([])

  async function load() {
    items.value = await api<UserSummary[]>('/api/users')
  }

  async function create(username: string, password: string): Promise<UserSummary> {
    const created = await api<UserSummary>('/api/users', {
      method: 'POST',
      body: JSON.stringify({ username, password }),
    })
    items.value.push(created)
    return created
  }

  async function changePassword(id: number, password: string) {
    await apiVoid(`/api/users/${id}/password`, {
      method: 'PUT',
      body: JSON.stringify({ password }),
    })
  }

  async function remove(id: number) {
    await apiVoid(`/api/users/${id}`, { method: 'DELETE' })
    items.value = items.value.filter((u) => u.id !== id)
  }

  return { items, load, create, changePassword, remove }
})
