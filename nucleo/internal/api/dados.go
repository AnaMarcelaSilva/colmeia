package api

import (
	"cmp"
	"encoding/json"
	"errors"
	"net/http"
	"path/filepath"
	"strconv"

	"github.com/AnaMarcelaSilva/colmeia/nucleo/internal/dados"
	"github.com/AnaMarcelaSilva/colmeia/nucleo/internal/ferramentas"
	"github.com/AnaMarcelaSilva/colmeia/nucleo/internal/git"
)

// Maior corpo aceito num pedido de dados.
const limiteCorpo = 64 << 10

func (s *Servidor) rotasDados(mux *http.ServeMux) {
	mux.HandleFunc("GET /v1/ferramentas", s.listarFerramentas)
	mux.HandleFunc("GET /v1/perfis", s.listarPerfis)
	mux.HandleFunc("POST /v1/perfis", s.criarPerfil)
	mux.HandleFunc("PATCH /v1/perfis/{id}", s.atualizarPerfil)
	mux.HandleFunc("GET /v1/perfis/{id}/quadro", s.quadro)
	mux.HandleFunc("GET /v1/perfis/{id}/eventos", s.eventos)
	mux.HandleFunc("GET /v1/perfis/{id}/contas", s.listarContas)
	mux.HandleFunc("PUT /v1/perfis/{id}/contas", s.definirContas)
	mux.HandleFunc("GET /v1/perfis/{id}/workspaces", s.listarWorkspaces)
	mux.HandleFunc("POST /v1/perfis/{id}/workspaces", s.criarWorkspace)
	mux.HandleFunc("GET /v1/perfis/{id}/projetos", s.listarProjetos)
	mux.HandleFunc("PATCH /v1/workspaces/{id}", s.atualizarWorkspace)
	mux.HandleFunc("POST /v1/workspaces/{id}/projetos", s.criarProjeto)
	mux.HandleFunc("DELETE /v1/projetos/{id}", s.removerProjeto)
	mux.HandleFunc("GET /v1/projetos/{id}/branches", s.listarBranches)
	mux.HandleFunc("GET /v1/projetos/{id}/tarefas", s.listarTarefas)
	mux.HandleFunc("POST /v1/projetos/{id}/tarefas", s.criarTarefa)
	mux.HandleFunc("PATCH /v1/tarefas/{id}", s.atualizarTarefa)
	mux.HandleFunc("DELETE /v1/tarefas/{id}", s.removerTarefa)
	s.rotasAgentes(mux)
	s.rotasComandos(mux)
	s.rotasAnexos(mux)
	s.rotasLinha(mux)
	s.rotasSprints(mux)
	s.rotasMensagens(mux)
	s.rotasPedidos(mux)
	s.rotasNavegador(mux)
	s.rotasArquivos(mux)
	s.rotasAgente(mux)
	s.rotasLousas(mux)
	s.rotasBancos(mux)
	s.rotasBancosDoAgente(mux)
}

// responderErro traduz os erros dos dados em status HTTP com uma mensagem clara.
func responderErro(w http.ResponseWriter, err error) {
	status := http.StatusInternalServerError
	mensagem := "erro interno"
	var invalido dados.ErrInvalido
	switch {
	case errors.As(err, &invalido):
		status, mensagem = http.StatusBadRequest, invalido.Motivo
	case errors.Is(err, dados.ErrNaoEncontrado):
		status, mensagem = http.StatusNotFound, "não encontrado"
	case errors.Is(err, dados.ErrJaExiste):
		status, mensagem = http.StatusConflict, "já existe um com esse nome"
	case errors.Is(err, git.ErrMudancas):
		status, mensagem = http.StatusConflict, "A cópia isolada tem mudanças que não estão em nenhum commit. Salve num commit ou descarte antes de remover."
	}
	w.Header().Set("Content-Type", "application/json")
	w.WriteHeader(status)
	json.NewEncoder(w).Encode(map[string]string{"erro": mensagem})
}

func ler(r *http.Request, destino any) error {
	return lerAte(r, destino, limiteCorpo)
}

