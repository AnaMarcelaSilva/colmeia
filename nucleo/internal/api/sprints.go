package api

import (
	"net/http"
	"os/exec"
	"strconv"
	"time"

	"github.com/AnaMarcelaSilva/colmeia/nucleo/internal/claudecode"
	"github.com/AnaMarcelaSilva/colmeia/nucleo/internal/dados"
	"github.com/AnaMarcelaSilva/colmeia/nucleo/internal/linha"
	"github.com/AnaMarcelaSilva/colmeia/nucleo/internal/proposta"
)

func (s *Servidor) rotasSprints(mux *http.ServeMux) {
	mux.HandleFunc("GET /v1/perfis/{id}/sprints", s.listarSprints)
	mux.HandleFunc("POST /v1/perfis/{id}/sprints", s.criarSprint)
	mux.HandleFunc("PATCH /v1/sprints/{id}", s.editarSprint)
	mux.HandleFunc("DELETE /v1/sprints/{id}", s.removerSprint)
	mux.HandleFunc("PUT /v1/sprints/{id}/titulos/{tarefa}", s.definirTituloSprint)
	mux.HandleFunc("POST /v1/sprints/{id}/assuntos", s.criarAssunto)
	mux.HandleFunc("POST /v1/sprints/{id}/assuntos/repetir", s.repetirAssuntos)
	mux.HandleFunc("PUT /v1/sprints/{id}/assuntos/tarefas", s.definirAssunto)
	mux.HandleFunc("PATCH /v1/assuntos/{id}", s.renomearAssunto)
	mux.HandleFunc("DELETE /v1/assuntos/{id}", s.removerAssunto)
	mux.HandleFunc("PUT /v1/assuntos/{id}/texto", s.definirTextoDoAssunto)
	mux.HandleFunc("POST /v1/assuntos/{id}/texto/gerar", s.gerarTextoDoAssunto)
	mux.HandleFunc("PUT /v1/sprints/{id}/fora/{tarefa}", s.foraDaSprint)
	mux.HandleFunc("POST /v1/perfis/{id}/sprints/gerar", s.gerarSprint)
	mux.HandleFunc("POST /v1/propostas/{id}/aceitar", s.aceitarProposta)
	mux.HandleFunc("DELETE /v1/propostas/{id}", s.descartarProposta)
}

// listarSprints traz as sprints do perfil e a atual (a que tem hoje), que é
// criada na hora se ainda não existir: a sprint segue sozinha o ritmo da
// anterior, sem ninguém precisar abrir uma nova toda semana.
func (s *Servidor) listarSprints(w http.ResponseWriter, r *http.Request) {
	perfil, err := idDaRota(r)
	if err != nil {
		responderErro(w, err)
		return
	}
	if _, err := s.Banco.Perfil(r.Context(), perfil); err != nil {
		responderErro(w, err)
		return
	}
	atual, err := s.Banco.SprintAtual(r.Context(), perfil, time.Now().Format("2006-01-02"))
	if err != nil {
		responderErro(w, err)
		return
	}
	lista, err := s.Banco.ListarSprints(r.Context(), perfil)
	if err != nil {
		responderErro(w, err)
		return
	}
	responderJSON(w, map[string]any{"sprints": lista, "atual": atual.ID})
}

type datasSprint struct {
	Inicio string `json:"inicio"`
	Fim    string `json:"fim"`
}

func (s *Servidor) criarSprint(w http.ResponseWriter, r *http.Request) {
	perfil, err := idDaRota(r)
	if err != nil {
		responderErro(w, err)
		return
	}
	var pedido datasSprint
	if err := ler(r, &pedido); err != nil {
		responderErro(w, err)
		return
	}
	sprint, err := s.Banco.CriarSprint(r.Context(), perfil, pedido.Inicio, pedido.Fim)
	if err != nil {
		responderErro(w, err)
		return
	}
	responderJSON(w, sprint)
}

func (s *Servidor) editarSprint(w http.ResponseWriter, r *http.Request) {
	id, err := idDaRota(r)
	if err != nil {
		responderErro(w, err)
		return
	}
	var pedido datasSprint
	if err := ler(r, &pedido); err != nil {
		responderErro(w, err)
		return
	}
	sprint, err := s.Banco.EditarSprint(r.Context(), id, pedido.Inicio, pedido.Fim)
	if err != nil {
		responderErro(w, err)
		return
	}
	responderJSON(w, sprint)
}

func (s *Servidor) removerSprint(w http.ResponseWriter, r *http.Request) {
	id, err := idDaRota(r)
	if err != nil {
		responderErro(w, err)
		return
	}
	if err := s.Banco.RemoverSprint(r.Context(), id); err != nil {
		responderErro(w, err)
		return
	}
	w.WriteHeader(http.StatusNoContent)
}

