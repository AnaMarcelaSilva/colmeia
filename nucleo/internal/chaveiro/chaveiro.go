// Package chaveiro guarda as senhas das conexões de banco fora do SQLite da
// Colmeia: no chaveiro do sistema (Secret Service pelo D-Bus no Linux) ou, sem
// ele, só na memória do núcleo até ele encerrar. A senha nunca vai para o
// banco, para log, evento ou resposta da API.
package chaveiro

import (
	"errors"
	"os"
	"sync"
	"time"

	"github.com/zalando/go-keyring"
)

var (
	// ErrNaoEncontrada: não há senha guardada para a conta.
	ErrNaoEncontrada = errors.New("senha não encontrada no chaveiro")
	// ErrIndisponivel: não há chaveiro do sistema (ou ele não respondeu a tempo).
	ErrIndisponivel = errors.New("chaveiro do sistema indisponível")
	// ErrBloqueado: o chaveiro não respondeu no prazo (bloqueado, esperando a senha de desbloqueio).
	ErrBloqueado = errors.New("o chaveiro não respondeu: pode estar bloqueado")
)

// Chaveiro é onde as senhas ficam. A conta é "conexao:<uuid>".
type Chaveiro interface {
	Disponivel() bool
	Guardar(conta, senha string) error
	Ler(conta string) (string, error)
	Apagar(conta string) error
}

// Servico é o nome com que as senhas aparecem no chaveiro.
func Servico() string {
	if s := os.Getenv("COLMEIA_CHAVEIRO_SERVICO"); s != "" {
		return s
	}
	return "Colmeia"
}

// PrazoPadrao para cada chamada ao chaveiro do sistema: o desbloqueio do
// gnome-keyring pode abrir uma janela e esperar a pessoa.
const PrazoPadrao = 30 * time.Second

// Operacoes do chaveiro do sistema (os testes trocam por um falso).
type Operacoes interface {
	Set(servico, conta, senha string) error
	Get(servico, conta string) (string, error)
	Delete(servico, conta string) error
}

type sistemaReal struct{}

func (sistemaReal) Set(s, c, p string) error        { return keyring.Set(s, c, p) }
func (sistemaReal) Get(s, c string) (string, error) { return keyring.Get(s, c) }
func (sistemaReal) Delete(s, c string) error        { return keyring.Delete(s, c) }
func naoEncontrada(err error) bool                  { return errors.Is(err, keyring.ErrNotFound) }
func traduzir(err error) error {
	if err == nil {
		return nil
	}
	if naoEncontrada(err) {
		return ErrNaoEncontrada
	}
	return err
}

// Sistema usa o chaveiro do sistema. Cada chamada tem prazo; a primeira
// necessidade faz uma sondagem e guarda se ele existe.
type Sistema struct {
	Servico string
	Prazo   time.Duration
	Ops     Operacoes

	once       sync.Once
	disponivel bool
}

// NovoSistema prepara o chaveiro do sistema com o serviço de COLMEIA_CHAVEIRO_SERVICO.
func NovoSistema() *Sistema {
	return &Sistema{Servico: Servico(), Prazo: PrazoPadrao, Ops: sistemaReal{}}
}

// comPrazo roda a chamada numa goroutine e desiste depois do prazo (a
// goroutine fica esperando o D-Bus, mas o núcleo segue).
func (s *Sistema) comPrazo(f func() (string, error)) (string, error) {
	type resposta struct {
		valor string
		err   error
	}
	canal := make(chan resposta, 1)
	go func() {
		v, err := f()
		canal <- resposta{v, err}
	}()
	prazo := s.Prazo
	if prazo <= 0 {
		prazo = PrazoPadrao
	}
	t := time.NewTimer(prazo)
	defer t.Stop()
	select {
	case r := <-canal:
		return r.valor, r.err
	case <-t.C:
		return "", ErrBloqueado
	}
}

// Disponivel faz a sondagem uma vez: ler uma conta que não existe. "Não
// encontrada" quer dizer que o chaveiro respondeu.
func (s *Sistema) Disponivel() bool {
	s.once.Do(func() {
		_, err := s.comPrazo(func() (string, error) { return s.Ops.Get(s.Servico, "sonda:inexistente") })
		s.disponivel = err == nil || naoEncontrada(err)
	})
	return s.disponivel
}

func (s *Sistema) Guardar(conta, senha string) error {
	if !s.Disponivel() {
		return ErrIndisponivel
	}
	_, err := s.comPrazo(func() (string, error) { return "", s.Ops.Set(s.Servico, conta, senha) })
	return traduzir(err)
}

func (s *Sistema) Ler(conta string) (string, error) {
	if !s.Disponivel() {
		return "", ErrIndisponivel
	}
	v, err := s.comPrazo(func() (string, error) { return s.Ops.Get(s.Servico, conta) })
	return v, traduzir(err)
}

func (s *Sistema) Apagar(conta string) error {
	if !s.Disponivel() {
		return nil
	}
	_, err := s.comPrazo(func() (string, error) { return "", s.Ops.Delete(s.Servico, conta) })
	if err = traduzir(err); errors.Is(err, ErrNaoEncontrada) {
		return nil
	}
	return err
}

// Memoria guarda as senhas num mapa, só enquanto o núcleo roda. É o caminho
// sem chaveiro e o dos testes. Com Ausente, faz o papel de "não há chaveiro".
type Memoria struct {
	Ausente bool

	mu     sync.Mutex
	senhas map[string]string
}

func NovaMemoria() *Memoria { return &Memoria{senhas: map[string]string{}} }

func (m *Memoria) Disponivel() bool { return !m.Ausente }

func (m *Memoria) Guardar(conta, senha string) error {
	if m.Ausente {
		return ErrIndisponivel
	}
	m.mu.Lock()
	defer m.mu.Unlock()
	if m.senhas == nil {
		m.senhas = map[string]string{}
	}
	m.senhas[conta] = senha
	return nil
}

func (m *Memoria) Ler(conta string) (string, error) {
	if m.Ausente {
		return "", ErrIndisponivel
	}
	m.mu.Lock()
	defer m.mu.Unlock()
	v, ok := m.senhas[conta]
	if !ok {
		return "", ErrNaoEncontrada
	}
	return v, nil
}

func (m *Memoria) Apagar(conta string) error {
	m.mu.Lock()
	defer m.mu.Unlock()
	delete(m.senhas, conta)
	return nil
}

// Contas guardadas (para os testes conferirem que algo foi apagado).
func (m *Memoria) Contas() []string {
	m.mu.Lock()
	defer m.mu.Unlock()
	lista := make([]string, 0, len(m.senhas))
	for c := range m.senhas {
		lista = append(lista, c)
	}
	return lista
}

// DoAmbiente escolhe o chaveiro: COLMEIA_CHAVEIRO=memoria faz o núcleo agir
// como se não houvesse chaveiro (as senhas ficam só na memória); senão, o do
// sistema.
func DoAmbiente() Chaveiro {
	if os.Getenv("COLMEIA_CHAVEIRO") == "memoria" {
		return &Memoria{Ausente: true}
	}
	return NovoSistema()
}
