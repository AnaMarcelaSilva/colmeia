package terminal

import (
	"bytes"
	"io"
	"strings"
	"sync/atomic"
	"testing"
	"time"
)

// ptyFalso guarda o último tamanho pedido e devolve fim de arquivo na leitura.
type ptyFalso struct {
	colunas, linhas uint16
	escrito         bytes.Buffer
}

func (p *ptyFalso) Read([]byte) (int, error)        { return 0, io.EOF }
func (p *ptyFalso) Write(b []byte) (int, error)     { return p.escrito.Write(b) }
func (p *ptyFalso) Close() error                    { return nil }
func (p *ptyFalso) Redimensionar(c, l uint16) error { p.colunas, p.linhas = c, l; return nil }
func (p *ptyFalso) Esperar() Saida                  { return Saida{} }

func TestRedimensionarIgnoraTamanhosAbsurdos(t *testing.T) {
	falso := &ptyFalso{}
	s := NovaSessao(0, falso, new(atomic.Int64))
	s.Redimensionar(120, 40)
	s.Redimensionar(0, 40)
	s.Redimensionar(MaxColunas+1, 40)
	s.Redimensionar(120, MaxLinhas+1)
	if falso.colunas != 120 || falso.linhas != 40 {
		t.Errorf("tamanho ficou %dx%d, esperado 120x40", falso.colunas, falso.linhas)
	}
}

func TestIntervaloFicaDentroDosLimites(t *testing.T) {
	s := NovaSessao(0, &ptyFalso{}, new(atomic.Int64))
	c, _ := s.Conectar(time.Nanosecond)
	if c.Intervalo() != intervaloMinimo {
		t.Errorf("intervalo %v, esperado o mínimo %v", c.Intervalo(), intervaloMinimo)
	}
	c.DefinirIntervalo(time.Hour)
	if c.Intervalo() != intervaloMaximo {
		t.Errorf("intervalo %v, esperado o máximo %v", c.Intervalo(), intervaloMaximo)
	}
}

func TestConfirmacaoNegativaNaoLiberaLeitura(t *testing.T) {
	s := NovaSessao(0, &ptyFalso{}, new(atomic.Int64))
	c, _ := s.Conectar(IntervaloPadrao)
	c.semConfirmacao.Store(LimiteSemConfirmacao + 1)
	s.Confirmar(c, -LimiteSemConfirmacao) // uma tela não pode "desconfirmar" para burlar o limite
	s.mu.Lock()
	congestionado := s.congestionado()
	s.mu.Unlock()
	if !congestionado {
		t.Error("confirmação negativa mudou a contagem")
	}
}

func TestDetectorSoVeOQueVeioDepoisDaEscrita(t *testing.T) {
	tempos := Tempos{Silencio: time.Hour, Ocioso: time.Hour, Eco: 30 * time.Millisecond}
	s := NovaSessao(0, &ptyFalso{}, new(atomic.Int64))
	s.Acompanhar("claude", tempos)
	defer s.atividade.parar()
	s.guardar([]byte("Do you want to proceed?\r\n"))
	if detectar("claude", s.ultimos(4096)) != PedeAprovacao {
		t.Fatal("o pedido de aprovação não foi visto")
	}
	// Você responde: o pedido fica para trás, e o eco (mesmo que repita o
	// texto, como a caixa de mensagem das ferramentas) também.
	s.Escrever([]byte("1"))
	s.guardar([]byte("1 Do you want to (y/n)"))
	if m := detectar("claude", s.ultimos(4096)); m != "" {
		t.Errorf("pedido respondido ou eco ainda contam: %q", m)
	}
	time.Sleep(2 * tempos.Eco)
	s.guardar([]byte("1\r\n2\r\n3\r\n$ "))
	if got := string(s.ultimos(4096)); got != "1\r\n2\r\n3\r\n$ " {
		t.Errorf("depois do corte: %q", got)
	}
	// Um pedido novo, depois do eco, volta a contar.
	s.guardar([]byte("Do you want to make this edit?"))
	if detectar("claude", s.ultimos(4096)) != PedeAprovacao {
		t.Error("um pedido novo não foi visto")
	}
}

func TestFecharPelaColmeiaCongelaOEstado(t *testing.T) {
	a := novaAtividade("claude", Tempos{Silencio: time.Hour, Ocioso: time.Hour, Eco: time.Millisecond}, func(int) []byte { return nil }, func(Mudanca) {})
	defer a.parar()
	a.mu.Lock()
	a.mudar(codAguardando, EsperandoResposta)
	a.mu.Unlock()
	a.congelar()
	// O programa redesenha ao receber o SIGHUP: não é trabalho.
	a.saida([]byte("tchau"))
	if estado, motivo, _ := a.atual(); estado != Aguardando || motivo != EsperandoResposta {
		t.Errorf("estado depois de congelar: %s (%s)", estado, motivo)
	}
}

func TestAmbienteLimpoTiraSoAsVariaveisDeSessao(t *testing.T) {
	limpo := AmbienteLimpo([]string{"HOME=/casa", "CLAUDECODE=1", "CLAUDE_CODE_CHILD_SESSION=1", "CLAUDE_CODE_MESSAGING_TOKEN=x", "CLAUDE_CONFIG_DIR=/conta", "CLAUDE_CODE_USE_VERTEX=1"})
	esperado := []string{"HOME=/casa", "CLAUDE_CONFIG_DIR=/conta", "CLAUDE_CODE_USE_VERTEX=1"}
	if strings.Join(limpo, " ") != strings.Join(esperado, " ") {
		t.Errorf("ambiente limpo: %v", limpo)
	}
}

func TestColagem(t *testing.T) {
	if string(Colagem("git status", true)) != "git status" {
		t.Error("uma linha não leva marca de colagem")
	}
	if string(Colagem("um\ndois", true)) != "\x1b[200~um\ndois\x1b[201~" {
		t.Error("várias linhas vão entre as marcas de colagem")
	}
	// Sem o modo de colagem, as marcas apareceriam cruas: as linhas vão juntas.
	if got := string(Colagem("[Pedido da daily]\r\nTraga o total\n\nQuando terminar, responda.", false)); got != "[Pedido da daily] Traga o total Quando terminar, responda." {
		t.Errorf("sem modo de colagem: %q", got)
	}
}

func TestModoDeColagemPelaSaida(t *testing.T) {
	var n atomic.Int64
	s := NovaSessao(1, nil, &n)
	if s.ColagemLigada() {
		t.Fatal("começa desligado")
	}
	s.guardar([]byte("ola\x1b[?20"))
	s.guardar([]byte("04hmais"))
	if !s.ColagemLigada() {
		t.Error("a sequência partida entre duas leituras não ligou")
	}
	s.guardar([]byte("x\x1b[?2004ly\x1b[?2004h"))
	if !s.ColagemLigada() {
		t.Error("vale a última sequência")
	}
	s.guardar([]byte("\x1b[?2004l"))
	if s.ColagemLigada() {
		t.Error("não desligou")
	}
}
