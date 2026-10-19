package api

import (
	"context"
	"errors"
	"io"
	"net/http"

	"github.com/AnaMarcelaSilva/colmeia/nucleo/internal/dados"
	"github.com/AnaMarcelaSilva/colmeia/nucleo/internal/icone"
)

// Rotas da imagem do workspace e da base de conhecimento (a tela). As do
// agente ficam em rotasAgente.
func (s *Servidor) rotasConhecimento(mux *http.ServeMux) {
	mux.HandleFunc("GET /v1/workspaces/{id}/icone", s.lerIcone)
	mux.HandleFunc("PUT /v1/workspaces/{id}/icone", s.gravarIcone)
	mux.HandleFunc("DELETE /v1/workspaces/{id}/icone", s.tirarIcone)
	mux.HandleFunc("POST /v1/icone/previa", s.previaIcone)
	mux.HandleFunc("GET /v1/workspaces/{id}/conhecimento", s.listarConhecimento)
	mux.HandleFunc("POST /v1/workspaces/{id}/conhecimento", s.criarConhecimento)
	mux.HandleFunc("PATCH /v1/conhecimento/{id}", s.editarConhecimento)
	mux.HandleFunc("POST /v1/conhecimento/{id}/aprovar", s.aprovarConhecimento)
	mux.HandleFunc("DELETE /v1/conhecimento/{id}", s.removerConhecimento)
}

func (s *Servidor) lerIcone(w http.ResponseWriter, r *http.Request) {
	id, err := idDaRota(r)
	if err != nil {
		responderErro(w, err)
		return
	}
	png, err := s.Banco.IconeWorkspace(r.Context(), id)
	if err == nil && png == nil {
		err = dados.ErrNaoEncontrado
	}
	if err != nil {
		responderErro(w, err)
		return
	}
	w.Header().Set("Content-Type", "image/png")
	w.Header().Set("X-Content-Type-Options", "nosniff")
	w.Write(png)
}

// reduzirIcone lê a imagem do corpo e devolve o PNG pequeno, ou o erro em
// português.
func reduzirIcone(r *http.Request) ([]byte, error) {
	bruto, err := io.ReadAll(io.LimitReader(r.Body, icone.MaxBytes+1))
	if err != nil {
		return nil, err
	}
	png, err := icone.Reduzir(bruto)
	if errors.Is(err, icone.ErrFormato) || errors.Is(err, icone.ErrGrande) {
		err = dados.ErrInvalido{Motivo: err.Error()}
	}
	return png, err
}

// previaIcone mostra como a imagem fica antes de gravar: a tela só lê PNG,
// e o corte é o mesmo de gravarIcone.
func (s *Servidor) previaIcone(w http.ResponseWriter, r *http.Request) {
	png, err := reduzirIcone(r)
	if err != nil {
		responderErro(w, err)
		return
	}
	w.Header().Set("Content-Type", "image/png")
	w.Header().Set("X-Content-Type-Options", "nosniff")
	w.Write(png)
}

// gravarIcone recebe a imagem no corpo, como veio do disco: o núcleo confere
// o formato, corta o quadrado do meio e grava só o PNG pequeno.
func (s *Servidor) gravarIcone(w http.ResponseWriter, r *http.Request) {
	id, err := idDaRota(r)
	if err != nil {
		responderErro(w, err)
		return
	}
	png, err := reduzirIcone(r)
	if err != nil {
		responderErro(w, err)
		return
	}
	ws, err := s.Banco.GravarIconeWorkspace(r.Context(), id, png)
	if err != nil {
		responderErro(w, err)
		return
	}
	responderJSON(w, ws)
}

func (s *Servidor) tirarIcone(w http.ResponseWriter, r *http.Request) {
	id, err := idDaRota(r)
	if err != nil {
		responderErro(w, err)
		return
	}
	ws, err := s.Banco.GravarIconeWorkspace(r.Context(), id, nil)
	if err != nil {
		responderErro(w, err)
		return
	}
	responderJSON(w, ws)
}

func (s *Servidor) listarConhecimento(w http.ResponseWriter, r *http.Request) {
	id, err := idDaRota(r)
	if err != nil {
		responderErro(w, err)
		return
	}
	lista, err := s.Banco.ListarConhecimento(r.Context(), id)
	if err != nil {
		responderErro(w, err)
		return
	}
	responderJSON(w, lista)
}

// pedidoConhecimento: projeto_id 0 é uma nota do workspace inteiro.
type pedidoConhecimento struct {
	ProjetoID int64  `json:"projeto_id"`
	Titulo    string `json:"titulo"`
	Texto     string `json:"texto"`
}

const maxPedidoConhecimento = 64 << 10

