package terminal

import "bytes"

// Motivos de um agente esperar você. São sempre um destes textos fixos: o
// conteúdo do terminal nunca sai daqui, nem para os eventos nem para o log.
const (
	PedeAprovacao     = "pede aprovação"
	EsperandoResposta = "esperando resposta"
)

// Padrões dos pedidos de aprovação de cada ferramenta, procurados no fim da
// tela já sem as sequências de cor. É heurística: muda quando a ferramenta
// muda o texto, por isso fica isolada aqui, com testes.
var padroesAprovacao = map[string][][]byte{
	"claude":   {[]byte("Do you want to"), []byte("❯ 1. Yes"), []byte("Esc to cancel"), []byte("(y/n)")},
	"codex":    {[]byte("Allow command"), []byte("Approve"), []byte("(y/n)"), []byte("[y/N]")},
	"gemini":   {[]byte("Allow execution"), []byte("Apply this change"), []byte("(y/n)")},
	"opencode": {[]byte("Allow"), []byte("(y/n)")},
}

// Só o fim da tela importa. A sessão já entrega só o que veio depois da sua
// última escrita (veja Sessao.ultimos); isto limita ao que cabe no fim da tela.
const fimDaTela = 1200

// detectar procura um pedido de aprovação na saída recente da ferramenta.
func detectar(ferramenta string, recente []byte) string {
	padroes := padroesAprovacao[ferramenta]
	if len(padroes) == 0 {
		return ""
	}
	texto := semSequencias(recente)
	if len(texto) > fimDaTela {
		texto = texto[len(texto)-fimDaTela:]
	}
	for _, p := range padroes {
		if bytes.Contains(texto, p) {
			return PedeAprovacao
		}
	}
	return ""
}

// semSequencias tira as sequências de escape (cores, cursor, títulos).
func semSequencias(b []byte) []byte {
	saida := make([]byte, 0, len(b))
	for i := 0; i < len(b); i++ {
		c := b[i]
		if c != 0x1b {
			if c >= ' ' || c == '\n' {
				saida = append(saida, c)
			}
			continue
		}
		if i+1 >= len(b) {
			break
		}
		i++
		switch b[i] {
		case '[': // CSI: até uma letra final
			for i+1 < len(b) && !(b[i+1] >= 0x40 && b[i+1] <= 0x7e) {
				i++
			}
			i++
		case ']': // OSC: até BEL ou ESC \
			for i+1 < len(b) && b[i+1] != 0x07 && b[i+1] != 0x1b {
				i++
			}
			i++
			if i < len(b) && b[i] == 0x1b {
				i++
			}
		}
	}
	return saida
}
