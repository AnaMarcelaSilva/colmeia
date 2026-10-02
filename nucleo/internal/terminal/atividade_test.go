//go:build unix

package terminal

import (
	"sync"
	"sync/atomic"
	"testing"
	"time"
)

var temposCurtos = Tempos{Silencio: 40 * time.Millisecond, Ocioso: 150 * time.Millisecond, Eco: 30 * time.Millisecond}

// acompanhado monta uma atividade com uma "tela" fabricada e guarda os avisos.
type acompanhado struct {
	mu     sync.Mutex
	tela   []byte
	avisos []Mudanca
	mudou  chan struct{}
	a      *atividade
}

func acompanhar(t *testing.T, ferramenta string) *acompanhado {
	t.Helper()
	c := &acompanhado{mudou: make(chan struct{}, 16)}
	c.a = novaAtividade(ferramenta, temposCurtos, func(int) []byte {
		c.mu.Lock()
		defer c.mu.Unlock()
		return append([]byte(nil), c.tela...)
	}, func(m Mudanca) {
		c.mu.Lock()
		c.avisos = append(c.avisos, m)
		c.mu.Unlock()
		c.mudou <- struct{}{}
	})
	t.Cleanup(func() { c.a.parar() })
	return c
}

func (c *acompanhado) escrever(b string) {
	c.mu.Lock()
	c.tela = append(c.tela, b...)
	c.mu.Unlock()
	c.a.saida([]byte(b))
}

func (c *acompanhado) esperar(t *testing.T, estado, motivo string) {
	t.Helper()
	limite := time.After(2 * time.Second)
	for {
		c.mu.Lock()
		var ultima Mudanca
		if len(c.avisos) > 0 {
			ultima = c.avisos[len(c.avisos)-1]
		}
		c.mu.Unlock()
		if ultima.Estado == estado && ultima.Motivo == motivo {
			return
		}
		select {
		case <-c.mudou:
		case <-limite:
			t.Fatalf("esperava %s (%s), último aviso: %+v", estado, motivo, ultima)
		}
	}
}

func TestClaudeEmSilencioEhSuaVez(t *testing.T) {
	c := acompanhar(t, "claude")
	c.escrever("\x1b[32m✻ Pensando…\x1b[0m")
	c.esperar(t, Aguardando, EsperandoResposta)
	// Voltou a escrever: trabalhando de novo.
	c.escrever("✻ Editando arquivos…")
	c.esperar(t, Trabalhando, "")
}

func TestPedidoDeAprovacaoTemPrioridade(t *testing.T) {
	c := acompanhar(t, "claude")
	c.escrever("\x1b[1mDo you want to make this edit?\x1b[0m\r\n\x1b[36m❯ 1. Yes\x1b[0m\r\n  2. No\r\n")
	c.esperar(t, Aguardando, PedeAprovacao)
}

func TestTerminalComumFicaOcioso(t *testing.T) {
	c := acompanhar(t, "shell")
	c.escrever("$ make\r\nok\r\n$ ")
	c.esperar(t, Ocioso, "")
	c.mu.Lock()
	for _, m := range c.avisos {
		if m.Estado == Aguardando {
			t.Errorf("um terminal comum em silêncio não espera você: %+v", c.avisos)
		}
	}
	c.mu.Unlock()
}

func TestBelForaDoTituloPedeAtencao(t *testing.T) {
	c := acompanhar(t, "shell")
	// O BEL que termina um título (OSC) não conta...
	c.escrever("\x1b]0;meu título\x07$ ")
	c.esperar(t, Ocioso, "")
	// ...o BEL solto, sim.
	c.escrever("pronto\x07")
	c.esperar(t, Aguardando, EsperandoResposta)
}

