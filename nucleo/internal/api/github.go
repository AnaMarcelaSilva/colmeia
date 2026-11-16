package api

import (
	"context"
	"errors"
	"net/http"
	"strings"

	"github.com/AnaMarcelaSilva/colmeia/nucleo/internal/dados"
	"github.com/AnaMarcelaSilva/colmeia/nucleo/internal/git"
)

// A branch da tarefa no GitHub: se foi enviada, o PR dela e os checks.
func (s *Servidor) rotasGithub(mux *http.ServeMux) {
	mux.HandleFunc("GET /v1/tarefas/{id}/github", s.situacaoNoGithub)
	mux.HandleFunc("POST /v1/tarefas/{id}/github/enviar", s.enviarAoGithub)
}

// situacaoGithub é o que a aba Arquivos mostra sobre a branch da tarefa.
type situacaoGithub struct {
	Branch string `json:"branch"`
	Base   string `json:"base"`
	// Se o repositório tem um remoto origin (sem ele, nada a enviar).
	Origin bool `json:"origin"`
	// "dono/repo" no GitHub; vazio com o origin em outro lugar.
	Repositorio string `json:"repositorio"`
	Enviada     bool   `json:"enviada"`
	AEnviar     int    `json:"a_enviar"`
	// As páginas do GitHub: abrir o PR da branch e a lista de PRs dela.
	Comparar string `json:"comparar"`
	Pulls    string `json:"pulls"`
	// Se o gh está instalado (é ele que conta o PR e os checks).
	Gh bool    `json:"gh"`
	PR *git.PR `json:"pr"`
	// O que não deu para saber, em uma frase.
	Aviso string `json:"aviso,omitempty"`
}

// situacao monta o retrato da branch da tarefa. Fora de um repositório git
// é erro; o resto que faltar vira aviso.
func situacao(ctx context.Context, pasta string) (situacaoGithub, error) {
	_, branch, err := git.Repositorio(ctx, pasta)
	if err != nil {
		if errors.Is(err, git.ErrNaoRepositorio) {
			return situacaoGithub{}, dados.ErrInvalido{Motivo: "A tarefa não está num repositório git."}
		}
		return situacaoGithub{}, erroDeCodigo(err)
	}
	st := situacaoGithub{Branch: branch, Gh: git.TemGh()}
	remoto, err := git.LerRemoto(ctx, pasta)
	if err != nil {
		st.Aviso = "O repositório não tem um remoto origin."
		return st, nil
	}
	st.Origin = true
	st.Repositorio = remoto.Caminho()
	base, commit, err := git.Base(ctx, pasta)
	if err == nil {
		st.Base = strings.TrimPrefix(base, "origin/")
	}
	if st.Base == branch {
		st.Aviso = "A tarefa está na branch " + branch + ", a mesma da base: não há PR a abrir."
		return st, nil
	}
	envio, err := git.LerEnvio(ctx, pasta, branch, commit)
	if err != nil {
		return st, erroDeCodigo(err)
	}
	st.Enviada, st.AEnviar = envio.Enviada, envio.AEnviar
	if !remoto.GitHub() {
		st.Aviso = "O origin não está no GitHub (" + remoto.Host + ")."
		return st, nil
	}
	st.Comparar = remoto.Comparar(st.Base, branch)
	st.Pulls = remoto.Pulls(branch)
	if st.Gh && st.Enviada {
		pr, err := git.PullRequest(ctx, pasta, branch)
		if err != nil {
			st.Aviso = err.Error()
		}
		st.PR = pr
	}
	return st, nil
}

// situacaoNoGithub devolve a branch, o envio e o PR da tarefa.
func (s *Servidor) situacaoNoGithub(w http.ResponseWriter, r *http.Request) {
	pasta, err := s.pastaDaTarefa(r)
	if err != nil {
		responderErro(w, err)
		return
	}
	st, err := situacao(r.Context(), pasta)
	if err != nil {
		responderErro(w, err)
		return
	}
	responderJSON(w, st)
}

// enviarAoGithub manda a branch da tarefa ao origin e devolve a situação nova.
func (s *Servidor) enviarAoGithub(w http.ResponseWriter, r *http.Request) {
	pasta, err := s.pastaDaTarefa(r)
	if err != nil {
		responderErro(w, err)
		return
	}
	st, err := situacao(r.Context(), pasta)
	if err != nil {
		responderErro(w, err)
		return
	}
	if !st.Origin || st.Base == st.Branch {
		responderErro(w, dados.ErrInvalido{Motivo: st.Aviso})
		return
	}
	if err := git.Enviar(r.Context(), pasta, st.Branch); err != nil {
		responderErro(w, dados.ErrInvalido{Motivo: "Não consegui enviar a branch: " + err.Error()})
		return
	}
	st, err = situacao(r.Context(), pasta)
	if err != nil {
		responderErro(w, err)
		return
	}
	responderJSON(w, st)
}
