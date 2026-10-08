package api

import (
	"net/http"
	"strconv"
	"time"

	"github.com/AnaMarcelaSilva/colmeia/nucleo/internal/dados"
	"github.com/AnaMarcelaSilva/colmeia/nucleo/internal/linha"
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
	v := r.URL.Query().Get("sprint")
	if v == "" {
		de, ate, err := periodo(r, true)
		return de, ate, 0, err
	}
	id, err := strconv.ParseInt(v, 10, 64)
	if err != nil || id <= 0 {
		return time.Time{}, time.Time{}, 0, dados.ErrInvalido{Motivo: "sprint inválida"}
	}
	perfil, _ := idDaRota(r)
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
	return de, ate, id, nil
}
