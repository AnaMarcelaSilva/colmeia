package api

import (
	"context"
	"crypto/rand"
	"errors"
	"fmt"
	"log"
	"net/http"
	"os"
	"os/exec"
	"path/filepath"
	"regexp"
	"strconv"
	"strings"

	"github.com/AnaMarcelaSilva/colmeia/nucleo/internal/dados"
	"github.com/AnaMarcelaSilva/colmeia/nucleo/internal/ferramentas"
	"github.com/AnaMarcelaSilva/colmeia/nucleo/internal/git"
	"github.com/AnaMarcelaSilva/colmeia/nucleo/internal/processos"
	"github.com/AnaMarcelaSilva/colmeia/nucleo/internal/sessoes"
	"github.com/AnaMarcelaSilva/colmeia/nucleo/internal/terminal"
)

func (s *Servidor) rotasAgentes(mux *http.ServeMux) {
	mux.HandleFunc("GET /v1/projetos/{id}/agentes", s.listarAgentesDoProjeto)
	mux.HandleFunc("GET /v1/tarefas/{id}/agentes", s.listarAgentes)
	mux.HandleFunc("POST /v1/tarefas/{id}/agentes", s.criarAgente)
	mux.HandleFunc("GET /v1/tarefas/{id}/sessoes", s.listarSessoes)
	mux.HandleFunc("POST /v1/agentes/{id}/iniciar", s.iniciarAgente)
	mux.HandleFunc("DELETE /v1/agentes/{id}", s.removerAgente)
	mux.HandleFunc("GET /v1/agentes/{id}/terminal", s.terminalDoAgente)
}

// agenteComEstado diz à tela se o terminal do agente está rodando, em que
// estado ele está e, se parou, como terminou.
type agenteComEstado struct {
	dados.Agente
	Ativo     bool       `json:"ativo"`
	Pasta     string     `json:"pasta,omitempty"`
	Estado    string     `json:"estado,omitempty"`
	Motivo    string     `json:"motivo,omitempty"`
	Desde     string     `json:"desde,omitempty"`
	DesdeHora string     `json:"desde_hora,omitempty"`
	UltimoFim *fimAgente `json:"ultimo_fim,omitempty"`
}

func (s *Servidor) listarAgentesDoProjeto(w http.ResponseWriter, r *http.Request) {
	id, err := idDaRota(r)
	if err != nil {
		responderErro(w, err)
		return
	}
	agentes, err := s.Banco.ListarAgentesDoProjeto(r.Context(), id)
	if err != nil {
		responderErro(w, err)
		return
	}
	lista := make([]agenteComEstado, 0, len(agentes))
	for _, a := range agentes {
		lista = append(lista, s.comEstado(a, "", nil))
	}
	responderJSON(w, lista)
}

func (s *Servidor) listarAgentes(w http.ResponseWriter, r *http.Request) {
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
	agentes, err := s.Banco.ListarAgentes(r.Context(), id)
	if err != nil {
		responderErro(w, err)
		return
	}
	lista := make([]agenteComEstado, 0, len(agentes))
	for _, a := range agentes {
		lista = append(lista, s.comEstado(a, tarefa.Pasta(projeto), nil))
	}
	responderJSON(w, lista)
}

// criarAgente grava o agente e já abre o terminal dele. Com `sessao`, o
// Claude Code retoma aquela conversa; sem ela, começa uma conversa nova com
// um id escolhido aqui, para poder ser retomada depois.
func (s *Servidor) criarAgente(w http.ResponseWriter, r *http.Request) {
	id, err := idDaRota(r)
	if err != nil {
		responderErro(w, err)
		return
	}
	var pedido struct{ Ferramenta, Papel, Sessao string }
	if err := ler(r, &pedido); err != nil {
		responderErro(w, err)
		return
	}
	if pedido.Sessao != "" && !sessoes.IDValido(pedido.Sessao) {
		responderErro(w, dados.ErrInvalido{Motivo: "conversa inválida"})
		return
	}
	if pedido.Ferramenta == "claude" && pedido.Sessao == "" {
		pedido.Sessao = novoUUID()
	}
	agente, err := s.Banco.CriarAgente(r.Context(), id, pedido.Ferramenta, pedido.Papel, pedido.Sessao)
	if err != nil {
		responderErro(w, err)
		return
	}
	if err := s.abrirTerminal(r.Context(), agente); err != nil {
		s.Banco.RemoverAgente(context.WithoutCancel(r.Context()), agente.ID)
		responderErro(w, err)
		return
	}
	responderJSON(w, s.comEstado(agente, "", nil))
}

func (s *Servidor) iniciarAgente(w http.ResponseWriter, r *http.Request) {
	id, err := idDaRota(r)
	if err != nil {
		responderErro(w, err)
		return
	}
	agente, err := s.Banco.Agente(r.Context(), id)
	if err != nil {
		responderErro(w, err)
		return
	}
	if !s.Agentes.Ativa(id) {
		if err := s.abrirTerminal(r.Context(), agente); err != nil {
			responderErro(w, err)
			return
		}
	}
	responderJSON(w, s.comEstado(agente, "", nil))
}

