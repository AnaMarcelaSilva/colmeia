package api

import (
	"context"
	"net/http"
	"os"
	"path/filepath"
	"runtime"
	"strings"
	"sync"
	"time"

	"github.com/AnaMarcelaSilva/colmeia/nucleo/internal/dados"
	"github.com/AnaMarcelaSilva/colmeia/nucleo/internal/ferramentas"
	"github.com/AnaMarcelaSilva/colmeia/nucleo/internal/rodar"
	"github.com/AnaMarcelaSilva/colmeia/nucleo/internal/terminal"
)

// O "Play" dos projetos: configurações de execução (um comando do shell numa
// subpasta, com variáveis de ambiente) que rodam num terminal do núcleo. A
// tela cria, edita, roda e para; um agente só propõe (pelas rotas /v1/agente),
// e a proposta não roda até você aceitar. Cada configuração roda uma vez por
// vez: rodar de novo para a anterior, como no IntelliJ.

func (s *Servidor) rotasComandos(mux *http.ServeMux) {
	mux.HandleFunc("GET /v1/projetos/{id}/comandos", s.listarComandos)
	mux.HandleFunc("POST /v1/projetos/{id}/comandos", s.criarComando)
	mux.HandleFunc("GET /v1/projetos/{id}/comandos/sugestoes", s.sugestoesDeComandos)
	mux.HandleFunc("GET /v1/workspaces/{id}/comandos", s.listarComandosDoWorkspace)
	mux.HandleFunc("PATCH /v1/comandos/{id}", s.editarComando)
	mux.HandleFunc("DELETE /v1/comandos/{id}", s.removerComando)
	mux.HandleFunc("POST /v1/comandos/{id}/aceitar", s.aceitarComando)
	mux.HandleFunc("POST /v1/comandos/{id}/rodar", s.rodarComando)
	mux.HandleFunc("POST /v1/comandos/{id}/parar", s.pararComando)
	mux.HandleFunc("GET /v1/comandos/{id}/terminal", s.terminalDoComando)
}

// fimExecucao é como a última execução de uma configuração terminou.
type fimExecucao struct {
	Codigo int    `json:"codigo"`
	Parada bool   `json:"parada"` // parada pela Colmeia (Parar, rodar de novo, desligar)
	Hora   string `json:"hora"`
}

// estadoExecucao é o que a tela mostra de uma configuração: rodando agora
// (desde quando, em que pasta) ou como terminou a última vez.
type estadoExecucao struct {
	Rodando bool         `json:"rodando"`
	Desde   string       `json:"desde,omitempty"`
	Onde    string       `json:"onde,omitempty"`
	Tarefa  int64        `json:"tarefa_id,omitempty"`
	Fim     *fimExecucao `json:"fim,omitempty"`
}

type comandoNaTela struct {
	dados.Comando
	estadoExecucao
}

// execucoes guarda o estado das execuções desde que o núcleo abriu.
type execucoes struct {
	mu     sync.Mutex
	estado map[int64]estadoExecucao
}

var registroExecucoes = execucoes{estado: map[int64]estadoExecucao{}}

func (e *execucoes) pegar(id int64) estadoExecucao {
	e.mu.Lock()
	defer e.mu.Unlock()
	return e.estado[id]
}

func (e *execucoes) guardar(id int64, v estadoExecucao) {
	e.mu.Lock()
	defer e.mu.Unlock()
	e.estado[id] = v
}

func (s *Servidor) naTela(c dados.Comando) comandoNaTela {
	estado := registroExecucoes.pegar(c.ID)
	estado.Rodando = s.Execucoes.Ativa(c.ID)
	return comandoNaTela{Comando: c, estadoExecucao: estado}
}

func (s *Servidor) listarComandos(w http.ResponseWriter, r *http.Request) {
	id, err := idDaRota(r)
	if err != nil {
		responderErro(w, err)
		return
	}
	if _, err := s.Banco.Projeto(r.Context(), id); err != nil {
		responderErro(w, err)
		return
	}
	lista, err := s.Banco.ListarComandos(r.Context(), id)
	if err != nil {
		responderErro(w, err)
		return
	}
	resposta := make([]comandoNaTela, 0, len(lista))
	for _, c := range lista {
		resposta = append(resposta, s.naTela(c))
	}
	responderJSON(w, resposta)
}

