import { api } from '../api'

export type PathNamespace = 'page' | 'gallery' | 'file' | 'all'

export interface FolderEntry {
  name: string
  page_count: number
  gallery_count: number
  file_count: number
}

export interface LeafEntry {
  name: string
  namespace: 'page' | 'gallery' | 'file'
  title?: string
}

export interface PathChildren {
  prefix: string
  folders: FolderEntry[]
  leaves: LeafEntry[]
}

export const pathsApi = {
  async children(input: {
    namespace?: PathNamespace
    prefix?: string
    limit?: number
  }): Promise<PathChildren> {
    return await api<PathChildren>('/api/paths/children', {
      method: 'POST',
      body: JSON.stringify({
        namespace: input.namespace ?? 'all',
        prefix: input.prefix ?? '',
        limit: input.limit,
      }),
    })
  },
}