// definirTituloSprint: {"titulo": "..."}; vazio volta ao título da tarefa.
func (s *Servidor) definirTituloSprint(w http.ResponseWriter, r *http.Request) {
	id, err := idDaRota(r)
	if err != nil {
		responderErro(w, err)
		return
	}
	tarefa, err := strconv.ParseInt(r.PathValue("tarefa"), 10, 64)
	if err != nil || tarefa <= 0 {
		responderErro(w, dados.ErrNaoEncontrado)
		return
	}
	var pedido struct {
		Titulo string `json:"titulo"`
	}
	if err := ler(r, &pedido); err != nil {
		responderErro(w, err)
		return
	}
	if err := s.Banco.DefinirTituloSprint(r.Context(), id, tarefa, pedido.Titulo); err != nil {
		responderErro(w, err)
		return
	}
	w.WriteHeader(http.StatusNoContent)
}

type nomeAssunto struct {
	Nome string `json:"nome"`
}

func (s *Servidor) criarAssunto(w http.ResponseWriter, r *http.Request) {
	id, err := idDaRota(r)
	if err != nil {
		responderErro(w, err)
		return
	}
	var pedido nomeAssunto
	if err := ler(r, &pedido); err != nil {
		responderErro(w, err)
		return
	}
	a, err := s.Banco.CriarAssunto(r.Context(), id, pedido.Nome)
	if err != nil {
		responderErro(w, err)
		return
	}
	responderJSON(w, a)
}

func (s *Servidor) renomearAssunto(w http.ResponseWriter, r *http.Request) {
	id, err := idDaRota(r)
	if err != nil {
		responderErro(w, err)
		return
	}
	var pedido nomeAssunto
	if err := ler(r, &pedido); err != nil {
		responderErro(w, err)
		return
	}
	a, err := s.Banco.RenomearAssunto(r.Context(), id, pedido.Nome)
	if err != nil {
		responderErro(w, err)
		return
	}
	responderJSON(w, a)
}

func (s *Servidor) removerAssunto(w http.ResponseWriter, r *http.Request) {
	id, err := idDaRota(r)
	if err != nil {
		responderErro(w, err)
		return
	}
	if err := s.Banco.RemoverAssunto(r.Context(), id); err != nil {
		responderErro(w, err)
		return
	}
	w.WriteHeader(http.StatusNoContent)
}

// definirAssunto: {"tarefas": [1, 2], "assunto": 3}; assunto 0 tira do assunto.
func (s *Servidor) definirAssunto(w http.ResponseWriter, r *http.Request) {
	id, err := idDaRota(r)
	if err != nil {
		responderErro(w, err)
		return
	}
	var pedido struct {
		Tarefas []int64 `json:"tarefas"`
		Assunto int64   `json:"assunto"`
	}
	if err := ler(r, &pedido); err != nil {
		responderErro(w, err)
		return
	}
	if err := s.Banco.DefinirAssunto(r.Context(), id, pedido.Tarefas, pedido.Assunto); err != nil {
		responderErro(w, err)
		return
	}
	w.WriteHeader(http.StatusNoContent)
}

// repetirAssuntos traz os assuntos da sprint anterior; responde quantos vieram.
func (s *Servidor) repetirAssuntos(w http.ResponseWriter, r *http.Request) {
	id, err := idDaRota(r)
	if err != nil {
		responderErro(w, err)
		return
	}
	n, err := s.Banco.RepetirAssuntos(r.Context(), id)
	if err != nil {
		responderErro(w, err)
		return
	}
	responderJSON(w, map[string]int{"assuntos": n})
}

// periodoDaSprint lê o período do resumo e do deck da sprint: sprint=<id>
// (uma sprint fixa do perfil, com os títulos dela no contexto) ou, como
// antes, ultimos=, mes= ou de= e ate=. Devolve o id da sprint (0 sem ela).
func (s *Servidor) periodoDaSprint(r *http.Request, c *linha.Contexto) (time.Time, time.Time, int64, error) {
	perfil, _ := idDaRota(r)
	return s.periodoDaSprintDe(r, perfil, c)
}

// periodoDaSprintDe é periodoDaSprint para o perfil dado (a rota pode não
// ser a do perfil).
func (s *Servidor) periodoDaSprintDe(r *http.Request, perfil int64, c *linha.Contexto) (time.Time, time.Time, int64, error) {
	v := r.URL.Query().Get("sprint")
	if v == "" {
		de, ate, err := periodo(r, true)
		return de, ate, 0, err
	}
	id, err := strconv.ParseInt(v, 10, 64)
	if err != nil || id <= 0 {
		return time.Time{}, time.Time{}, 0, dados.ErrInvalido{Motivo: "sprint inválida"}
	}
	sprint, err := s.Banco.SprintPorID(r.Context(), id)
	if err != nil {
		return time.Time{}, time.Time{}, 0, err
	}
	if sprint.Perfil != perfil {
		return time.Time{}, time.Time{}, 0, dados.ErrNaoEncontrado
	}
	de, _ := time.ParseInLocation("2006-01-02", sprint.Inicio, time.Local)
	ate, _ := time.ParseInLocation("2006-01-02", sprint.Fim, time.Local)
	if c.TitulosSprint, err = s.Banco.TitulosDaSprint(r.Context(), id); err != nil {
		return time.Time{}, time.Time{}, 0, err
	}
	if c.Assuntos, c.AssuntoDa, err = s.Banco.AssuntosDaSprint(r.Context(), id); err != nil {
		return time.Time{}, time.Time{}, 0, err
	}
	if c.ForaDaSprint, err = s.Banco.ForaDaSprint(r.Context(), id); err != nil {
		return time.Time{}, time.Time{}, 0, err
	}
	return de, ate, id, nil
}