// listarComandosDoWorkspace traz as configurações de todos os projetos do
// workspace (com projeto_nome), para rodar as de um projeto a partir de outro.
func (s *Servidor) listarComandosDoWorkspace(w http.ResponseWriter, r *http.Request) {
	id, err := idDaRota(r)
	if err != nil {
		responderErro(w, err)
		return
	}
	if _, err := s.Banco.Workspace(r.Context(), id); err != nil {
		responderErro(w, err)
		return
	}
	lista, err := s.Banco.ListarComandosDoWorkspace(r.Context(), id)
	if err != nil {
		responderErro(w, err)
		return
	}
	resposta := make([]comandoNaTela, 0, len(lista))
	for _, c := range lista {
		resposta = append(resposta, s.naTela(c))
	}
	responderJSON(w, resposta)
}

// aceitarComando faz valer a proposta de um agente; a tela fica sabendo
// pelo evento comando.aceito.
func (s *Servidor) aceitarComando(w http.ResponseWriter, r *http.Request) {
	id, err := idDaRota(r)
	if err != nil {
		responderErro(w, err)
		return
	}
	c, err := s.Banco.AceitarComando(r.Context(), id)
	if err != nil {
		responderErro(w, err)
		return
	}
	responderJSON(w, s.naTela(c))
}

type pedidoComando struct {
	dados.CamposComando
	Origem string `json:"origem"`
}

// pedidoPlayDoAgente é o que o agente manda em POST /v1/agente/comandos.
type pedidoPlayDoAgente struct {
	dados.CamposComando
	// O projeto do workspace em que o Play roda; vazio é o da tarefa.
	Projeto string `json:"projeto"`
}

// projetoDoWorkspace acha, entre os projetos do workspace do agente, o de
// nome dado (sem diferenciar maiúsculas); vazio é o da própria tarefa.
func (s *Servidor) projetoDoWorkspace(ctx context.Context, q quemAgente, nome string) (dados.Projeto, error) {
	nome = strings.TrimSpace(nome)
	if nome == "" || strings.EqualFold(nome, q.projeto.Nome) {
		return q.projeto, nil
	}
	projetos, err := s.Banco.ListarProjetos(ctx, q.perfil)
	if err != nil {
		return dados.Projeto{}, err
	}
	nomes := make([]string, 0, len(projetos))
	for _, p := range projetos {
		if p.WorkspaceID != q.projeto.WorkspaceID {
			continue
		}
		if strings.EqualFold(p.Nome, nome) {
			return p, nil
		}
		nomes = append(nomes, p.Nome)
	}
	return dados.Projeto{}, dados.ErrInvalido{Motivo: "não há projeto " + nome + " no workspace; os projetos são: " + strings.Join(nomes, ", ")}
}

// agenteListarPlays: GET /v1/agente/comandos lista as configurações de
// execução do workspace (as aceitas e as propostas), sem os valores das
// variáveis, que podem ser segredos do usuário.
func (s *Servidor) agenteListarPlays(w http.ResponseWriter, r *http.Request, q quemAgente) {
	lista, err := s.Banco.ListarComandosDoWorkspace(r.Context(), q.projeto.WorkspaceID)
	if err != nil {
		responderErro(w, err)
		return
	}
	plays := make([]map[string]any, 0, len(lista))
	for _, c := range lista {
		variaveis := make([]string, 0, len(c.Ambiente))
		for _, v := range c.Ambiente {
			variaveis = append(variaveis, v.Nome)
		}
		plays = append(plays, map[string]any{"id": c.ID, "projeto": c.ProjetoNome, "nome": c.Nome, "comando": c.Comando, "pasta": c.Pasta,
			"variaveis": variaveis, "proposta": c.Proposta, "rodando": s.Execucoes.Ativa(c.ID)})
	}
	responderJSON(w, map[string]any{"projeto": q.projeto.Nome, "workspace": q.projeto.Workspace, "comandos": plays})
}

