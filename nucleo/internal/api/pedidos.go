package api

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"log"
	"net/http"
	"os/exec"
	"strings"
	"time"

	"github.com/AnaMarcelaSilva/colmeia/nucleo/internal/dados"
	"github.com/AnaMarcelaSilva/colmeia/nucleo/internal/processos"
	"github.com/AnaMarcelaSilva/colmeia/nucleo/internal/sessoes"
	"github.com/AnaMarcelaSilva/colmeia/nucleo/internal/terminal"
)

// SemDigitar: o pedido só entra no terminal depois desse tempo sem você
// digitar nele, para não se juntar a um texto pela metade.
var SemDigitar = 5 * time.Second

func (s *Servidor) rotasPedidos(mux *http.ServeMux) {
	mux.HandleFunc("GET /v1/tarefas/{id}/pedidos", s.listarPedidos)
	mux.HandleFunc("GET /v1/tarefas/{id}/pedidos/destino", s.destinoDoPedido)
	mux.HandleFunc("POST /v1/tarefas/{id}/pedidos", s.criarPedido)
	mux.HandleFunc("DELETE /v1/pedidos/{id}", s.cancelarPedido)
	mux.HandleFunc("GET /v1/perfis/{id}/pedidos", s.pedidosAbertosDoPerfil)
}

// Destino diz à tela, antes do Enviar, para quem o pedido vai.
type Destino struct {
	// Acao: "ativo" (um Claude Code rodando), "reiniciar" (um parado, que
	// volta com a conversa), "novo" (abre um) ou "bloqueado".
	Acao   string           `json:"acao"`
	Agente *agenteComEstado `json:"agente,omitempty"`
	// Conversa: o título da conversa que um agente novo retoma ("" é uma nova).
	Conversa string `json:"conversa,omitempty"`
	// Outros: a tarefa tem agentes, mas nenhum é Claude Code.
	Outros bool   `json:"outros,omitempty"`
	Motivo string `json:"motivo,omitempty"`
	sessao string
}

// destino decide qual agente recebe o pedido: um Claude Code ativo da tarefa
// (de preferência um que espera você), senão um parado (reiniciado), senão
// um novo, que retoma a última conversa da pasta.
func (s *Servidor) destino(ctx context.Context, tarefa int64) (Destino, error) {
	t, p, perfil, err := s.Banco.Tarefa(ctx, tarefa)
	if err != nil {
		return Destino{}, err
	}
	pasta := t.Pasta(p)
	agentes, err := s.Banco.ListarAgentes(ctx, tarefa)
	if err != nil {
		return Destino{}, err
	}
	var ativo, parado *agenteComEstado
	outros := false
	for _, a := range agentes {
		if a.Ferramenta != "claude" {
			outros = true
			continue
		}
		e := s.comEstado(a, pasta, nil)
		switch {
		case e.Ativo && (ativo == nil || (e.Estado == terminal.Aguardando && ativo.Estado != terminal.Aguardando)):
			ativo = &e
		case !e.Ativo:
			parado = &e
		}
	}
	if ativo != nil {
		return Destino{Acao: "ativo", Agente: ativo}, nil
	}
	if _, err := exec.LookPath("claude"); err != nil {
		return Destino{Acao: "bloqueado", Motivo: "O Claude Code não está instalado."}, nil
	}
	if processos.RodandoFora("claude", pasta) {
		return Destino{Acao: "bloqueado", Motivo: "O Claude Code está aberto nessa pasta fora da Colmeia. Feche-o ou peça por lá."}, nil
	}
	if parado != nil {
		return Destino{Acao: "reiniciar", Agente: parado}, nil
	}
	d := Destino{Acao: "novo", Outros: outros}
	separada, err := s.pastaSeparada(ctx, perfil, "claude")
	if err != nil {
		return d, err
	}
	if configuracao, err := sessoes.PastaDeConfiguracao(separada); err == nil {
		if lista, err := sessoes.Listar(configuracao, pasta); err == nil && len(lista) > 0 {
			d.Conversa, d.sessao = lista[0].Titulo, lista[0].ID
		}
	}
	return d, nil
}

func (s *Servidor) destinoDoPedido(w http.ResponseWriter, r *http.Request) {
	id, err := idDaRota(r)
	if err != nil {
		responderErro(w, err)
		return
	}
	d, err := s.destino(r.Context(), id)
	if err != nil {
		responderErro(w, err)
		return
	}
	responderJSON(w, d)
}

func (s *Servidor) listarPedidos(w http.ResponseWriter, r *http.Request) {
	id, err := idDaRota(r)
	if err != nil {
		responderErro(w, err)
		return
	}
	if _, _, _, err := s.Banco.Tarefa(r.Context(), id); err != nil {
		responderErro(w, err)
		return
	}
	lista, err := s.Banco.ListarPedidos(r.Context(), id, r.URL.Query().Get("tipo"), r.URL.Query().Get("periodo"))
	if err != nil {
		responderErro(w, err)
		return
	}
	responderJSON(w, lista)
}