func (s *Servidor) criarConhecimento(w http.ResponseWriter, r *http.Request) {
	id, err := idDaRota(r)
	if err != nil {
		responderErro(w, err)
		return
	}
	var p pedidoConhecimento
	if err := lerAte(r, &p, maxPedidoConhecimento); err != nil {
		responderErro(w, err)
		return
	}
	c, err := s.Banco.CriarConhecimento(r.Context(), dados.NovoConhecimento{
		Workspace: id, Projeto: p.ProjetoID, Titulo: p.Titulo, Texto: p.Texto, Origem: dados.OrigemVoce})
	if err != nil {
		responderErro(w, err)
		return
	}
	responderJSON(w, c)
}

func (s *Servidor) editarConhecimento(w http.ResponseWriter, r *http.Request) {
	id, err := idDaRota(r)
	if err != nil {
		responderErro(w, err)
		return
	}
	var p struct {
		Titulo string `json:"titulo"`
		Texto  string `json:"texto"`
	}
	if err := lerAte(r, &p, maxPedidoConhecimento); err != nil {
		responderErro(w, err)
		return
	}
	c, err := s.Banco.EditarConhecimento(r.Context(), id, p.Titulo, p.Texto)
	if err != nil {
		responderErro(w, err)
		return
	}
	responderJSON(w, c)
}

func (s *Servidor) aprovarConhecimento(w http.ResponseWriter, r *http.Request) {
	id, err := idDaRota(r)
	if err != nil {
		responderErro(w, err)
		return
	}
	c, err := s.Banco.AprovarConhecimento(r.Context(), id)
	if err != nil {
		responderErro(w, err)
		return
	}
	responderJSON(w, c)
}

func (s *Servidor) removerConhecimento(w http.ResponseWriter, r *http.Request) {
	id, err := idDaRota(r)
	if err != nil {
		responderErro(w, err)
		return
	}
	if err := s.Banco.RemoverConhecimento(r.Context(), id); err != nil {
		responderErro(w, err)
		return
	}
	responderJSON(w, map[string]any{"ok": true})
}

// agenteLerConhecimento devolve as notas aprovadas do projeto e do workspace
// da tarefa do agente, inteiras.
func (s *Servidor) agenteLerConhecimento(w http.ResponseWriter, r *http.Request, q quemAgente) {
	notas, err := s.Banco.ConhecimentoDoProjeto(r.Context(), q.projeto.ID)
	if err != nil {
		responderErro(w, err)
		return
	}
	lista := make([]map[string]any, 0, len(notas))
	for _, n := range notas {
		onde := "workspace"
		if n.ProjetoID != 0 {
			onde = "projeto"
		}
		lista = append(lista, map[string]any{"id": n.ID, "onde": onde, "titulo": n.Titulo, "texto": n.Texto})
	}
	responderJSON(w, map[string]any{"workspace": q.projeto.Workspace, "projeto": q.projeto.Nome, "notas": lista})
}

// agenteAnotarConhecimento grava a nota do agente como sugestão (ou aprovada,
// com a aprovação automática do workspace ligada).
func (s *Servidor) agenteAnotarConhecimento(w http.ResponseWriter, r *http.Request, q quemAgente) {
	var p struct {
		Titulo    string `json:"titulo"`
		Texto     string `json:"texto"`
		Onde      string `json:"onde"`
		Substitui int64  `json:"substitui"`
	}
	if err := lerAte(r, &p, maxPedidoConhecimento); err != nil {
		responderErro(w, err)
		return
	}
	novo := dados.NovoConhecimento{Workspace: q.projeto.WorkspaceID, Titulo: p.Titulo, Texto: p.Texto,
		Origem: dados.OrigemAgente, Agente: q.ctx.ID, Substitui: p.Substitui}
	switch p.Onde {
	case "", "projeto":
		novo.Projeto = q.projeto.ID
	case "workspace":
	default:
		responderErro(w, dados.ErrInvalido{Motivo: "onde precisa ser projeto ou workspace"})
		return
	}
	c, err := s.Banco.CriarConhecimento(r.Context(), novo)
	if err != nil {
		responderErro(w, err)
		return
	}
	responderJSON(w, map[string]any{"id": c.ID, "estado": c.Estado})
}

// textoDoConhecimento é o que o agente recebe ao começar ("" sem notas).
func (s *Servidor) textoDoConhecimento(ctx context.Context, projeto dados.Projeto) (string, error) {
	notas, err := s.Banco.ConhecimentoDoProjeto(ctx, projeto.ID)
	if err != nil {
		return "", err
	}
	return dados.TextoDoConhecimento(notas, projeto.Workspace, projeto.Nome), nil
}