// agenteProporPlay: POST /v1/agente/comandos grava uma configuração de
// execução proposta pelo agente, no projeto da tarefa ou em outro do mesmo
// workspace. Ela não roda até o usuário aceitar na Colmeia.
func (s *Servidor) agenteProporPlay(w http.ResponseWriter, r *http.Request, q quemAgente) {
	var pedido pedidoPlayDoAgente
	if err := lerAte(r, &pedido, 64<<10); err != nil {
		responderErro(w, err)
		return
	}
	projeto, err := s.projetoDoWorkspace(r.Context(), q, pedido.Projeto)
	if err != nil {
		responderErro(w, err)
		return
	}
	autor := dados.AutorProposta{Agente: q.ctx.ID, Ferramenta: q.ctx.Ferramenta, Papel: q.ctx.Papel,
		Quem: ferramentas.Nome(q.ctx.Ferramenta) + " (" + q.ctx.Papel + ")", Tarefa: q.tarefa.ID, Titulo: q.tarefa.Titulo}
	c, err := s.Banco.ProporComando(r.Context(), projeto.ID, pedido.CamposComando, autor)
	if err != nil {
		responderErro(w, err)
		return
	}
	responderJSON(w, map[string]any{"id": c.ID, "projeto": projeto.Nome, "nome": c.Nome, "proposta": true})
}

func (s *Servidor) criarComando(w http.ResponseWriter, r *http.Request) {
	id, err := idDaRota(r)
	if err != nil {
		responderErro(w, err)
		return
	}
	var pedido pedidoComando
	if err := ler(r, &pedido); err != nil {
		responderErro(w, err)
		return
	}
	c, err := s.Banco.CriarComando(r.Context(), id, pedido.CamposComando, cmpOr(pedido.Origem, "voce"))
	if err != nil {
		responderErro(w, err)
		return
	}
	responderJSON(w, s.naTela(c))
}

// sugestoesDeComandos lê as configurações do IntelliJ e o que os arquivos do
// projeto sugerem, sem as que já estão gravadas (pelo nome).
func (s *Servidor) sugestoesDeComandos(w http.ResponseWriter, r *http.Request) {
	id, err := idDaRota(r)
	if err != nil {
		responderErro(w, err)
		return
	}
	p, err := s.Banco.Projeto(r.Context(), id)
	if err != nil {
		responderErro(w, err)
		return
	}
	gravados, err := s.Banco.ListarComandos(r.Context(), id)
	if err != nil {
		responderErro(w, err)
		return
	}
	tem := map[string]bool{}
	for _, c := range gravados {
		tem[c.Nome] = true
	}
	achados := rodar.Procurar(p.Caminho)
	novas := make([]rodar.Sugestao, 0, len(achados.Sugestoes))
	for _, sug := range achados.Sugestoes {
		if !tem[sug.Nome] {
			novas = append(novas, sug)
		}
	}
	achados.Sugestoes = novas
	responderJSON(w, achados)
}

func (s *Servidor) editarComando(w http.ResponseWriter, r *http.Request) {
	id, err := idDaRota(r)
	if err != nil {
		responderErro(w, err)
		return
	}
	var campos dados.CamposComando
	if err := ler(r, &campos); err != nil {
		responderErro(w, err)
		return
	}
	c, err := s.Banco.EditarComando(r.Context(), id, campos)
	if err != nil {
		responderErro(w, err)
		return
	}
	responderJSON(w, s.naTela(c))
}