func (s *Servidor) pedidosAbertosDoPerfil(w http.ResponseWriter, r *http.Request) {
	id, err := idDaRota(r)
	if err != nil {
		responderErro(w, err)
		return
	}
	lista, err := s.Banco.PedidosAbertosDoPerfil(r.Context(), id)
	if err != nil {
		responderErro(w, err)
		return
	}
	responderJSON(w, lista)
}

// criarPedido põe o pedido na fila do agente escolhido pelo núcleo,
// iniciando (ou criando) um Claude Code se preciso. O pedido entra no
// terminal quando o agente espera sua resposta (veja tentarEntregar).
func (s *Servidor) criarPedido(w http.ResponseWriter, r *http.Request) {
	id, err := idDaRota(r)
	if err != nil {
		responderErro(w, err)
		return
	}
	var pedido struct {
		Texto, Tipo, Periodo string
		Cols, Rows           uint16
	}
	if err := ler(r, &pedido); err != nil {
		responderErro(w, err)
		return
	}
	if err := dados.PeriodoNotaValido(pedido.Tipo, pedido.Periodo); err != nil {
		responderErro(w, err)
		return
	}
	if _, err := dados.TextoDePedido(pedido.Texto); err != nil {
		responderErro(w, err)
		return
	}
	ctx := r.Context()
	d, err := s.destino(ctx, id)
	if err != nil {
		responderErro(w, err)
		return
	}
	tamanho := terminal.Tamanho{Colunas: pedido.Cols, Linhas: pedido.Rows}
	var agente int64
	switch d.Acao {
	case "bloqueado":
		w.Header().Set("Content-Type", "application/json")
		w.WriteHeader(http.StatusConflict)
		json.NewEncoder(w).Encode(map[string]string{"erro": d.Motivo})
		return
	case "ativo", "reiniciar":
		agente = d.Agente.ID
		if d.Acao == "reiniciar" {
			if err := s.abrirTerminal(ctx, d.Agente.Agente, tamanho); err != nil {
				responderErro(w, err)
				return
			}
		}
	case "novo":
		sessao := d.sessao
		if sessao == "" {
			sessao = novoUUID()
		}
		a, err := s.Banco.CriarAgente(ctx, id, "claude", "dev", sessao)
		if err != nil {
			responderErro(w, err)
			return
		}
		if err := s.abrirTerminal(ctx, a, tamanho); err != nil {
			s.Banco.RemoverAgente(context.WithoutCancel(ctx), a.ID)
			responderErro(w, err)
			return
		}
		agente = a.ID
	}
	p, err := s.Banco.CriarPedido(ctx, id, agente, pedido.Tipo, pedido.Periodo, pedido.Texto)
	if err != nil {
		responderErro(w, err)
		return
	}
	s.tentarEntregar(agente)
	if atual, err := s.Banco.Pedido(ctx, p.ID); err == nil {
		p = atual
	}
	responderJSON(w, map[string]any{"pedido": p, "destino": d})
}

func (s *Servidor) cancelarPedido(w http.ResponseWriter, r *http.Request) {
	id, err := idDaRota(r)
	if err != nil {
		responderErro(w, err)
		return
	}
	p, err := s.Banco.MudarPedido(r.Context(), id, []string{dados.PedidoFila}, dados.PedidoCancelado, "")
	if errors.Is(err, dados.ErrPedidoFechado) {
		responderErro(w, dados.ErrInvalido{Motivo: "o pedido já foi entregue ao agente e não dá mais para cancelar"})
		return
	}
	if err != nil {
		responderErro(w, err)
		return
	}
	responderJSON(w, p)
}

// textoParaOAgente é o que entra no terminal do agente.
func textoParaOAgente(p dados.Pedido) string {
	onde := "daily de " + dataCurta(p.Periodo)
	if p.Tipo == "sprint" {
		de, ate, _ := strings.Cut(p.Periodo, "..")
		onde = "sprint de " + dataCurta(de) + " a " + dataCurta(ate)
	}
	return fmt.Sprintf("[Pedido da %s · tarefa #%d · pedido %d] %s\n"+
		"Responda pelas ferramentas da Colmeia: complementar_nota (tipo %s, período %s, pedido %d), "+
		"anexe as capturas se houver (capturar_navegador ou anexar_imagem) e, só depois de tudo, concluir_pedido %d. "+
		"Seja breve (até 5 linhas): a nota aparece num slide.",
		onde, p.TarefaID, p.ID, p.Texto, p.Tipo, p.Periodo, p.ID, p.ID)
}

func dataCurta(dia string) string {
	t, err := time.Parse("2006-01-02", dia)
	if err != nil {
		return dia
	}
	return t.Format("02/01")
}