func idDaRota(r *http.Request) (int64, error) {
	id, err := strconv.ParseInt(r.PathValue("id"), 10, 64)
	if err != nil || id <= 0 {
		return 0, dados.ErrNaoEncontrado
	}
	return id, nil
}

func (s *Servidor) listarFerramentas(w http.ResponseWriter, r *http.Request) {
	responderJSON(w, ferramentas.Detectar(r.Context()))
}

func (s *Servidor) listarPerfis(w http.ResponseWriter, r *http.Request) {
	perfis, err := s.Banco.ListarPerfis(r.Context())
	if err != nil {
		responderErro(w, err)
		return
	}
	responderJSON(w, perfis)
}

func (s *Servidor) criarPerfil(w http.ResponseWriter, r *http.Request) {
	var pedido struct{ Nome, Tema string }
	if err := ler(r, &pedido); err != nil {
		responderErro(w, err)
		return
	}
	perfil, err := s.Banco.CriarPerfil(r.Context(), pedido.Nome, pedido.Tema)
	if err != nil {
		responderErro(w, err)
		return
	}
	responderJSON(w, perfil)
}

func (s *Servidor) atualizarPerfil(w http.ResponseWriter, r *http.Request) {
	id, err := idDaRota(r)
	if err != nil {
		responderErro(w, err)
		return
	}
	// Só os campos enviados mudam.
	var pedido struct {
		Tema         *string `json:"tema"`
		AvisoCaptura *bool   `json:"aviso_captura"`
		TempoAgentes *bool   `json:"tempo_agentes"`
	}
	if err := ler(r, &pedido); err != nil {
		responderErro(w, err)
		return
	}
	if pedido.Tema != nil {
		if err := s.Banco.DefinirTema(r.Context(), id, *pedido.Tema); err != nil {
			responderErro(w, err)
			return
		}
	}
	if pedido.AvisoCaptura != nil {
		if err := s.Banco.DefinirAvisoCaptura(r.Context(), id, *pedido.AvisoCaptura); err != nil {
			responderErro(w, err)
			return
		}
	}
	if pedido.TempoAgentes != nil {
		if err := s.Banco.DefinirTempoAgentes(r.Context(), id, *pedido.TempoAgentes); err != nil {
			responderErro(w, err)
			return
		}
	}
	responderJSON(w, map[string]any{"ok": true})
}

func (s *Servidor) atualizarWorkspace(w http.ResponseWriter, r *http.Request) {
	id, err := idDaRota(r)
	if err != nil {
		responderErro(w, err)
		return
	}
	var pedido struct {
		Recolhido *bool `json:"recolhido"`
	}
	if err := ler(r, &pedido); err != nil {
		responderErro(w, err)
		return
	}
	if pedido.Recolhido == nil {
		responderErro(w, dados.ErrInvalido{Motivo: "Diga se o workspace fica recolhido."})
		return
	}
	if err := s.Banco.RecolherWorkspace(r.Context(), id, *pedido.Recolhido); err != nil {
		responderErro(w, err)
		return
	}
	responderJSON(w, map[string]any{"ok": true})
}

// contaComPasta mostra à tela onde fica a conta separada de cada ferramenta.
type contaComPasta struct {
	dados.Conta
	Pasta    string `json:"pasta,omitempty"`
	Variavel string `json:"variavel,omitempty"`
}

func (s *Servidor) listarContas(w http.ResponseWriter, r *http.Request) {
	id, err := idDaRota(r)
	if err != nil {
		responderErro(w, err)
		return
	}
	contas, err := s.Banco.ListarContas(r.Context(), id)
	if err != nil {
		responderErro(w, err)
		return
	}
	resposta := make([]contaComPasta, 0, len(contas))
	for _, c := range contas {
		item := contaComPasta{Conta: c, Variavel: ferramentas.Variavel(c.Ferramenta)}
		if c.Modo == "separada" {
			item.Pasta = ferramentas.PastaDaConta(s.DirDados, id, c.Ferramenta)
		}
		resposta = append(resposta, item)
	}
	responderJSON(w, resposta)
}