func (s *Servidor) removerComando(w http.ResponseWriter, r *http.Request) {
	id, err := idDaRota(r)
	if err != nil {
		responderErro(w, err)
		return
	}
	c, perfil, err := s.Banco.ComandoPorID(r.Context(), id)
	if err != nil {
		responderErro(w, err)
		return
	}
	s.Execucoes.Fechar(id)
	if err := s.Banco.RemoverComando(r.Context(), id); err != nil {
		responderErro(w, err)
		return
	}
	s.Avisos.Publicar(perfil, map[string]any{"tipo": "comando.removido", "comando_id": id, "projeto_id": c.ProjetoID})
	w.WriteHeader(http.StatusNoContent)
}

type pedidoRodar struct {
	// Tarefa: roda na pasta da tarefa (a cópia isolada dela, se tiver). Zero
	// é a pasta do projeto.
	Tarefa int64  `json:"tarefa_id"`
	Cols   uint16 `json:"cols"`
	Rows   uint16 `json:"rows"`
}

// pastaDaExecucao é a subpasta da configuração dentro da pasta do projeto
// (ou da tarefa). Ela precisa existir e, seguindo os links, continuar
// dentro da pasta de cima.
func pastaDaExecucao(base, sub string) (string, error) {
	sub, err := dados.PastaRelativa(sub)
	if err != nil {
		return "", err
	}
	raiz, err := filepath.EvalSymlinks(base)
	if err != nil {
		return "", dados.ErrInvalido{Motivo: "a pasta não existe mais: " + base}
	}
	pasta, err := filepath.EvalSymlinks(filepath.Join(raiz, filepath.FromSlash(sub)))
	if err != nil {
		return "", dados.ErrInvalido{Motivo: "a pasta da configuração não existe: " + filepath.Join(base, filepath.FromSlash(sub))}
	}
	if rel, err := filepath.Rel(raiz, pasta); err != nil || rel == ".." || strings.HasPrefix(rel, ".."+string(filepath.Separator)) {
		return "", dados.ErrInvalido{Motivo: "a pasta da configuração sai do projeto"}
	}
	if info, err := os.Stat(pasta); err != nil || !info.IsDir() {
		return "", dados.ErrInvalido{Motivo: "a pasta da configuração não é uma pasta: " + pasta}
	}
	return pasta, nil
}

// linhaDeComando roda o texto pelo shell da pessoa, como num terminal: com
// -l, o PATH do login (go, node, flutter instalados no perfil) vale aqui
// mesmo com a Colmeia aberta pelo menu de apps.
func linhaDeComando(texto string) []string {
	if runtime.GOOS == "windows" {
		shell := terminal.ShellPadrao()
		if strings.HasSuffix(strings.ToLower(shell), "cmd.exe") {
			return []string{shell, "/C", texto}
		}
		return []string{shell, "-NoLogo", "-NoProfile", "-Command", texto}
	}
	return []string{terminal.ShellPadrao(), "-lc", texto}
}

func (s *Servidor) rodarComando(w http.ResponseWriter, r *http.Request) {
	id, err := idDaRota(r)
	if err != nil {
		responderErro(w, err)
		return
	}
	// O corpo é opcional: a tarefa e o tamanho do terminal.
	var pedido pedidoRodar
	if r.ContentLength != 0 {
		if err := ler(r, &pedido); err != nil {
			responderErro(w, err)
			return
		}
	}
	resposta, err := s.rodarConfiguracao(r.Context(), id, pedido)
	if err != nil {
		responderErro(w, err)
		return
	}
	responderJSON(w, resposta)
}

