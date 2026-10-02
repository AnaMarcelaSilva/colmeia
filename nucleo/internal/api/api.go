// Package api é o protocolo entre o núcleo e as telas, versionado desde o
// primeiro dia (/v1). O desktop fala com o núcleo exatamente como um cliente
// futuro (o celular) falaria: nenhuma regra de negócio fica na tela.
package api

import (
	"context"
	"encoding/json"
	"net/http"
	"strconv"
	"sync"
	"sync/atomic"
	"time"

	"github.com/coder/websocket"

	"github.com/AnaMarcelaSilva/colmeia/nucleo/internal/avisos"
	"github.com/AnaMarcelaSilva/colmeia/nucleo/internal/dados"
	"github.com/AnaMarcelaSilva/colmeia/nucleo/internal/demo"
	"github.com/AnaMarcelaSilva/colmeia/nucleo/internal/terminal"
)

// VersaoProtocolo muda só quando uma mudança quebra clientes antigos.
const VersaoProtocolo = 1

// Maior mensagem aceita de uma tela (digitação, tamanho, confirmação).
const limiteMensagem = 1 << 20

type Servidor struct {
	// Sessoes são os terminais de teste do modo demonstração; Agentes, os
	// terminais dos agentes de verdade, que aparecem e somem.
	Sessoes []*terminal.Sessao
	Agentes *terminal.Gerente
	Bytes   *atomic.Int64
	Versao  string
	// Demo liga as cargas de teste, que escrevem comandos nos terminais.
	Demo bool
	// Banco guarda perfis, projetos e tarefas; DirDados é onde ele e as contas
	// separadas das ferramentas ficam.
	Banco    *dados.Banco
	DirDados string
	// Avisos leva as mudanças às telas conectadas, na hora.
	Avisos *avisos.Barramento
	// Tempos do acompanhamento dos agentes (os testes encurtam).
	Tempos terminal.Tempos
	// AvisoHistorico explica à tela que o histórico foi alterado por fora.
	AvisoHistorico string
	// AoEncerrar desliga o núcleo (POST /v1/encerrar).
	AoEncerrar func()

	mu sync.Mutex // um agente abre por vez

	// Trabalhador das mudanças dos agentes (veja eventos.go).
	mudancas    chan mudancaAgente
	parado      chan struct{}
	trabalhou   chan struct{}
	muContextos sync.Mutex
	contextos   map[int64]dados.ContextoAgente
}

func (s *Servidor) Rotas() http.Handler {
	mux := http.NewServeMux()
	mux.HandleFunc("GET /v1/versao", s.versao)
	mux.HandleFunc("GET /v1/estatisticas", s.estatisticas)
	mux.HandleFunc("GET /v1/terminais/{id}", s.terminal)
	if s.Agentes == nil {
		s.Agentes = terminal.NovoGerente()
	}
	if s.Avisos == nil {
		s.Avisos = avisos.Novo()
	}
	if s.Tempos == (terminal.Tempos{}) {
		s.Tempos = terminal.TemposPadrao
	}
	mux.HandleFunc("POST /v1/encerrar", s.encerrar)
	if s.Banco != nil {
		s.Banco.AoGravar(s.publicarEventos)
		s.iniciarTrabalhador()
		s.rotasDados(mux)
	}
	if s.Demo {
		mux.HandleFunc("POST /v1/demo/carga", s.carga)
	}
	return mux
}

func responderJSON(w http.ResponseWriter, v any) {
	w.Header().Set("Content-Type", "application/json")
	json.NewEncoder(w).Encode(v)
}

func (s *Servidor) versao(w http.ResponseWriter, _ *http.Request) {
	responderJSON(w, map[string]any{"protocolo": VersaoProtocolo, "nucleo": s.Versao, "demo": s.Demo, "terminais": len(s.Sessoes)})
}

func (s *Servidor) estatisticas(w http.ResponseWriter, _ *http.Request) {
	responderJSON(w, map[string]any{"terminais": len(s.Sessoes), "agentes": s.Agentes.Quantidade(), "bytes": s.Bytes.Load()})
}

// encerrar desliga o núcleo pelo canal, sem precisar de kill: responde antes
// e desliga em seguida, encerrando os agentes como num SIGTERM.
func (s *Servidor) encerrar(w http.ResponseWriter, _ *http.Request) {
	if s.AoEncerrar == nil {
		http.Error(w, "este núcleo não pode ser encerrado pela API", http.StatusNotImplemented)
		return
	}
	w.Header().Set("Content-Type", "application/json")
	w.WriteHeader(http.StatusAccepted)
	json.NewEncoder(w).Encode(map[string]any{"ok": true})
	go s.AoEncerrar()
}

