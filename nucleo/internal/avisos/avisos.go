// Package avisos é o barramento em memória que leva as mudanças às telas
// conectadas, na hora. Quem publica nunca espera: uma tela lenta perde as
// mensagens que não couberam na fila e recebe um único "recarregar", que a
// faz pedir o retrato do quadro de novo.
package avisos

import (
	"context"
	"encoding/json"
	"errors"
	"sync"
	"sync/atomic"
)

const (
	// Mensagens guardadas por tela antes de ela ser considerada atrasada.
	TamanhoFila = 256
	// Telas conectadas ao mesmo tempo (a desktop, o celular no futuro, testes).
	MaxAssinaturas = 16
)

var ErrMuitasAssinaturas = errors.New("telas demais conectadas aos eventos")

// Todos os perfis: um aviso do núcleo que vale para qualquer tela.
const Todos = 0

type Barramento struct {
	mu          sync.Mutex
	seq         atomic.Uint64
	assinaturas map[*Assinatura]struct{}
}

func Novo() *Barramento { return &Barramento{assinaturas: map[*Assinatura]struct{}{}} }

// Seq é o número da última mensagem publicada. O retrato do quadro leva esse
// número: a tela aplica por cima dele só o que veio depois.
func (b *Barramento) Seq() uint64 { return b.seq.Load() }

// Publicar numera a mensagem, acrescenta "seq" e entrega às telas do perfil.
// Nunca bloqueia: é chamado de dentro dos pedidos e da leitura dos terminais.
func (b *Barramento) Publicar(perfil int64, mensagem map[string]any) {
	b.mu.Lock()
	defer b.mu.Unlock()
	seq := b.seq.Add(1)
	mensagem["seq"] = seq
	bruto, err := json.Marshal(mensagem)
	if err != nil {
		return
	}
	for a := range b.assinaturas {
		if perfil != Todos && a.perfil != perfil {
			continue
		}
		select {
		case a.fila <- bruto:
		default:
			a.atrasada.Store(true)
		}
	}
}

// Assinatura é uma tela recebendo os avisos de um perfil. A abelha fixa
// recebe os mesmos avisos, mas não conta como janela aberta (Janelas).
type Assinatura struct {
	perfil   int64
	abelha   bool
	fila     chan []byte
	atrasada atomic.Bool
	b        *Barramento
}

func (b *Barramento) Assinar(perfil int64) (*Assinatura, error) { return b.assinar(perfil, false) }

// AssinarAbelha é a assinatura do programa da abelha fixa.
func (b *Barramento) AssinarAbelha(perfil int64) (*Assinatura, error) { return b.assinar(perfil, true) }

func (b *Barramento) assinar(perfil int64, abelha bool) (*Assinatura, error) {
	b.mu.Lock()
	defer b.mu.Unlock()
	if len(b.assinaturas) >= MaxAssinaturas {
		return nil, ErrMuitasAssinaturas
	}
	a := &Assinatura{perfil: perfil, abelha: abelha, fila: make(chan []byte, TamanhoFila), b: b}
	b.assinaturas[a] = struct{}{}
	return a, nil
}

// Janelas conta as janelas da Colmeia ligadas aos avisos do perfil, sem a
// abelha fixa: diz se um pedido de abrir uma tarefa tem quem atenda.
func (b *Barramento) Janelas(perfil int64) int {
	b.mu.Lock()
	defer b.mu.Unlock()
	n := 0
	for a := range b.assinaturas {
		if a.perfil == perfil && !a.abelha {
			n++
		}
	}
	return n
}

func (a *Assinatura) Cancelar() {
	a.b.mu.Lock()
	defer a.b.mu.Unlock()
	delete(a.b.assinaturas, a)
}

// Proxima espera a próxima mensagem. Se a tela ficou para trás, descarta o
// que sobrou na fila e devolve {"tipo":"recarregar"}.
func (a *Assinatura) Proxima(ctx context.Context) ([]byte, error) {
	if a.atrasada.Swap(false) {
		for {
			select {
			case <-a.fila:
				continue
			default:
			}
			break
		}
		return []byte(`{"tipo":"recarregar"}`), nil
	}
	select {
	case m := <-a.fila:
		return m, nil
	case <-ctx.Done():
		return nil, ctx.Err()
	}
}
