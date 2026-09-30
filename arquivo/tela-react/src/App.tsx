import { useEffect, useState } from 'react'
import { Terminal } from './Terminal'
import { NUCLEO, metricas } from './metricas'
import './app.css'

const papeis = ['líder', 'dev', 'dev', 'revisor', 'testador', 'dev', 'dev', 'revisor', 'testador', 'dev']
const projetos = ['loja-web', 'api-pedidos', 'estudos-rust']
type Carga = 'parada' | 'leve' | 'pesada'

function formatarVazao(bytesPorSegundo: number) {
  if (bytesPorSegundo >= 1024 * 1024) return `${(bytesPorSegundo / 1024 / 1024).toFixed(1)} MB/s`
  if (bytesPorSegundo >= 1024) return `${(bytesPorSegundo / 1024).toFixed(0)} KB/s`
  return `${bytesPorSegundo} B/s`
}

function Medidor() {
  const [leitura, setLeitura] = useState({ fps: 0, vazao: 0 })
  useEffect(() => {
    let bytes = metricas.bytes
    let quadros = metricas.quadros
    const t = setInterval(() => {
      setLeitura({ fps: metricas.quadros - quadros, vazao: metricas.bytes - bytes })
      bytes = metricas.bytes
      quadros = metricas.quadros
    }, 1000)
    return () => clearInterval(t)
  }, [])
  return (
    <div className="medidor">
      <span><b>{leitura.fps}</b> FPS</span>
      <span><b>{formatarVazao(leitura.vazao)}</b> recebidos</span>
    </div>
  )
}

export default function App() {
  const [carga, setCarga] = useState<Carga>('parada')

  const aplicarCarga = (modo: Carga) => {
    setCarga(modo)
    fetch(`http://${NUCLEO}/carga?modo=${modo}`).catch(() => {})
  }

  return (
    <div className="app">
      <aside className="lateral">
        <div className="marca">Protótipo</div>
        <div className="grupo">Profissional</div>
        <div className="subgrupo">Empresa X</div>
        {projetos.map((p, i) => (
          <div key={p} className={`projeto ${i === 0 ? 'ativo' : ''}`}>{p}</div>
        ))}
        <div className="projeto novo">+ Novo projeto</div>
      </aside>

      <main className="principal">
        <header className="topo">
          <div className="caminho">
            Profissional <span>›</span> Empresa X <span>›</span> <b>loja-web</b>
            <button className="chip">branch: todas ▾</button>
          </div>
          <Medidor />
          <div className="cargas">
            {(['parada', 'leve', 'pesada'] as Carga[]).map((m) => (
              <button key={m} className={carga === m ? 'ligado' : ''} onClick={() => aplicarCarga(m)}>
                {m === 'parada' ? 'Parar' : `Carga ${m}`}
              </button>
            ))}
          </div>
        </header>

        <section className="tarefa">
          <h1>Nova tela de pedidos</h1>
          <span className="coluna">Agente trabalhando</span>
          <span className="stack">React + xterm.js</span>
        </section>

        <section className="grade">
          {papeis.map((papel, id) => (
            <div key={id} className="cartao">
              <div className="cabecalho">
                <i className={carga === 'parada' ? 'ocioso' : 'ativo'} />
                agente-{id} <span>· {papel}</span>
              </div>
              <Terminal id={id} />
            </div>
          ))}
        </section>
      </main>
    </div>
  )
}