func (s *Servidor) removerAgente(w http.ResponseWriter, r *http.Request) {
	id, err := idDaRota(r)
	if err != nil {
		responderErro(w, err)
		return
	}
	s.Agentes.Fechar(id)
	if err := s.Banco.RemoverAgente(r.Context(), id); err != nil {
		responderErro(w, err)
		return
	}
	responderJSON(w, map[string]any{"ok": true})
}

func (s *Servidor) terminalDoAgente(w http.ResponseWriter, r *http.Request) {
	id, err := idDaRota(r)
	if err != nil {
		http.Error(w, "terminal inexistente", http.StatusNotFound)
		return
	}
	sessao, ok := s.Agentes.Pegar(id)
	if !ok {
		http.Error(w, "o agente não está rodando", http.StatusNotFound)
		return
	}
	s.transmitir(w, r, sessao)
}

// listarSessoes mostra as conversas do Claude Code guardadas para a pasta da
// tarefa, na conta que o perfil usa, e se o Claude Code está aberto nela fora
// da Colmeia (a mesma conversa em dois lugares se atropela).
func (s *Servidor) listarSessoes(w http.ResponseWriter, r *http.Request) {
	id, err := idDaRota(r)
	if err != nil {
		responderErro(w, err)
		return
	}
	tarefa, projeto, perfil, err := s.Banco.Tarefa(r.Context(), id)
	if err != nil {
		responderErro(w, err)
		return
	}
	separada, err := s.pastaSeparada(r.Context(), perfil, "claude")
	if err != nil {
		responderErro(w, err)
		return
	}
	configuracao, err := sessoes.PastaDeConfiguracao(separada)
	if err != nil {
		responderErro(w, err)
		return
	}
	pasta := tarefa.Pasta(projeto)
	lista, err := sessoes.Listar(configuracao, pasta)
	if err != nil {
		responderErro(w, err)
		return
	}
	responderJSON(w, map[string]any{"pasta": pasta, "sessoes": lista, "aberto_fora": processos.RodandoFora("claude", pasta)})
}

// pastaSeparada devolve a pasta da conta separada da ferramenta no perfil, ou
// "" quando o perfil usa a conta do sistema.
func (s *Servidor) pastaSeparada(ctx context.Context, perfil int64, ferramenta string) (string, error) {
	contas, err := s.Banco.ListarContas(ctx, perfil)
	if err != nil {
		return "", err
	}
	for _, c := range contas {
		if c.Ferramenta == ferramenta && c.Modo == "separada" && ferramentas.Variavel(ferramenta) != "" {
			return ferramentas.PastaDaConta(s.DirDados, perfil, ferramenta), nil
		}
	}
	return "", nil
}

// abrirTerminal inicia a ferramenta do agente na pasta da tarefa, com a conta
// do perfil. Nada vem da tela direto para a linha de comando: a ferramenta é
// uma da lista, e a conversa, um id já conferido.
func (s *Servidor) abrirTerminal(ctx context.Context, a dados.Agente) error {
	s.mu.Lock()
	defer s.mu.Unlock()
	if s.Agentes.Ativa(a.ID) {
		return nil
	}
	tarefa, projeto, perfil, err := s.Banco.Tarefa(ctx, a.TarefaID)
	if err != nil {
		return err
	}
	pasta := tarefa.Pasta(projeto)
	if info, err := os.Stat(pasta); err != nil || !info.IsDir() {
		return dados.ErrInvalido{Motivo: "a pasta da tarefa não existe mais: " + pasta}
	}
	env := []string{processos.Marca + "=" + strconv.FormatInt(a.ID, 10)}
	var comando []string
	if a.Ferramenta == "shell" {
		shell := os.Getenv("SHELL")
		if shell == "" {
			shell = "/bin/bash"
		}
		comando = []string{shell}
	} else {
		executavel, err := exec.LookPath(a.Ferramenta)
		if err != nil {
			return dados.ErrInvalido{Motivo: a.Ferramenta + " não está instalado"}
		}
		comando = []string{executavel}
		separada, err := s.pastaSeparada(ctx, perfil, a.Ferramenta)
		if err != nil {
			return err
		}
		if separada != "" {
			if err := os.MkdirAll(separada, 0o700); err != nil {
				return err
			}
			env = append(env, ferramentas.Variavel(a.Ferramenta)+"="+separada)
		}
		if a.Ferramenta == "claude" && sessoes.IDValido(a.Sessao) {
			configuracao, err := sessoes.PastaDeConfiguracao(separada)
			if err != nil {
				return err
			}
			// Conversa que já existe é retomada; senão começa com o id escolhido.
			if _, err := os.Stat(filepath.Join(sessoes.PastaDoProjeto(configuracao, pasta), a.Sessao+".jsonl")); err == nil {
				comando = append(comando, "--resume", a.Sessao)
			} else {
				comando = append(comando, "--session-id", a.Sessao)
			}
		}
	}
	contexto, err := s.Banco.ContextoDoAgente(ctx, a.ID)
	if err != nil {
		return err
	}
	pty, err := terminal.Iniciar(comando, env, pasta)
	if err != nil {
		return fmt.Errorf("abrindo o terminal: %w", err)
	}
	sessao := terminal.NovaSessao(a.ID, pty, s.Bytes)
	sessao.Acompanhar(a.Ferramenta, s.Tempos)
	s.guardarContexto(contexto)
	s.Agentes.Adicionar(sessao)
	ctx = context.WithoutCancel(ctx)
	if err := s.Banco.Registrar(ctx, "agente.iniciou", contexto.Escopo(), contexto); err != nil {
		log.Printf("agente %d: gravando o início: %v", a.ID, err)
	}
	// Um agente que começa numa tarefa do Backlog põe a tarefa em andamento.
	// Um terminal comum não: aberto e parado, ele não é trabalho acontecendo.
	if contexto.Coluna == "backlog" && a.Ferramenta != "shell" {
		trabalhando := "trabalhando"
		if _, err := s.Banco.AtualizarTarefa(ctx, contexto.TarefaID, dados.Mudanca{Coluna: &trabalhando}, dados.OrigemAutomatica); err != nil {
			log.Printf("tarefa %d: mudança automática de coluna: %v", contexto.TarefaID, err)
		}
	}
	return nil
}