func (s *Servidor) definirContas(w http.ResponseWriter, r *http.Request) {
	id, err := idDaRota(r)
	if err != nil {
		responderErro(w, err)
		return
	}
	var contas []dados.Conta
	if err := ler(r, &contas); err != nil {
		responderErro(w, err)
		return
	}
	for _, c := range contas {
		if c.Modo == "separada" && ferramentas.Variavel(c.Ferramenta) == "" {
			responderErro(w, dados.ErrInvalido{Motivo: c.Ferramenta + " ainda não permite uma conta separada por perfil"})
			return
		}
	}
	if err := s.Banco.DefinirContas(r.Context(), id, contas); err != nil {
		responderErro(w, err)
		return
	}
	responderJSON(w, map[string]any{"ok": true})
}

func (s *Servidor) listarWorkspaces(w http.ResponseWriter, r *http.Request) {
	id, err := idDaRota(r)
	if err != nil {
		responderErro(w, err)
		return
	}
	lista, err := s.Banco.ListarWorkspaces(r.Context(), id)
	if err != nil {
		responderErro(w, err)
		return
	}
	responderJSON(w, lista)
}

func (s *Servidor) criarWorkspace(w http.ResponseWriter, r *http.Request) {
	id, err := idDaRota(r)
	if err != nil {
		responderErro(w, err)
		return
	}
	var pedido struct{ Nome string }
	if err := ler(r, &pedido); err != nil {
		responderErro(w, err)
		return
	}
	ws, err := s.Banco.CriarWorkspace(r.Context(), id, pedido.Nome)
	if err != nil {
		responderErro(w, err)
		return
	}
	responderJSON(w, ws)
}

func (s *Servidor) listarProjetos(w http.ResponseWriter, r *http.Request) {
	id, err := idDaRota(r)
	if err != nil {
		responderErro(w, err)
		return
	}
	lista, err := s.Banco.ListarProjetos(r.Context(), id)
	if err != nil {
		responderErro(w, err)
		return
	}
	responderJSON(w, lista)
}

// criarProjeto confere a pasta antes de gravar. Um repositório git entra pela
// raiz, com a branch atual como padrão; uma pasta sem git entra como pasta de
// trabalho, sem branches.
func (s *Servidor) criarProjeto(w http.ResponseWriter, r *http.Request) {
	id, err := idDaRota(r)
	if err != nil {
		responderErro(w, err)
		return
	}
	var pedido struct{ Nome, Caminho string }
	if err := ler(r, &pedido); err != nil {
		responderErro(w, err)
		return
	}
	tipo := "git"
	raiz, branch, err := git.Repositorio(r.Context(), pedido.Caminho)
	if errors.Is(err, git.ErrNaoRepositorio) {
		tipo, raiz, branch, err = "pasta", filepath.Clean(pedido.Caminho), "", nil
	}
	if err != nil {
		responderErro(w, dados.ErrInvalido{Motivo: err.Error()})
		return
	}
	projeto, err := s.Banco.CriarProjeto(r.Context(), id, pedido.Nome, raiz, tipo, branch)
	if err != nil {
		responderErro(w, err)
		return
	}
	responderJSON(w, projeto)
}

func (s *Servidor) removerProjeto(w http.ResponseWriter, r *http.Request) {
	id, err := idDaRota(r)
	if err != nil {
		responderErro(w, err)
		return
	}
	// Antes de esquecer o projeto: para os agentes e tira as cópias isoladas.
	// Uma cópia com mudanças sem commit impede a remoção.
	tarefas, err := s.Banco.ListarTarefas(r.Context(), id)
	if err != nil {
		responderErro(w, err)
		return
	}
	projeto, err := s.Banco.Projeto(r.Context(), id)
	if err != nil {
		responderErro(w, err)
		return
	}
	// As execuções do Play do projeto param junto.
	if comandos, err := s.Banco.ListarComandos(r.Context(), id); err == nil {
		for _, c := range comandos {
			s.Execucoes.Fechar(c.ID)
		}
	}
	for _, t := range tarefas {
		if err := s.fecharAgentesDaTarefa(r.Context(), t.ID); err != nil {
			responderErro(w, err)
			return
		}
		if err := s.removerCopia(r.Context(), t, projeto); err != nil {
			responderErro(w, err)
			return
		}
	}
	if err := s.Banco.RemoverProjeto(r.Context(), id); err != nil {
		responderErro(w, err)
		return
	}
	responderJSON(w, map[string]any{"ok": true})
}

