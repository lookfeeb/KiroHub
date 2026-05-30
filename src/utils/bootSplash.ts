interface BootSplashElementLike {
  dataset: Record<string, unknown>
  remove: () => void
}

interface BootSplashDocumentLike {
  getElementById?: (id: string) => BootSplashElementLike | null
}

export function dismissBootSplash(doc: BootSplashDocumentLike = document): boolean {
  const splash = doc?.getElementById?.('boot-splash')
  if (!splash) return false

  splash.dataset.state = 'hidden'
  splash.remove()
  return true
}
