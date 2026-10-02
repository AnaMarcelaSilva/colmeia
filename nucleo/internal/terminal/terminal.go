// Package terminal mantém os terminais dos agentes. O núcleo é dono deles:
// fechar a tela não encerra o terminal, e quem se conecta recebe o histórico.
package terminal

import (
	"io"
	"log"
	"sync"
	"sync/atomic"
	"time"
)

const (
	tamanhoHistorico = 256 * 1024 // buffer fixo por terminal
	tamanhoReenvio   = 128 * 1024 // ao conectar, só o que cabe na rolagem da tela
	// Acima disso sem confirmação da tela, o núcleo para de ler o terminal:
	// o programa que escreve fica esperando e o processador descansa.
	LimiteSemConfirmacao = 1024 * 1024

	IntervaloPadrao = 16 * time.Millisecond
	intervaloMinimo = 8 * time.Millisecond
	intervaloMaximo = 5 * time.Second

	MaxColunas = 500
	MaxLinhas  = 300
)

// Pty é o lado do núcleo de um pseudo-terminal.
type Pty interface {
	io.ReadWriteCloser
	Redimensionar(colunas, linhas uint16) error
}

// Cliente é uma tela conectada a um terminal.
type Cliente struct {
	pendente       []byte        // saída ainda não enviada; protegido por Sessao.mu
	Aviso          chan struct{} // acorda o envio quando chega saída nova
	intervalo      atomic.Int64  // tempo mínimo entre envios, em nanossegundos
	semConfirmacao atomic.Int64  // bytes enviados que a tela ainda não desenhou
}

func (c *Cliente) Intervalo() time.Duration { return time.Duration(c.intervalo.Load()) }

// DefinirIntervalo muda o ritmo de envio, sempre dentro dos limites.
func (c *Cliente) DefinirIntervalo(d time.Duration) {
	c.intervalo.Store(int64(min(max(d, intervaloMinimo), intervaloMaximo)))
	c.acordar()
}

func (c *Cliente) acordar() {
	select {
	case c.Aviso <- struct{}{}:
	default:
	}
}

type Sessao struct {
	ID        int64
	pty       Pty
	fim       chan struct{} // fechado quando o programa do terminal termina
	mu        sync.Mutex
	liberado  *sync.Cond
	historico []byte
	clientes  map[*Cliente]struct{}
	bytes     *atomic.Int64
}

func NovaSessao(id int64, pty Pty, bytes *atomic.Int64) *Sessao {
	s := &Sessao{ID: id, pty: pty, fim: make(chan struct{}), clientes: map[*Cliente]struct{}{}, bytes: bytes}
	s.liberado = sync.NewCond(&s.mu)
	return s
}

// congestionado diz se alguma tela está atrasada demais. Chamar com mu travado.
func (s *Sessao) congestionado() bool {
	for c := range s.clientes {
		if len(c.pendente) > LimiteSemConfirmacao || c.semConfirmacao.Load() > LimiteSemConfirmacao {
			return true
		}
	}
	return false
}

// Ler repassa a saída do terminal para as telas até o terminal fechar.
func (s *Sessao) Ler() {
	buf := make([]byte, 32*1024)
	for {
		s.mu.Lock()
		for s.congestionado() {
			s.liberado.Wait()
		}
		s.mu.Unlock()

		n, err := s.pty.Read(buf)
		if n > 0 {
			s.bytes.Add(int64(n))
			s.mu.Lock()
			for c := range s.clientes {
				c.pendente = append(c.pendente, buf[:n]...)
				c.acordar()
			}
			s.historico = append(s.historico, buf[:n]...)
			// Corta só ao passar do dobro, para não copiar o histórico a cada leitura.
			if len(s.historico) > 2*tamanhoHistorico {
				s.historico = append(make([]byte, 0, 2*tamanhoHistorico), s.historico[len(s.historico)-tamanhoHistorico:]...)
			}
			s.mu.Unlock()
		}
		if err != nil {
			// O conteúdo do terminal nunca vai para o log.
			log.Printf("terminal %d encerrado", s.ID)
			close(s.fim)
			return
		}
	}
}