func TestEcoDaDigitacaoNaoEhTrabalho(t *testing.T) {
	c := acompanhar(t, "claude")
	c.escrever("pergunta?")
	c.esperar(t, Aguardando, EsperandoResposta)
	c.mu.Lock()
	antes := len(c.avisos)
	c.mu.Unlock()
	// Você digita a resposta: o eco volta logo depois e não muda o estado.
	c.a.entrada()
	c.escrever("sim, pode")
	time.Sleep(3 * temposCurtos.Silencio)
	c.mu.Lock()
	depois := len(c.avisos)
	c.mu.Unlock()
	if depois != antes {
		t.Errorf("o eco da digitação mudou o estado")
	}
}

func TestTemposDaSessao(t *testing.T) {
	c := acompanhar(t, "claude")
	c.escrever("trabalhando")
	c.esperar(t, Aguardando, EsperandoResposta)
	time.Sleep(20 * time.Millisecond)
	m := c.a.parar()
	if m.Trabalhando < temposCurtos.Silencio || m.Aguardando <= 0 || m.Duracao < m.Trabalhando+m.Aguardando {
		t.Errorf("tempos da sessão: %+v", m)
	}
}

func TestDetectorNaoOlhaOPassado(t *testing.T) {
	antigo := []byte("Do you want to proceed?\r\n")
	for range 200 {
		antigo = append(antigo, "\x1b[32mlinha de trabalho depois da aprovação\x1b[0m\r\n"...)
	}
	if m := detectar("claude", antigo); m != "" {
		t.Errorf("um pedido já respondido voltou: %q", m)
	}
	if m := detectar("shell", []byte("Continue? (y/n)")); m != "" {
		t.Errorf("o terminal comum não tem padrões: %q", m)
	}
	if m := detectar("codex", []byte("\x1b[1mAllow command?\x1b[0m [y/N]")); m != PedeAprovacao {
		t.Errorf("pedido do codex: %q", m)
	}
}

func TestLeituraNaoAlocaNoCasoComum(t *testing.T) {
	a := novaAtividade("claude", TemposPadrao, func(int) []byte { return nil }, func(Mudanca) {})
	defer a.parar()
	bloco := []byte("\x1b]0;título\x07saída comum do agente\r\n")
	a.saida(bloco)
	if n := testing.AllocsPerRun(1000, func() { a.saida(bloco) }); n != 0 {
		t.Errorf("a leitura alocou %.0f vezes por bloco", n)
	}
}

func BenchmarkSaida(b *testing.B) {
	a := novaAtividade("claude", TemposPadrao, func(int) []byte { return nil }, func(Mudanca) {})
	defer a.parar()
	bloco := make([]byte, 32*1024)
	for i := range bloco {
		bloco[i] = byte('a' + i%26)
	}
	b.SetBytes(int64(len(bloco)))
	b.ReportAllocs()
	for b.Loop() {
		a.saida(bloco)
	}
}

func TestCodigoDeSaidaEQuemFechou(t *testing.T) {
	g := NovoGerente()
	var mu sync.Mutex
	fins := map[int64]Mudanca{}
	g.AoMudar = func(id int64, m Mudanca) {
		if m.Tipo == "terminou" {
			mu.Lock()
			fins[id] = m
			mu.Unlock()
		}
	}
	var bytes atomic.Int64
	abrir := func(id int64, comando ...string) *Sessao {
		pty, err := Iniciar(comando, nil, t.TempDir())
		if err != nil {
			t.Fatal(err)
		}
		s := NovaSessao(id, pty, &bytes)
		s.Acompanhar("shell", temposCurtos)
		g.Adicionar(s)
		return s
	}
	falhou := abrir(1, "sh", "-c", "exit 3")
	<-falhou.lido
	abrir(2, "sleep", "30")
	g.Fechar(2)

	mu.Lock()
	defer mu.Unlock()
	if m := fins[1]; m.Saida != (Saida{Codigo: 3}) || m.PelaColmeia != "" {
		t.Errorf("sh -c 'exit 3': %+v", m)
	}
	if m := fins[2]; m.PelaColmeia != PelaRemocao || !m.Saida.PorSinal {
		t.Errorf("fechado pela Colmeia: %+v", m)
	}
}
