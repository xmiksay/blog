import { api } from '../api'

export const markdownApi = {
  async render(markdown: string): Promise<string> {
    const res = await api<{ html: string }>('/api/markdown/render', {
      method: 'POST',
      body: JSON.stringify({ markdown }),
    })
    return res.html
  },
}