// fecharAgentesDaTarefa encerra os terminais dos agentes da tarefa.
func (s *Servidor) fecharAgentesDaTarefa(ctx context.Context, tarefa int64) error {
	agentes, err := s.Banco.ListarAgentes(ctx, tarefa)
	if err != nil {
		return err
	}
	for _, a := range agentes {
		s.Agentes.Fechar(a.ID)
	}
	return nil
}

// Cópias isoladas

var foraDoNome = regexp.MustCompile(`[^A-Za-z0-9._-]+`)

// pastaDaCopia é onde fica a cópia isolada de uma tarefa: dentro dos dados da
// Colmeia, com o id da tarefa e a branch no nome para ficar fácil de achar.
func (s *Servidor) pastaDaCopia(tarefa int64, branch string) string {
	nome := strings.Trim(foraDoNome.ReplaceAllString(branch, "-"), "-.")
	return filepath.Join(s.DirDados, "copias", fmt.Sprintf("%d-%s", tarefa, nome))
}

// criarCopia tira a cópia isolada da tarefa e grava onde ela ficou. Se não der,
// a tarefa é desfeita.
func (s *Servidor) criarCopia(ctx context.Context, t dados.Tarefa, p dados.Projeto, base string, nova bool) (dados.Tarefa, error) {
	desfazer := func(err error) (dados.Tarefa, error) {
		s.Banco.RemoverTarefa(context.WithoutCancel(ctx), t.ID)
		return t, err
	}
	if nova {
		if base == "" {
			base = p.BranchPadrao
		}
		if err := dados.BranchValida(base); err != nil {
			return desfazer(err)
		}
	}
	if err := os.MkdirAll(filepath.Join(s.DirDados, "copias"), 0o700); err != nil {
		return desfazer(err)
	}
	destino := s.pastaDaCopia(t.ID, t.Branch)
	if err := git.CriarCopia(ctx, p.Caminho, destino, t.Branch, base, nova); err != nil {
		return desfazer(dados.ErrInvalido{Motivo: "o git não criou a cópia: " + err.Error()})
	}
	if err := s.Banco.DefinirCopia(ctx, t.ID, destino); err != nil {
		git.RemoverCopia(context.WithoutCancel(ctx), p.Caminho, destino)
		return desfazer(err)
	}
	t.Copia = destino
	return t, nil
}

// removerCopia tira a cópia isolada da tarefa, se ela tiver uma.
func (s *Servidor) removerCopia(ctx context.Context, t dados.Tarefa, p dados.Projeto) error {
	if t.Local != "copia" || t.Copia == "" {
		return nil
	}
	err := git.RemoverCopia(ctx, p.Caminho, t.Copia)
	if err != nil && !errors.Is(err, git.ErrMudancas) {
		return dados.ErrInvalido{Motivo: "o git não removeu a cópia: " + err.Error()}
	}
	return err
}

func novoUUID() string {
	var b [16]byte
	rand.Read(b[:])
	b[6] = b[6]&0x0f | 0x40
	b[8] = b[8]&0x3f | 0x80
	return fmt.Sprintf("%x-%x-%x-%x-%x", b[0:4], b[4:6], b[6:8], b[8:10], b[10:16])
}
