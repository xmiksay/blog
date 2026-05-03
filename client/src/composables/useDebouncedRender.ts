import { ref, watch, type Ref } from 'vue'
import { markdownApi } from '../api/markdown'

export function useDebouncedRender(
  source: Ref<string>,
  active: Ref<boolean>,
  delayMs = 400,
) {
  const html = ref('')
  const loading = ref(false)
  const error = ref('')
  let handle: number | null = null
  let seq = 0

  async function run(md: string) {
    const mySeq = ++seq
    loading.value = true
    error.value = ''
    try {
      const out = await markdownApi.render(md)
      if (mySeq === seq) html.value = out
    } catch (e) {
      if (mySeq === seq) error.value = e instanceof Error ? e.message : 'Render failed'
    } finally {
      if (mySeq === seq) loading.value = false
    }
  }

  watch(
    [source, active],
    ([md, on]) => {
      if (!on) return
      if (handle !== null) window.clearTimeout(handle)
      handle = window.setTimeout(() => run(md), delayMs)
    },
    { immediate: true },
  )

  return { html, loading, error }
}
