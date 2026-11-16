package terminal

import (
	"strings"
	"testing"
)

func desenhar(t *testing.T, tam Tamanho, partes ...string) *tela {
	t.Helper()
	e := novaTela(tam)
	for _, p := range partes {
		e.alimentar([]byte(p))
	}
	return e
}

func conferir(t *testing.T, e *tela, esperado ...string) {
	t.Helper()
	if got := e.texto(100); strings.Join(got, "\n") != strings.Join(esperado, "\n") {
		t.Errorf("tela:\n%q\nesperado:\n%q", got, esperado)
	}
}

func TestTelaDesenhaPorPosicaoComoOClaudeCode(t *testing.T) {
	// O Claude Code pula os espaços com "vá para a coluna N" e só reescreve
	// o que mudou, apagando o fim da linha.
	e := desenhar(t, Tamanho{40, 5},
		"\x1b[H\x1b[2J",
		"\r\x1b[1B\x1b[38;2;1;2;3m●\x1b[39m\x1b[3GVou\x1b[7Gprocurar\x1b[16Gonde\x1b[K",
		"\r\x1b[1B\x1b[K\r\x1b[1B\x1b[2C⎿\x1b[5GLeu 7 linhas\x1b[K",
	)
	conferir(t, e, "", "● Vou procurar onde", "", "  ⎿ Leu 7 linhas")
	// Um quadro novo só muda o que mudou: "onde" vira "o cupom".
	e.alimentar([]byte("\x1b[2;16Ho \x1b[18Gcupom\x1b[K"))
	conferir(t, e, "", "● Vou procurar o cupom", "", "  ⎿ Leu 7 linhas")
}

func TestTelaRolaComoUmShellEGuardaARolagem(t *testing.T) {
	e := desenhar(t, Tamanho{20, 3}, "um\r\ndois\r\ntrês\r\nquatro\r\ncinco")
	conferir(t, e, "um", "dois", "três", "quatro", "cinco")
	if got := e.texto(2); strings.Join(got, "|") != "quatro|cinco" {
		t.Errorf("últimas duas: %q", got)
	}
	// Uma linha mais larga que a tela quebra na coluna seguinte.
	e = desenhar(t, Tamanho{5, 3}, "abcdefgh")
	conferir(t, e, "abcde", "fgh")
}

func TestTelaProgressoReescritoComRetornoDeCarro(t *testing.T) {
	e := desenhar(t, Tamanho{30, 3}, "baixando 10%\rbaixando 100%\r\npronto")
	conferir(t, e, "baixando 100%", "pronto")
}

func TestTelaApagaInsereEMoveLinhas(t *testing.T) {
	e := desenhar(t, Tamanho{10, 4}, "a\r\nb\r\nc\r\nd")
	e.alimentar([]byte("\x1b[2;1H\x1b[M")) // apaga a linha "b"
	conferir(t, e, "a", "c", "d")
	e.alimentar([]byte("\x1b[2;1H\x1b[L")) // abre uma linha antes de "c"
	conferir(t, e, "a", "", "c", "d")
	e.alimentar([]byte("\x1b[1;1H\x1b[J")) // limpa do cursor para baixo
	conferir(t, e)
	e = desenhar(t, Tamanho{10, 2}, "abcdef\x1b[3G\x1b[2P")
	conferir(t, e, "abef")
	e = desenhar(t, Tamanho{10, 2}, "abcdef\x1b[3G\x1b[2@")
	conferir(t, e, "ab  cdef")
	e = desenhar(t, Tamanho{10, 2}, "abcdef\x1b[3G\x1b[2X")
	conferir(t, e, "ab  ef")
	e = desenhar(t, Tamanho{10, 2}, "abcdef\x1b[3G\x1b[1K")
	conferir(t, e, "   def")
}

func TestTelaRegiaoDeRolagem(t *testing.T) {
	e := desenhar(t, Tamanho{10, 4}, "topo\r\n\r\n\r\nrodapé", "\x1b[2;3r\x1b[2;1Hum\r\ndois\r\ntrês")
	conferir(t, e, "topo", "dois", "três", "rodapé")
}

func TestTelaAlternativaVoltaParaAPrincipal(t *testing.T) {
	e := desenhar(t, Tamanho{10, 3}, "antes\r\n", "\x1b[?1049h\x1b[Hmenu\x1b[?1049l")
	conferir(t, e, "antes")
	e.alimentar([]byte("\x1b[?1049h\x1b[Hmenu"))
	conferir(t, e, "menu")
}

func TestTelaCaracteresLargosEAcentos(t *testing.T) {
	e := desenhar(t, Tamanho{6, 2}, "🐝ab")
	conferir(t, e, "🐝ab")
	if e.coluna != 4 {
		t.Errorf("a abelha ocupa duas células: coluna %d", e.coluna)
	}
	// Escrever em cima da metade de um largo apaga ele inteiro.
	e.alimentar([]byte("\x1b[2Gx"))
	conferir(t, e, " xab")
	// Acento combinado não ocupa célula; o largo no fim da linha vai para a próxima.
	e = desenhar(t, Tamanho{4, 2}, "ésta🐝")
	conferir(t, e, "esta", "🐝")
}

func TestTelaSequenciaPartidaEntreLeituras(t *testing.T) {
	e := desenhar(t, Tamanho{20, 2}, "a\x1b[", "3Gb", "\xc3", "\xa9")
	conferir(t, e, "a bé")
	// Um título (OSC) partido também não vaza para a tela.
	e = desenhar(t, Tamanho{20, 2}, "x\x1b]0;tít", "ulo\x07y")
	conferir(t, e, "xy")
}

func TestTelaRedimensionaMantendoOCursor(t *testing.T) {
	e := desenhar(t, Tamanho{10, 4}, "a\r\nb\r\nc\r\nd")
	e.redimensionar(Tamanho{10, 2})
	conferir(t, e, "a", "b", "c", "d")
	if e.linha != 1 || e.linhas != 2 {
		t.Errorf("cursor na linha %d de %d", e.linha, e.linhas)
	}
	e.alimentar([]byte("\r\ne"))
	conferir(t, e, "a", "b", "c", "d", "e")
}

func TestSessaoLinhasRecentesVemDaTela(t *testing.T) {
	s := NovaSessao(1, nil, nil)
	s.DefinirTamanho(Tamanho{30, 4})
	s.guardar([]byte("\x1b[H● Oi\x1b[K\r\x1b[1B\x1b[2C⎿\x1b[5Gtudo bem\x1b[K"))
	if got := strings.Join(s.LinhasRecentes(10), "|"); got != "● Oi|  ⎿ tudo bem" {
		t.Errorf("linhas: %q", got)
	}
}
