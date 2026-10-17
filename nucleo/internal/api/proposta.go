package api

import (
	"context"
	"net/http"
	"os/exec"
	"strings"
	"sync"
	"time"

	"github.com/AnaMarcelaSilva/colmeia/nucleo/internal/claudecode"
	"github.com/AnaMarcelaSilva/colmeia/nucleo/internal/dados"
	"github.com/AnaMarcelaSilva/colmeia/nucleo/internal/linha"
	"github.com/AnaMarcelaSilva/colmeia/nucleo/internal/proposta"
)

// "Gerar sprint com IA": o núcleo pede ao Claude Code (sem terminal, num
// pedido só) títulos, descrições e assuntos para a sprint, e guarda a
// proposta na memória. Nada é gravado até a tela aceitar; aceitar aplica
// exatamente o que foi conferido aqui (a tela só manda o número).

const (
	maxPropostas     = 8
	validadeProposta = time.Hour
)

type propostaGuardada struct {
	sprint dados.Sprint
	itens  []proposta.Item
	// O título de cada tarefa: o mesmo na proposta apaga o título da sprint.
	tituloDaTarefa map[int64]string
	criada         time.Time
}

type propostas struct {
	mu        sync.Mutex
	proxima   int64
	guardadas map[int64]*propostaGuardada
	// Uma geração por vez: cada uma gasta tokens da conta.
	gerando bool
}

func (p *propostas) comecar() bool {
	p.mu.Lock()
	defer p.mu.Unlock()
	if p.gerando {
		return false
	}
	p.gerando = true
	return true
}

func (p *propostas) terminar() {
	p.mu.Lock()
	p.gerando = false
	p.mu.Unlock()
}

// guardar fica com as mais novas: a mais antiga sai quando passa do limite.
func (p *propostas) guardar(g *propostaGuardada) int64 {
	p.mu.Lock()
	defer p.mu.Unlock()
	if p.guardadas == nil {
		p.guardadas = map[int64]*propostaGuardada{}
	}
	p.proxima++
	p.guardadas[p.proxima] = g
	delete(p.guardadas, p.proxima-maxPropostas)
	return p.proxima
}

func (p *propostas) pegar(id int64) (*propostaGuardada, bool) {
	p.mu.Lock()
	defer p.mu.Unlock()
	g, ok := p.guardadas[id]
	if ok && time.Since(g.criada) > validadeProposta {
		delete(p.guardadas, id)
		return nil, false
	}
	return g, ok
}

func (p *propostas) remover(id int64) bool {
	p.mu.Lock()
	defer p.mu.Unlock()
	_, ok := p.guardadas[id]
	delete(p.guardadas, id)
	return ok
}

// entradaDaSprint: os dados da sprint que vão no pedido, tirados do deck.
func entradaDaSprint(deck linha.Deck, sprint dados.Sprint) proposta.Sprint {
	e := proposta.Sprint{Inicio: sprint.Inicio, Fim: sprint.Fim, Assuntos: []string{}, Tarefas: []proposta.Tarefa{}}
	for _, a := range deck.Assuntos {
		e.Assuntos = append(e.Assuntos, a.Nome)
	}
	for _, s := range deck.Slides {
		if s.Removida {
			continue
		}
		t := proposta.Tarefa{ID: s.TarefaID, Titulo: s.Titulo, TituloDaTarefa: s.TituloOriginal, Projeto: s.Projeto, Estado: s.Status, Descricao: s.Nota}
		if s.AssuntoID != 0 {
			t.Assunto = s.Secao
		}
		var feito []string
		for _, f := range s.Feito {
			feito = append(feito, f.Itens...)
		}
		t.Feito = proposta.Resumir(feito)
		e.Tarefas = append(e.Tarefas, t)
	}
	return e
}

// itemProposta é o que a tela recebe: a proposta e, ao lado, como está.
type itemProposta struct {
	proposta.Item
	Projeto string `json:"projeto"`
	Antes   struct {
		Titulo    string `json:"titulo"`
		Descricao string `json:"descricao"`
		Assunto   string `json:"assunto"`
	} `json:"antes"`
}