// rodarConfiguracao roda uma configuração do Play (pela tela ou por um
// comando falado). Rodar de novo para a execução anterior.
func (s *Servidor) rodarConfiguracao(ctx context.Context, id int64, pedido pedidoRodar) (comandoNaTela, error) {
	c, perfil, err := s.Banco.ComandoPorID(ctx, id)
	if err != nil {
		return comandoNaTela{}, err
	}
	if c.Proposta {
		return comandoNaTela{}, dados.ErrInvalido{Motivo: "aceite a proposta de " + c.PropostaPor + " antes de rodar"}
	}
	p, err := s.Banco.Projeto(ctx, c.ProjetoID)
	if err != nil {
		return comandoNaTela{}, err
	}
	base := p.Caminho
	if pedido.Tarefa != 0 {
		t, projeto, _, err := s.Banco.Tarefa(ctx, pedido.Tarefa)
		if err != nil {
			return comandoNaTela{}, err
		}
		if t.ProjetoID != p.ID {
			return comandoNaTela{}, dados.ErrInvalido{Motivo: "a tarefa é de outro projeto"}
		}
		base = t.Pasta(projeto)
	}
	pasta, err := pastaDaExecucao(base, c.Pasta)
	if err != nil {
		return comandoNaTela{}, err
	}
	env := make([]string, 0, len(c.Ambiente))
	for _, v := range c.Ambiente {
		env = append(env, v.Nome+"="+v.Valor)
	}
	// Rodar de novo para a execução anterior (e espera ela sair).
	s.Execucoes.Fechar(id)
	tamanho := terminal.Tamanho{Colunas: pedido.Cols, Linhas: pedido.Rows}.OuPadrao()
	pty, err := terminal.Iniciar(linhaDeComando(c.Comando), env, pasta, tamanho)
	if err != nil {
		return comandoNaTela{}, dados.ErrInvalido{Motivo: "não consegui rodar: " + err.Error()}
	}
	sessao := terminal.NovaSessao(id, pty, s.Bytes)
	sessao.DefinirTamanho(tamanho)
	agora := time.Now()
	estado := estadoExecucao{Desde: agora.UTC().Format(time.RFC3339Nano), Onde: pasta, Tarefa: pedido.Tarefa}
	registroExecucoes.guardar(id, estado)
	sessao.AoTerminar(func(saida terminal.Saida, pelaColmeia string) {
		fim := &fimExecucao{Codigo: saida.Codigo, Parada: pelaColmeia != "", Hora: hora(time.Now())}
		// Rodou de novo antes deste fim chegar: o estado já é da execução nova.
		if atual := registroExecucoes.pegar(id); atual.Desde == estado.Desde {
			registroExecucoes.guardar(id, estadoExecucao{Onde: pasta, Tarefa: pedido.Tarefa, Fim: fim})
		}
		s.Avisos.Publicar(perfil, map[string]any{
			"tipo": "comando.terminou", "comando_id": id, "projeto_id": c.ProjetoID, "desde": estado.Desde,
			"codigo": fim.Codigo, "parada": fim.Parada, "hora": fim.Hora,
		})
		s.atualizarSeFecharam()
	})
	s.Execucoes.Adicionar(sessao)
	s.Avisos.Publicar(perfil, map[string]any{
		"tipo": "comando.iniciou", "comando_id": id, "projeto_id": c.ProjetoID, "desde": estado.Desde, "onde": pasta, "tarefa_id": pedido.Tarefa,
	})
	resposta := s.naTela(c)
	resposta.Rodando = true
	return resposta, nil
}

func (s *Servidor) pararComando(w http.ResponseWriter, r *http.Request) {
	id, err := idDaRota(r)
	if err != nil {
		responderErro(w, err)
		return
	}
	if _, _, err := s.Banco.ComandoPorID(r.Context(), id); err != nil {
		responderErro(w, err)
		return
	}
	// O programa tem uns segundos para sair antes de ser forçado: a resposta
	// não espera, e o fim chega à tela pelo evento comando.terminou. O
	// terminal continua aberto, com o que foi escrito.
	go s.Execucoes.Parar(id)
	w.WriteHeader(http.StatusNoContent)
}

// terminalDoComando liga a tela ao terminal da última execução, que fica
// aberto (com o que foi escrito) até a próxima ou até remover.
func (s *Servidor) terminalDoComando(w http.ResponseWriter, r *http.Request) {
	id, err := idDaRota(r)
	if err != nil {
		http.Error(w, "terminal inexistente", http.StatusNotFound)
		return
	}
	sessao, ok := s.Execucoes.Pegar(id)
	if !ok {
		http.Error(w, "a configuração ainda não rodou", http.StatusNotFound)
		return
	}
	s.transmitir(w, r, sessao)
}