// foraDaSprint tira a tarefa da sprint fixa ({"fora": true}) ou a traz de
// volta. A daily, o quadro e a linha do tempo não mudam.
func (s *Servidor) foraDaSprint(w http.ResponseWriter, r *http.Request) {
	sprint, err := idDaRota(r)
	if err != nil {
		responderErro(w, err)
		return
	}
	tarefa, err := strconv.ParseInt(r.PathValue("tarefa"), 10, 64)
	if err != nil || tarefa <= 0 {
		responderErro(w, dados.ErrInvalido{Motivo: "tarefa inválida"})
		return
	}
	var pedido struct {
		Fora *bool `json:"fora"`
	}
	if err := ler(r, &pedido); err != nil {
		responderErro(w, err)
		return
	}
	if pedido.Fora == nil {
		responderErro(w, dados.ErrInvalido{Motivo: "informe fora (true ou false)"})
		return
	}
	if err := s.Banco.TirarDaSprint(r.Context(), sprint, tarefa, *pedido.Fora); err != nil {
		responderErro(w, err)
		return
	}
	w.WriteHeader(http.StatusNoContent)
}

// definirTextoDoAssunto grava a fala do assunto escrita por você
// ({"texto": "..."}; vazio apaga) e responde o assunto.
func (s *Servidor) definirTextoDoAssunto(w http.ResponseWriter, r *http.Request) {
	id, err := idDaRota(r)
	if err != nil {
		responderErro(w, err)
		return
	}
	var pedido struct {
		Texto string `json:"texto"`
	}
	if err := ler(r, &pedido); err != nil {
		responderErro(w, err)
		return
	}
	a, err := s.Banco.DefinirTextoDoAssunto(r.Context(), id, pedido.Texto, dados.OrigemTextoVoce, 0)
	if err != nil {
		responderErro(w, err)
		return
	}
	responderJSON(w, a)
}

// gerarTextoDoAssunto pede ao Claude Code, sem terminal, a fala do assunto
// com as tarefas dele que ficaram na sprint (no recorte do pedido, como o
// deck: projeto= ou workspace=), com um {"pedido": "..."} opcional, grava e
// responde o assunto. Espera a resposta, até o prazo.
func (s *Servidor) gerarTextoDoAssunto(w http.ResponseWriter, r *http.Request) {
	id, err := idDaRota(r)
	if err != nil {
		responderErro(w, err)
		return
	}
	var pedido struct {
		Pedido string `json:"pedido"`
	}
	if err := ler(r, &pedido); err != nil {
		responderErro(w, err)
		return
	}
	executavel, err := exec.LookPath(claudecode.Executavel)
	if err != nil {
		responderErro(w, dados.ErrInvalido{Motivo: claudecode.SemClaude})
		return
	}
	a, err := s.Banco.AssuntoPorID(r.Context(), id)
	if err != nil {
		responderErro(w, err)
		return
	}
	sprint, err := s.Banco.SprintPorID(r.Context(), a.Sprint)
	if err != nil {
		responderErro(w, err)
		return
	}
	// O deck da sprint do assunto, no recorte pedido: ele já deixa de fora
	// as tarefas tiradas da sprint.
	r2 := r.Clone(r.Context())
	q := r2.URL.Query()
	q.Set("sprint", strconv.FormatInt(sprint.ID, 10))
	r2.URL.RawQuery = q.Encode()
	deck, err := s.montarDeckDe(r2, sprint.Perfil, "sprint")
	if err != nil {
		responderErro(w, err)
		return
	}
	entrada := proposta.Assunto{Nome: a.Nome, Inicio: sprint.Inicio, Fim: sprint.Fim, Tarefas: []proposta.Tarefa{}}
	for _, t := range entradaDaSprint(deck, sprint).Tarefas {
		if t.Assunto == a.Nome {
			entrada.Tarefas = append(entrada.Tarefas, t)
		}
	}
	env, err := s.contaDoClaude(r.Context(), sprint.Perfil)
	if err != nil {
		responderErro(w, err)
		return
	}
	if !s.propostas.comecar() {
		responderErro(w, dados.ErrInvalido{Motivo: "já estou gerando um texto; espere ele chegar"})
		return
	}
	texto, err := proposta.Gerador{Executavel: executavel, Env: env}.GerarTexto(r.Context(), pedido.Pedido, entrada)
	s.propostas.terminar()
	if err != nil {
		responderErro(w, err)
		return
	}
	a, err = s.Banco.DefinirTextoDoAssunto(r.Context(), id, texto, dados.OrigemTextoIA, len(entrada.Tarefas))
	if err != nil {
		responderErro(w, err)
		return
	}
	responderJSON(w, a)
}
