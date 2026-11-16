package api

import (
	"net/http"
	"strconv"

	"github.com/AnaMarcelaSilva/colmeia/nucleo/internal/dados"
)

// A pauta da reunião: a ordem das tarefas, o visto de "apresentada" e o
// "Daily apresentada" que fecha a daily de hoje. A tela recebe pauta.mudou
// e daily.apresentada e busca o deck de novo.

func (s *Servidor) rotasPauta(mux *http.ServeMux) {
	mux.HandleFunc("PUT /v1/perfis/{id}/pauta", s.ordenarPauta)
	mux.HandleFunc("PUT /v1/perfis/{id}/pauta/{tarefa}", s.marcarApresentada)
	mux.HandleFunc("POST /v1/perfis/{id}/daily/apresentada", s.fecharDaily)
}

// ordenarPauta guarda a ordem das tarefas de uma reunião ({tipo, chave,
// ordem: [ids]}); a chave é a do período das notas (chave_nota do deck).
func (s *Servidor) ordenarPauta(w http.ResponseWriter, r *http.Request) {
	perfil, err := idDaRota(r)
	if err != nil {
		responderErro(w, err)
		return
	}
	var pedido struct {
		Tipo  string  `json:"tipo"`
		Chave string  `json:"chave"`
		Ordem []int64 `json:"ordem"`
	}
	if err := ler(r, &pedido); err != nil {
		responderErro(w, err)
		return
	}
	if len(pedido.Ordem) > 500 {
		responderErro(w, dados.ErrInvalido{Motivo: "a pauta tem tarefas demais"})
		return
	}
	if err := s.Banco.OrdenarPauta(r.Context(), perfil, pedido.Tipo, pedido.Chave, pedido.Ordem); err != nil {
		responderErro(w, err)
		return
	}
	w.WriteHeader(http.StatusNoContent)
}

// marcarApresentada marca ou desmarca a tarefa como apresentada ({tipo,
// chave, apresentada}).
func (s *Servidor) marcarApresentada(w http.ResponseWriter, r *http.Request) {
	perfil, err := idDaRota(r)
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
		Tipo        string `json:"tipo"`
		Chave       string `json:"chave"`
		Apresentada *bool  `json:"apresentada"`
	}
	if err := ler(r, &pedido); err != nil {
		responderErro(w, err)
		return
	}
	if pedido.Apresentada == nil {
		responderErro(w, dados.ErrInvalido{Motivo: "informe apresentada (true ou false)"})
		return
	}
	if err := s.Banco.MarcarApresentada(r.Context(), perfil, pedido.Tipo, pedido.Chave, tarefa, *pedido.Apresentada); err != nil {
		responderErro(w, err)
		return
	}
	w.WriteHeader(http.StatusNoContent)
}

// fecharDaily marca a daily do dia ({dia}; sem corpo, hoje) como
// apresentada agora e responde {"em": hora}.
func (s *Servidor) fecharDaily(w http.ResponseWriter, r *http.Request) {
	perfil, err := idDaRota(r)
	if err != nil {
		responderErro(w, err)
		return
	}
	var pedido struct {
		Dia string `json:"dia"`
	}
	if r.ContentLength != 0 {
		if err := ler(r, &pedido); err != nil {
			responderErro(w, err)
			return
		}
	}
	if pedido.Dia == "" {
		pedido.Dia = hoje()
	}
	em, err := s.Banco.FecharDaily(r.Context(), perfil, pedido.Dia)
	if err != nil {
		responderErro(w, err)
		return
	}
	responderJSON(w, map[string]any{"em": em})
}