// tentarEntregar escreve no terminal o próximo pedido da fila do agente, se
// ele espera sua resposta agora. Nunca durante um pedido de aprovação (o
// texto responderia a pergunta) e nunca com você digitando nele há menos de
// SemDigitar. É chamado na criação do pedido e a cada vez que o agente volta
// a esperar: nada de consulta periódica.
func (s *Servidor) tentarEntregar(agente int64) {
	s.muEntrega.Lock()
	defer s.muEntrega.Unlock()
	sessao, ok := s.Agentes.Pegar(agente)
	if !ok || sessao.Encerrada() {
		return
	}
	estado, motivo, desde := sessao.Estado()
	if estado != terminal.Aguardando || motivo != terminal.EsperandoResposta {
		return
	}
	ctx := context.Background()
	abertos, err := s.Banco.PedidosAbertosDoAgente(ctx, agente)
	if err != nil || len(abertos) == 0 {
		return
	}
	var proximo *dados.Pedido
	for i, p := range abertos {
		if p.Estado == dados.PedidoEntregue {
			// Um pedido entregue depois que o agente parou: ele ainda nem começou.
			if t, err := time.Parse(time.RFC3339Nano, p.EntregueEm); err == nil && t.After(desde) {
				return
			}
			continue
		}
		if proximo == nil {
			proximo = &abertos[i]
		}
	}
	if proximo == nil {
		return
	}
	if falta := SemDigitar - time.Since(sessao.UltimaEntrada()); falta > 0 {
		if t := s.adiadas[agente]; t != nil {
			t.Reset(falta)
		} else {
			s.adiadas[agente] = time.AfterFunc(falta, func() { s.tentarEntregar(agente) })
		}
		return
	}
	if _, err := s.Banco.MudarPedido(ctx, proximo.ID, []string{dados.PedidoFila}, dados.PedidoEntregue, ""); err != nil {
		return
	}
	texto := append(terminal.Colagem(textoParaOAgente(*proximo), sessao.ColagemLigada()), '\r')
	if _, err := sessao.Escrever(texto); err != nil {
		log.Printf("pedido %d: escrevendo no terminal: %v", proximo.ID, err)
	}
}

// fecharRespondidos: o agente terminou a vez (voltou a esperar você desde
// `desde`) depois de responder um pedido pela nota, sem concluir_pedido. O
// pedido fecha agora, com tudo já na nota. Roda antes de a tela saber do
// novo estado, para o "respondeu" chegar antes do "precisa de você".
func (s *Servidor) fecharRespondidos(ctx context.Context, agente int64, desde time.Time) {
	s.muEntrega.Lock()
	defer s.muEntrega.Unlock()
	if len(s.respondendo) == 0 {
		return
	}
	abertos, err := s.Banco.PedidosAbertosDoAgente(ctx, agente)
	if err != nil {
		return
	}
	for _, p := range abertos {
		if p.Estado != dados.PedidoEntregue || !s.respondendo[p.ID] {
			continue
		}
		if t, err := time.Parse(time.RFC3339Nano, p.EntregueEm); err != nil || t.After(desde) {
			continue
		}
		delete(s.respondendo, p.ID)
		if _, err := s.Banco.MudarPedido(ctx, p.ID, []string{dados.PedidoEntregue}, dados.PedidoRespondido, ""); err != nil && !errors.Is(err, dados.ErrPedidoFechado) {
			log.Printf("pedido %d: marcando respondido: %v", p.ID, err)
		}
	}
}

// marcarRespondendo guarda que o agente começou a responder o pedido.
func (s *Servidor) marcarRespondendo(pedido int64) {
	s.muEntrega.Lock()
	s.respondendo[pedido] = true
	s.muEntrega.Unlock()
}

// responderPedido fecha o pedido como respondido (concluir_pedido).
func (s *Servidor) responderPedido(ctx context.Context, pedido int64) (dados.Pedido, error) {
	s.muEntrega.Lock()
	delete(s.respondendo, pedido)
	s.muEntrega.Unlock()
	return s.Banco.MudarPedido(ctx, pedido, []string{dados.PedidoFila, dados.PedidoEntregue}, dados.PedidoRespondido, "")
}

// falharPedidosDoAgente marca como falhos os pedidos abertos de um agente
// que terminou ou foi removido.
func (s *Servidor) falharPedidosDoAgente(ctx context.Context, agente int64, motivo string) {
	s.muEntrega.Lock()
	if t := s.adiadas[agente]; t != nil {
		t.Stop()
		delete(s.adiadas, agente)
	}
	s.muEntrega.Unlock()
	abertos, err := s.Banco.PedidosAbertosDoAgente(ctx, agente)
	if err != nil {
		return
	}
	for _, p := range abertos {
		s.muEntrega.Lock()
		delete(s.respondendo, p.ID)
		s.muEntrega.Unlock()
		s.Banco.MudarPedido(ctx, p.ID, []string{dados.PedidoFila, dados.PedidoEntregue}, dados.PedidoFalhou, motivo)
	}
}

// motivoDaFalha explica por que um pedido não teve resposta.
func motivoDaFalha(motivo string) string {
	switch motivo {
	case terminal.PelaRemocao:
		return "o agente foi parado pela Colmeia"
	case terminal.PeloDesligar:
		return "o núcleo foi encerrado"
	case "erro":
		return "o agente parou com erro"
	}
	return "o agente foi encerrado"
}
