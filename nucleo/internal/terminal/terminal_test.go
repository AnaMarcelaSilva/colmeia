package terminal

import (
	"bytes"
	"io"
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
