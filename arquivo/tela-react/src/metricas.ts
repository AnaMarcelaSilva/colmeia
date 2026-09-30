export const NUCLEO = '127.0.0.1:7777'

// Contadores globais, fora do estado do React: incrementar não causa renderização.
export const metricas = { bytes: 0, quadros: 0 }

const contarQuadro = () => {
  metricas.quadros++
  requestAnimationFrame(contarQuadro)
}
requestAnimationFrame(contarQuadro)
