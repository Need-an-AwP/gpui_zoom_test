import { useEffect, useState } from 'react'
import init from './wasm/gpui_zoom_test.js'

let wasmInitPromise: Promise<unknown> | null = null

function initGpuiWasm() {
  if (!wasmInitPromise) {
    wasmInitPromise = init()
  }
  return wasmInitPromise
}

export function App() {
  const [status, setStatus] = useState<'loading' | 'ready' | 'error'>('loading')
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    let cancelled = false

    initGpuiWasm()
      .then(() => {
        if (!cancelled) {
          setStatus('ready')
        }
      })
      .catch((err: unknown) => {
        if (!cancelled) {
          setStatus('error')
          setError(err instanceof Error ? err.message : String(err))
        }
      })

    return () => {
      cancelled = true
    }
  }, [])

  if (status === 'ready') {
    return null
  }

  return (
    <div className="wasm-status">
      {status === 'loading' ? (
        <span>Loading GPUI WASM...</span>
      ) : (
        <>
          <span>Failed to initialize GPUI WASM</span>
          {error && <pre className="wasm-error">{error}</pre>}
        </>
      )}
    </div>
  )
}

export default App
