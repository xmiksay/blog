import { defineStore } from 'pinia'
import { ref, computed } from 'vue'
import { api, apiVoid, ApiError } from '../api'

interface User {
  user_id: number
  username: string
}

export const useAuthStore = defineStore('auth', () => {
  const user = ref<User | null>(null)
  const checked = ref(false)

  const isLoggedIn = computed(() => user.value !== null)

  async function checkSession() {
    try {
      user.value = await api<User>('/api/auth/me')
    } catch (e) {
      if (e instanceof ApiError && e.status === 401) {
        user.value = null
      } else {
        throw e
      }
    } finally {
      checked.value = true
    }
  }

  async function login(username: string, password: string) {
    user.value = await api<User>('/api/auth/login', {
      method: 'POST',
      body: JSON.stringify({ username, password }),
    })
  }

  async function logout() {
    await apiVoid('/api/auth/logout', { method: 'POST' })
    user.value = null
  }

  return { user, checked, isLoggedIn, checkSession, login, logout }
})
