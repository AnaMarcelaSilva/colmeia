package navegador

import (
	"bufio"
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"sync"
)

// MaxMensagem do Chrome: uma captura vem em base64 numa mensagem só.
const MaxMensagem = 64 << 20

// conexao fala o protocolo de controle do Chrome (CDP) por um par de pipes:
// mensagens JSON separadas por um byte zero. Cada chamada tem um id e espera
// a resposta num canal próprio; os eventos vão para quem estiver esperando
// por eles. A leitura fica parada até chegar mensagem: nada de espera ativa.
type conexao struct {
	escrita io.Writer
	muEsc   sync.Mutex

	mu       sync.Mutex
	proximo  int64
	espera   map[int64]chan resposta
	ouvintes map[*ouvinte]struct{}
	fechada  bool
	fim      chan struct{}
	err      error

	// aoEvento recebe todos os eventos (sessao "" para os do navegador
	// inteiro). Roda na leitura: não pode chamar o navegador e esperar.
	aoEvento func(sessao, metodo string, params json.RawMessage)
}

type resposta struct {
	resultado json.RawMessage
	err       error
}

type mensagemCDP struct {
	ID        int64           `json:"id,omitempty"`
	Metodo    string          `json:"method,omitempty"`
	Params    json.RawMessage `json:"params,omitempty"`
	Resultado json.RawMessage `json:"result,omitempty"`
	Erro      *struct {
		Codigo   int    `json:"code"`
		Mensagem string `json:"message"`
	} `json:"error,omitempty"`
	Sessao string `json:"sessionId,omitempty"`
}

// ouvinte espera um evento de uma sessão (ou de qualquer uma, com sessão "").
type ouvinte struct {
	sessao, metodo string
	ch             chan json.RawMessage
}

func novaConexao(leitura io.Reader, escrita io.Writer) *conexao {
	c := &conexao{escrita: escrita, espera: map[int64]chan resposta{}, ouvintes: map[*ouvinte]struct{}{}, fim: make(chan struct{})}
	go c.ler(leitura)
	return c
}

// separarPorZero corta as mensagens no byte zero.
func separarPorZero(dados []byte, noFim bool) (int, []byte, error) {
	if i := bytes.IndexByte(dados, 0); i >= 0 {
		return i + 1, dados[:i], nil
	}
	if noFim && len(dados) > 0 {
		return len(dados), dados, nil
	}
	return 0, nil, nil
}

func (c *conexao) ler(leitura io.Reader) {
	leitor := bufio.NewScanner(leitura)
	leitor.Buffer(make([]byte, 0, 64<<10), MaxMensagem)
	leitor.Split(separarPorZero)
	for leitor.Scan() {
		var m mensagemCDP
		if json.Unmarshal(leitor.Bytes(), &m) != nil {
			continue
		}
		if m.ID != 0 {
			c.mu.Lock()
			ch := c.espera[m.ID]
			delete(c.espera, m.ID)
			c.mu.Unlock()
			if ch != nil {
				r := resposta{resultado: m.Resultado}
				if m.Erro != nil {
					r.err = fmt.Errorf("o navegador recusou: %s", m.Erro.Mensagem)
				}
				ch <- r
			}
			continue
		}
		if m.Metodo == "" {
			continue
		}
		c.mu.Lock()
		for o := range c.ouvintes {
			if o.metodo == m.Metodo && (o.sessao == "" || o.sessao == m.Sessao) {
				select {
				case o.ch <- m.Params:
				default:
				}
			}
		}
		ao := c.aoEvento
		c.mu.Unlock()
		if ao != nil {
			ao(m.Sessao, m.Metodo, m.Params)
		}
	}
	err := leitor.Err()
	if err == nil {
		err = io.EOF
	}
	c.fechar(err)
}

// fechar acorda todas as chamadas pendentes com o erro.
func (c *conexao) fechar(err error) {
	c.mu.Lock()
	defer c.mu.Unlock()
	if c.fechada {
		return
	}
	c.fechada, c.err = true, err
	for id, ch := range c.espera {
		ch <- resposta{err: errFechado}
		delete(c.espera, id)
	}
	close(c.fim)
}

var errFechado = errors.New("o navegador foi fechado")

// chamar envia um comando (na sessão, se houver) e espera a resposta.
func (c *conexao) chamar(ctx context.Context, sessao, metodo string, params any, resultado any) error {
	c.mu.Lock()
	if c.fechada {
		c.mu.Unlock()
		return errFechado
	}
	c.proximo++
	id := c.proximo
	ch := make(chan resposta, 1)
	c.espera[id] = ch
	c.mu.Unlock()

	m := map[string]any{"id": id, "method": metodo}
	if params != nil {
		m["params"] = params
	}
	if sessao != "" {
		m["sessionId"] = sessao
	}
	bruto, err := json.Marshal(m)
	if err != nil {
		return err
	}
	c.muEsc.Lock()
	_, err = c.escrita.Write(append(bruto, 0))
	c.muEsc.Unlock()
	if err != nil {
		c.mu.Lock()
		delete(c.espera, id)
		c.mu.Unlock()
		return errFechado
	}
	select {
	case r := <-ch:
		if r.err != nil {
			return r.err
		}
		if resultado != nil && len(r.resultado) > 0 {
			return json.Unmarshal(r.resultado, resultado)
		}
		return nil
	case <-ctx.Done():
		c.mu.Lock()
		delete(c.espera, id)
		c.mu.Unlock()
		return ctx.Err()
	}
}

// ouvir registra a espera por um evento; cancelar tira o registro.
func (c *conexao) ouvir(sessao, metodo string) (*ouvinte, func()) {
	o := &ouvinte{sessao: sessao, metodo: metodo, ch: make(chan json.RawMessage, 4)}
	c.mu.Lock()
	c.ouvintes[o] = struct{}{}
	c.mu.Unlock()
	return o, func() {
		c.mu.Lock()
		delete(c.ouvintes, o)
		c.mu.Unlock()
	}
}