func (s *Servidor) listarBranches(w http.ResponseWriter, r *http.Request) {
	id, err := idDaRota(r)
	if err != nil {
		responderErro(w, err)
		return
	}
	projeto, err := s.Banco.Projeto(r.Context(), id)
	if err != nil {
		responderErro(w, err)
		return
	}
	if projeto.Tipo == "pasta" {
		responderJSON(w, []string{})
		return
	}
	branches, err := git.Branches(r.Context(), projeto.Caminho)
	if err != nil {
		responderErro(w, dados.ErrInvalido{Motivo: "não consegui ler as branches: " + err.Error()})
		return
	}
	responderJSON(w, branches)
}

func (s *Servidor) listarTarefas(w http.ResponseWriter, r *http.Request) {
	id, err := idDaRota(r)
	if err != nil {
		responderErro(w, err)
		return
	}
	lista, err := s.Banco.ListarTarefas(r.Context(), id)
	if err != nil {
		responderErro(w, err)
		return
	}
	responderJSON(w, lista)
}

func (s *Servidor) criarTarefa(w http.ResponseWriter, r *http.Request) {
	id, err := idDaRota(r)
	if err != nil {
		responderErro(w, err)
		return
	}
	// Local "copia" tira uma cópia isolada na branch da tarefa: nova (Nova,
	// a partir de Base) ou uma que já existe.
	var pedido struct {
		Titulo, Branch, Local, Base string
		Nova                        bool
	}
	if err := ler(r, &pedido); err != nil {
		responderErro(w, err)
		return
	}
	tarefa, err := s.Banco.CriarTarefa(r.Context(), id, pedido.Titulo, pedido.Branch, pedido.Local)
	if err != nil {
		responderErro(w, err)
		return
	}
	if tarefa.Local == "copia" {
		projeto, err := s.Banco.Projeto(r.Context(), id)
		if err == nil {
			tarefa, err = s.criarCopia(r.Context(), tarefa, projeto, pedido.Base, pedido.Nova)
		}
		if err != nil {
			responderErro(w, err)
			return
		}
	}
	responderJSON(w, tarefa)
}

func (s *Servidor) atualizarTarefa(w http.ResponseWriter, r *http.Request) {
	id, err := idDaRota(r)
	if err != nil {
		responderErro(w, err)
		return
	}
	var mudanca dados.Mudanca
	if err := ler(r, &mudanca); err != nil {
		responderErro(w, err)
		return
	}
	tarefa, err := s.Banco.AtualizarTarefa(r.Context(), id, mudanca, dados.OrigemVoce)
	if err != nil {
		responderErro(w, err)
		return
	}
	responderJSON(w, tarefa)
}

func (s *Servidor) removerTarefa(w http.ResponseWriter, r *http.Request) {
	id, err := idDaRota(r)
	if err != nil {
		responderErro(w, err)
		return
	}
	tarefa, projeto, _, err := s.Banco.Tarefa(r.Context(), id)
	if err != nil {
		responderErro(w, err)
		return
	}
	// Uma cópia com mudanças sem commit impede a remoção, e aí os agentes
	// continuam rodando. Senão eles param antes, para ninguém escrever na cópia
	// enquanto ela sai.
	if tarefa.Local == "copia" {
		if mudou, err := git.TemMudancas(r.Context(), tarefa.Copia); err != nil || mudou {
			responderErro(w, cmp.Or(err, git.ErrMudancas))
			return
		}
	}
	if err := s.fecharAgentesDaTarefa(r.Context(), id); err != nil {
		responderErro(w, err)
		return
	}
	if err := s.removerCopia(r.Context(), tarefa, projeto); err != nil {
		responderErro(w, err)
		return
	}
	if err := s.Banco.RemoverTarefa(r.Context(), id); err != nil {
		responderErro(w, err)
		return
	}
	responderJSON(w, map[string]any{"ok": true})
}
