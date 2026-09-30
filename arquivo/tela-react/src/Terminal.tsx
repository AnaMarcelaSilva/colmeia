import { useEffect, useRef } from 'react'
import { Terminal as XTerm } from '@xterm/xterm'
import { FitAddon } from '@xterm/addon-fit'
import { WebglAddon } from '@xterm/addon-webgl'
import '@xterm/xterm/css/xterm.css'
import { NUCLEO, metricas } from './metricas'

const CONFIRMAR_A_CADA = 64 * 1024
// ?semwebgl na URL (ou VITE_RENDERIZADOR=dom no build) força o renderizador padrão, para comparar.
const usarWebgl =
  import.meta.env.VITE_RENDERIZADOR !== 'dom' && !new URLSearchParams(location.search).has('semwebgl')

const tema = {
  background: '#0f1115',
  foreground: '#d7dae0',
  cursor: '#c792ea',
  selectionBackground: '#3a3f4b',
  scrollbarSliderBackground: 'rgba(138, 144, 160, 0.25)',
  scrollbarSliderHoverBackground: 'rgba(138, 144, 160, 0.4)',
  scrollbarSliderActiveBackground: 'rgba(138, 144, 160, 0.55)',
  overviewRulerBorder: '#0f1115',
}

export function Terminal({ id }: { id: number }) {
  const caixa = useRef<HTMLDivElement>(null)

  useEffect(() => {
    const term = new XTerm({
      fontSize: 12,
      fontFamily: '"JetBrains Mono", "DejaVu Sans Mono", monospace',
      scrollback: 1000,
      theme: tema,
    })
    const ajuste = new FitAddon()
    term.loadAddon(ajuste)
    term.open(caixa.current!)
    if (usarWebgl) try {
      const webgl = new WebglAddon()
      webgl.onContextLoss(() => webgl.dispose())
      term.loadAddon(webgl)
    } catch {
      // sem WebGL: o xterm.js continua com o renderizador padrão
    }

    const ws = new WebSocket(`ws://${NUCLEO}/terminal?id=${id}`)
    ws.binaryType = 'arraybuffer'
    const enviarTamanho = () => {
      if (ws.readyState === WebSocket.OPEN) ws.send(JSON.stringify({ cols: term.cols, rows: term.rows }))
    }
    ws.onopen = enviarTamanho
    // Controle de fluxo: confirma ao núcleo o que o xterm.js já processou.
    let desenhados = 0
    ws.onmessage = (e) => {
      const dados = new Uint8Array(e.data as ArrayBuffer)
      metricas.bytes += dados.length
      term.write(dados, () => {
        desenhados += dados.length
        if (desenhados >= CONFIRMAR_A_CADA && ws.readyState === WebSocket.OPEN) {
          ws.send(JSON.stringify({ ack: desenhados }))
          desenhados = 0
        }
      })
    }
    const codificador = new TextEncoder()
    const digitacao = term.onData((d) => {
      if (ws.readyState === WebSocket.OPEN) ws.send(codificador.encode(d))
    })

    let quadro = 0
    const observador = new ResizeObserver(() => {
      cancelAnimationFrame(quadro)
      quadro = requestAnimationFrame(() => {
        ajuste.fit()
        enviarTamanho()
      })
    })
    observador.observe(caixa.current!)

    return () => {
      observador.disconnect()
      cancelAnimationFrame(quadro)
      digitacao.dispose()
      ws.close()
      term.dispose()
    }
  }, [id])

  return <div className="terminal" ref={caixa} />
}