// gerarSprint: POST /v1/perfis/{id}/sprints/gerar?sprint=N (com o recorte,
// como o deck) e {"pedido": "..."}. Espera o Claude Code responder (até o
// prazo) e devolve a proposta com o número para aceitar ou descartar.
func (s *Servidor) gerarSprint(w http.ResponseWriter, r *http.Request) {
	perfil, err := idDaRota(r)
	if err != nil {
		responderErro(w, err)
		return
	}
	if r.URL.Query().Get("sprint") == "" {
		responderErro(w, dados.ErrInvalido{Motivo: "escolha uma sprint"})
		return
	}
	var pedido struct {
		Pedido string `json:"pedido"`
	}
	if err := ler(r, &pedido); err != nil {
		responderErro(w, err)
		return
	}
	texto, err := proposta.ConferirPedido(pedido.Pedido)
	if err != nil {
		responderErro(w, err)
		return
	}
	executavel, err := exec.LookPath(claudecode.Executavel)
	if err != nil {
		responderErro(w, dados.ErrInvalido{Motivo: claudecode.SemClaude})
		return
	}
	deck, err := s.montarDeck(r, "sprint")
	if err != nil {
		responderErro(w, err)
		return
	}
	sprint, err := s.Banco.SprintPorID(r.Context(), deck.SprintID)
	if err != nil {
		responderErro(w, err)
		return
	}
	entrada := entradaDaSprint(deck, sprint)
	// A mesma conta que os agentes do perfil usam.
	env, err := s.contaDoClaude(r.Context(), perfil)
	if err != nil {
		responderErro(w, err)
		return
	}
	if !s.propostas.comecar() {
		responderErro(w, dados.ErrInvalido{Motivo: "já estou gerando uma proposta; espere ela chegar"})
		return
	}
	p, err := proposta.Gerador{Executavel: executavel, Env: env}.Gerar(r.Context(), texto, entrada)
	s.propostas.terminar()
	if err != nil {
		responderErro(w, err)
		return
	}
	g := &propostaGuardada{sprint: sprint, itens: p.Itens, tituloDaTarefa: map[int64]string{}, criada: time.Now()}
	porID := map[int64]proposta.Tarefa{}
	for _, t := range entrada.Tarefas {
		porID[t.ID] = t
		g.tituloDaTarefa[t.ID] = cmpOr(t.TituloDaTarefa, t.Titulo)
	}
	itens := make([]itemProposta, 0, len(p.Itens))
	for _, it := range p.Itens {
		t := porID[it.TarefaID]
		ip := itemProposta{Item: it, Projeto: t.Projeto}
		ip.Antes.Titulo, ip.Antes.Descricao, ip.Antes.Assunto = t.Titulo, t.Descricao, t.Assunto
		itens = append(itens, ip)
	}
	responderJSON(w, map[string]any{"id": s.propostas.guardar(g), "itens": itens})
}

// aceitarProposta grava a proposta guardada: o título da sprint, a descrição
// (a nota da sprint) e o assunto de cada tarefa, criando os assuntos novos.
// Repetir é seguro (os mesmos nomes caem nos mesmos assuntos).
func (s *Servidor) aceitarProposta(w http.ResponseWriter, r *http.Request) {
	id, err := idDaRota(r)
	if err != nil {
		responderErro(w, err)
		return
	}
	g, ok := s.propostas.pegar(id)
	if !ok {
		responderErro(w, dados.ErrInvalido{Motivo: "a proposta não existe mais; gere de novo"})
		return
	}
	if err := s.aplicarProposta(r.Context(), g); err != nil {
		responderErro(w, err)
		return
	}
	s.propostas.remover(id)
	w.WriteHeader(http.StatusNoContent)
}

func (s *Servidor) aplicarProposta(ctx context.Context, g *propostaGuardada) error {
	existentes, _, err := s.Banco.AssuntosDaSprint(ctx, g.sprint.ID)
	if err != nil {
		return err
	}
	chave := func(nome string) string { return strings.ToLower(strings.TrimSpace(nome)) }
	porNome := map[string]int64{}
	for _, a := range existentes {
		porNome[chave(a.Nome)] = a.ID
	}
	var ordem []int64
	grupos := map[int64][]int64{}
	for _, it := range g.itens {
		assunto := int64(0)
		if it.Assunto != "" {
			var ok bool
			if assunto, ok = porNome[chave(it.Assunto)]; !ok {
				a, err := s.Banco.CriarAssunto(ctx, g.sprint.ID, it.Assunto)
				if err != nil {
					return err
				}
				assunto, porNome[chave(it.Assunto)] = a.ID, a.ID
			}
		}
		if _, ok := grupos[assunto]; !ok {
			ordem = append(ordem, assunto)
		}
		grupos[assunto] = append(grupos[assunto], it.TarefaID)
		titulo := it.Titulo
		if titulo == g.tituloDaTarefa[it.TarefaID] {
			titulo = ""
		}
		if err := s.Banco.DefinirTituloSprint(ctx, g.sprint.ID, it.TarefaID, titulo); err != nil {
			return err
		}
		if _, err := s.Banco.GravarNota(ctx, dados.GravacaoNota{Tarefa: it.TarefaID, Tipo: "sprint", Periodo: g.sprint.ChaveNota(), Texto: it.Descricao}); err != nil {
			return err
		}
	}
	for _, assunto := range ordem {
		if err := s.Banco.DefinirAssunto(ctx, g.sprint.ID, grupos[assunto], assunto); err != nil {
			return err
		}
	}
	return nil
}

func (s *Servidor) descartarProposta(w http.ResponseWriter, r *http.Request) {
	id, err := idDaRota(r)
	if err != nil {
		responderErro(w, err)
		return
	}
	if !s.propostas.remover(id) {
		responderErro(w, dados.ErrNaoEncontrado)
		return
	}
	w.WriteHeader(http.StatusNoContent)
}
