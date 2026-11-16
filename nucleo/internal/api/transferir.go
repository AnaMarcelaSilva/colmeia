package api

import (
	"context"
	"fmt"
	"net/http"
	"slices"
	"strings"

	"github.com/AnaMarcelaSilva/colmeia/nucleo/internal/dados"
	"github.com/AnaMarcelaSilva/colmeia/nucleo/internal/git"
)

// Uma tarefa que foi parar no projeto errado (o agente criou onde quis, ou
// você mudou de ideia) muda de projeto sem perder nada: notas, lousa, anexos,
// pedidos e linha do tempo são dela. Só não vai com agentes rodando (eles
// trabalham na pasta do projeto de antes) nem com cópia isolada (a cópia é
// do repositório de antes).

func (s *Servidor) rotasTransferir(mux *http.ServeMux) {
	mux.HandleFunc("PUT /v1/tarefas/{id}/projeto", s.transferirTarefa)
}

// transferirTarefa (tela): leva a tarefa para o projeto pedido, do mesmo perfil.
func (s *Servidor) transferirTarefa(w http.ResponseWriter, r *http.Request) {
	id, err := idDaRota(r)
	if err != nil {
		responderErro(w, err)
		return
	}
	var pedido struct {
		Projeto int64 `json:"projeto"`
	}
	if err := ler(r, &pedido); err != nil {
		responderErro(w, err)
		return
	}
	tarefa, _, _, err := s.Banco.Tarefa(r.Context(), id)
	if err != nil {
		responderErro(w, err)
		return
	}
	t, err := s.transferir(r.Context(), tarefa, pedido.Projeto, dados.OrigemVoce)
	if err != nil {
		responderErro(w, err)
		return
	}
	responderJSON(w, t)
}

// transferir confere o que o banco não vê (agentes, a branch no destino) e
// grava a mudança.
func (s *Servidor) transferir(ctx context.Context, tarefa dados.Tarefa, destino int64, origem string) (dados.Tarefa, error) {
	agentes, err := s.Banco.ListarAgentes(ctx, tarefa.ID)
	if err != nil {
		return dados.Tarefa{}, err
	}
	if n := len(agentes); n == 1 {
		return dados.Tarefa{}, dados.ErrInvalido{Motivo: "a tarefa tem 1 agente, que trabalha na pasta do projeto de agora: remova o agente antes"}
	} else if n > 1 {
		return dados.Tarefa{}, dados.ErrInvalido{Motivo: fmt.Sprintf("a tarefa tem %d agentes, que trabalham na pasta do projeto de agora: remova os agentes antes", n)}
	}
	if tarefa.Local == "copia" {
		return dados.Tarefa{}, dados.ErrInvalido{Motivo: "uma tarefa com cópia isolada não muda de projeto"}
	}
	p, err := s.Banco.Projeto(ctx, destino)
	if err != nil {
		return dados.Tarefa{}, dados.ErrInvalido{Motivo: "o projeto de destino não existe"}
	}
	// A branch só segue se existir no destino; senão a tarefa fica sem branch,
	// como uma criada sem ela.
	branch := ""
	if p.Tipo != "pasta" && tarefa.Branch != "" {
		if lista, err := git.Branches(ctx, p.Caminho); err == nil && slices.Contains(lista, tarefa.Branch) {
			branch = tarefa.Branch
		}
	}
	return s.Banco.TransferirTarefa(ctx, tarefa.ID, p.ID, branch, origem)
}

// projetoPorNome acha um projeto do perfil pelo nome ("loja-web") ou por
// "workspace/nome", sem diferença de maiúsculas. Sem achar, o erro lista os
// nomes para o agente não chutar de novo.
func (s *Servidor) projetoPorNome(ctx context.Context, perfil int64, nome string) (dados.Projeto, error) {
	lista, err := s.Banco.ListarProjetos(ctx, perfil)
	if err != nil {
		return dados.Projeto{}, err
	}
	pedido := strings.ToLower(strings.TrimSpace(nome))
	var achados []dados.Projeto
	for _, p := range lista {
		if strings.ToLower(p.Nome) == pedido || strings.ToLower(p.Workspace+"/"+p.Nome) == pedido {
			achados = append(achados, p)
		}
	}
	switch len(achados) {
	case 1:
		return achados[0], nil
	case 0:
		nomes := make([]string, 0, len(lista))
		for _, p := range lista {
			nomes = append(nomes, p.Workspace+"/"+p.Nome)
		}
		return dados.Projeto{}, dados.ErrInvalido{Motivo: "Não há projeto chamado “" + strings.TrimSpace(nome) + "”. Os projetos do perfil são: " + strings.Join(nomes, ", ") + "."}
	default:
		return dados.Projeto{}, dados.ErrInvalido{Motivo: "Há mais de um projeto chamado “" + strings.TrimSpace(nome) + "”: diga workspace/nome."}
	}
}

// agenteListarProjetos: os projetos do perfil, para o agente criar ou
// transferir uma tarefa onde você pediu.
func (s *Servidor) agenteListarProjetos(w http.ResponseWriter, r *http.Request, q quemAgente) {
	lista, err := s.Banco.ListarProjetos(r.Context(), q.perfil)
	if err != nil {
		responderErro(w, err)
		return
	}
	projetos := []map[string]any{}
	for _, p := range lista {
		projetos = append(projetos, map[string]any{"workspace": p.Workspace, "nome": p.Nome, "pasta": p.Caminho, "git": p.Tipo != "pasta", "seu": p.ID == q.projeto.ID})
	}
	responderJSON(w, map[string]any{"projetos": projetos})
}

// agenteTransferirTarefa: a pedido da usuária, o agente leva uma tarefa do
// projeto dele (não a sua: ele roda nela) para outro projeto do perfil.
func (s *Servidor) agenteTransferirTarefa(w http.ResponseWriter, r *http.Request, q quemAgente) {
	var pedido struct {
		Tarefa  int64  `json:"tarefa"`
		Projeto string `json:"projeto"`
	}
	if err := ler(r, &pedido); err != nil {
		responderErro(w, err)
		return
	}
	if pedido.Tarefa == 0 || pedido.Tarefa == q.tarefa.ID {
		responderErro(w, dados.ErrInvalido{Motivo: "A sua tarefa não muda de projeto enquanto você roda nela: a usuária faz isso pela tela, no menu do cartão. Para outra tarefa do projeto, passe o id (de listar_tarefas)."})
		return
	}
	ctx := r.Context()
	tarefa, _, _, err := s.Banco.Tarefa(ctx, pedido.Tarefa)
	if err != nil || tarefa.ProjetoID != q.projeto.ID {
		responderErro(w, dados.ErrInvalido{Motivo: "Só dá para transferir tarefas do projeto em que você trabalha (veja listar_tarefas)."})
		return
	}
	p, err := s.projetoPorNome(ctx, q.perfil, pedido.Projeto)
	if err != nil {
		responderErro(w, err)
		return
	}
	t, err := s.transferir(ctx, tarefa, p.ID, dados.OrigemAgente)
	if err != nil {
		responderErro(w, err)
		return
	}
	responderJSON(w, map[string]any{"tarefa": t.ID, "titulo": t.Titulo, "projeto": p.Nome, "workspace": p.Workspace, "branch": t.Branch})
}