// Conectar registra uma tela e devolve o fim do histórico para ela desenhar.
func (s *Sessao) Conectar(intervalo time.Duration) (*Cliente, []byte) {
	s.mu.Lock()
	defer s.mu.Unlock()
	c := &Cliente{Aviso: make(chan struct{}, 1)}
	c.intervalo.Store(int64(min(max(intervalo, intervaloMinimo), intervaloMaximo)))
	inicio := max(0, len(s.historico)-tamanhoReenvio)
	historico := append([]byte(nil), s.historico[inicio:]...)
	c.semConfirmacao.Store(int64(len(historico)))
	s.clientes[c] = struct{}{}
	return c, historico
}

// Retirar pega o que está pendente para a tela e libera a leitura se ela esperava.
func (s *Sessao) Retirar(c *Cliente) []byte {
	s.mu.Lock()
	defer s.mu.Unlock()
	bloco := c.pendente
	c.pendente = nil
	c.semConfirmacao.Add(int64(len(bloco)))
	s.liberado.Broadcast()
	return bloco
}

// Confirmar registra que a tela desenhou `n` bytes.
func (s *Sessao) Confirmar(c *Cliente, n int64) {
	if n <= 0 {
		return
	}
	s.mu.Lock()
	defer s.mu.Unlock()
	c.semConfirmacao.Add(-n)
	s.liberado.Broadcast()
}

func (s *Sessao) Desconectar(c *Cliente) {
	s.mu.Lock()
	defer s.mu.Unlock()
	delete(s.clientes, c)
	s.liberado.Broadcast()
}

func (s *Sessao) Escrever(dados []byte) (int, error) { return s.pty.Write(dados) }

// Redimensionar aceita só tamanhos plausíveis.
func (s *Sessao) Redimensionar(colunas, linhas uint16) {
	if colunas == 0 || linhas == 0 || colunas > MaxColunas || linhas > MaxLinhas {
		return
	}
	s.pty.Redimensionar(colunas, linhas)
}

func (s *Sessao) Fechar() error { return s.pty.Close() }

// Fim avisa quando o programa do terminal termina.
func (s *Sessao) Fim() <-chan struct{} { return s.fim }

// Encerrada diz se o programa do terminal já terminou.
func (s *Sessao) Encerrada() bool {
	select {
	case <-s.fim:
		return true
	default:
		return false
	}
}

// Gerente guarda os terminais dos agentes, que aparecem e somem enquanto o
// núcleo roda.
type Gerente struct {
	mu      sync.Mutex
	sessoes map[int64]*Sessao
}

func NovoGerente() *Gerente { return &Gerente{sessoes: map[int64]*Sessao{}} }

// Adicionar registra a sessão e começa a ler dela. Se já havia uma com o mesmo
// id, a antiga é fechada.
func (g *Gerente) Adicionar(s *Sessao) {
	g.mu.Lock()
	antiga := g.sessoes[s.ID]
	g.sessoes[s.ID] = s
	g.mu.Unlock()
	if antiga != nil {
		antiga.Fechar()
	}
	go s.Ler()
}

func (g *Gerente) Pegar(id int64) (*Sessao, bool) {
	g.mu.Lock()
	defer g.mu.Unlock()
	s, ok := g.sessoes[id]
	return s, ok
}

// Ativa diz se há um terminal rodando para o id.
func (g *Gerente) Ativa(id int64) bool {
	s, ok := g.Pegar(id)
	return ok && !s.Encerrada()
}

// Fechar encerra e esquece o terminal do id, se houver.
func (g *Gerente) Fechar(id int64) {
	g.mu.Lock()
	s := g.sessoes[id]
	delete(g.sessoes, id)
	g.mu.Unlock()
	if s != nil {
		s.Fechar()
	}
}

func (g *Gerente) Quantidade() int {
	g.mu.Lock()
	defer g.mu.Unlock()
	return len(g.sessoes)
}

// FecharTodos encerra todos os terminais, ao desligar o núcleo.
func (g *Gerente) FecharTodos() {
	g.mu.Lock()
	sessoes := g.sessoes
	g.sessoes = map[int64]*Sessao{}
	g.mu.Unlock()
	var espera sync.WaitGroup
	for _, s := range sessoes {
		espera.Go(func() { s.Fechar() })
	}
	espera.Wait()
}