func (s *Servidor) carga(w http.ResponseWriter, r *http.Request) {
	if err := demo.Aplicar(s.Sessoes, r.URL.Query().Get("modo")); err != nil {
		http.Error(w, err.Error(), http.StatusBadRequest)
		return
	}
	responderJSON(w, map[string]any{"ok": true})
}

// Mensagem de texto da tela para o núcleo: tamanho, confirmação ou novo ritmo.
type mensagem struct {
	Cols, Rows uint16
	Ack        int64
	Intervalo  int64 // milissegundos
}

func (s *Servidor) terminal(w http.ResponseWriter, r *http.Request) {
	id, err := strconv.Atoi(r.PathValue("id"))
	if err != nil || id < 0 || id >= len(s.Sessoes) {
		http.Error(w, "terminal inexistente", http.StatusNotFound)
		return
	}
	s.transmitir(w, r, s.Sessoes[id])
}

// transmitir liga a tela a um terminal pelo WebSocket. Quando o programa do
// terminal termina, a tela recebe {"fim":true} e a conexão continua aberta
// para ela ver o que ficou escrito.
func (s *Servidor) transmitir(w http.ResponseWriter, r *http.Request, sessao *terminal.Sessao) {
	intervalo := terminal.IntervaloPadrao
	if ms, err := strconv.ParseInt(r.URL.Query().Get("intervalo"), 10, 64); err == nil {
		intervalo = time.Duration(ms) * time.Millisecond
	}
	// Sem InsecureSkipVerify: uma conexão com Origin de outro site é recusada.
	conn, err := websocket.Accept(w, r, nil)
	if err != nil {
		return
	}
	defer conn.CloseNow()
	conn.SetReadLimit(limiteMensagem)

	// A tela informa o tamanho já na conexão, para o programa redesenhar
	// nele antes de o histórico ser enviado.
	colunas, _ := strconv.ParseUint(r.URL.Query().Get("cols"), 10, 16)
	linhas, _ := strconv.ParseUint(r.URL.Query().Get("rows"), 10, 16)
	sessao.Redimensionar(uint16(colunas), uint16(linhas))
	cliente, historico := sessao.Conectar(intervalo)
	defer sessao.Desconectar(cliente)

	ctx, cancelar := context.WithCancel(r.Context())
	defer cancelar()
	go s.receber(ctx, cancelar, conn, sessao, cliente)

	if len(historico) > 0 && conn.Write(ctx, websocket.MessageBinary, historico) != nil {
		return
	}
	// Parado, não acorda: espera chegar saída, aguarda o intervalo do cliente
	// para juntar mais e envia tudo de uma vez.
	espera := time.NewTimer(0)
	<-espera.C
	fim := sessao.Fim()
	for {
		select {
		case <-ctx.Done():
			return
		case <-cliente.Aviso:
		case <-fim:
			fim = nil
			if bloco := sessao.Retirar(cliente); len(bloco) > 0 && conn.Write(ctx, websocket.MessageBinary, bloco) != nil {
				return
			}
			if conn.Write(ctx, websocket.MessageText, []byte(`{"fim":true}`)) != nil {
				return
			}
			continue
		}
		espera.Reset(cliente.Intervalo())
		select {
		case <-ctx.Done():
			return
		case <-espera.C:
		}
		if bloco := sessao.Retirar(cliente); len(bloco) > 0 {
			if conn.Write(ctx, websocket.MessageBinary, bloco) != nil {
				return
			}
		}
	}
}

// receber trata o que vem da tela: binário é digitação; texto é controle.
func (s *Servidor) receber(ctx context.Context, cancelar func(), conn *websocket.Conn, sessao *terminal.Sessao, cliente *terminal.Cliente) {
	defer cancelar()
	for {
		tipo, dados, err := conn.Read(ctx)
		if err != nil {
			return
		}
		if tipo == websocket.MessageBinary {
			sessao.Escrever(dados)
			continue
		}
		var m mensagem
		if json.Unmarshal(dados, &m) != nil {
			continue
		}
		sessao.Confirmar(cliente, m.Ack)
		if m.Cols > 0 && m.Rows > 0 {
			sessao.Redimensionar(m.Cols, m.Rows)
		}
		if m.Intervalo > 0 {
			cliente.DefinirIntervalo(time.Duration(m.Intervalo) * time.Millisecond)
		}
	}
}
