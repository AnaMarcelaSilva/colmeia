//go:build unix

package api

import (
	"bytes"
	"context"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"sync/atomic"
	"testing"
	"time"

	"github.com/coder/websocket"

	"github.com/AnaMarcelaSilva/colmeia/nucleo/internal/dados"
)

func servidorComDados(t *testing.T) *httptest.Server {
	t.Helper()
	dir := t.TempDir()
	banco, err := dados.Abrir(filepath.Join(dir, "dados"))
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { banco.Fechar() })
	srv := httptest.NewServer((&Servidor{Bytes: new(atomic.Int64), Banco: banco, DirDados: dir}).Rotas())
	t.Cleanup(srv.Close)
	return srv
}

func pedir(t *testing.T, metodo, url string, corpo any) (int, map[string]any) {
	t.Helper()
	var leitor *bytes.Reader
	if corpo != nil {
		b, _ := json.Marshal(corpo)
		leitor = bytes.NewReader(b)
	} else {
		leitor = bytes.NewReader(nil)
	}
	r, _ := http.NewRequest(metodo, url, leitor)
	resposta, err := http.DefaultClient.Do(r)
	if err != nil {
		t.Fatal(err)
	}
	defer resposta.Body.Close()
	var resultado map[string]any
	json.NewDecoder(resposta.Body).Decode(&resultado)
	return resposta.StatusCode, resultado
}

func repositorioGit(t *testing.T) string {
	t.Helper()
	dir := t.TempDir()
	for _, args := range [][]string{{"init", "-q", "-b", "main"}, {"-c", "user.email=t@t", "-c", "user.name=t", "commit", "-q", "--allow-empty", "-m", "inicio"}} {
		if saida, err := exec.Command("git", append([]string{"-C", dir}, args...)...).CombinedOutput(); err != nil {
			t.Fatalf("git %v: %v %s", args, err, saida)
		}
	}
	return dir
}

func TestProjetoGitOuPastaDeTrabalho(t *testing.T) {
	srv := servidorComDados(t)
	_, perfil := pedir(t, "POST", srv.URL+"/v1/perfis", map[string]string{"nome": "Profissional"})
	_, ws := pedir(t, "POST", srv.URL+"/v1/perfis/1/workspaces", map[string]string{"nome": "Empresa X"})
	if perfil["id"] == nil || ws["id"] == nil {
		t.Fatalf("perfil %v, workspace %v", perfil, ws)
	}

	// Uma pasta sem git entra como pasta de trabalho, sem branches.
	status, pasta := pedir(t, "POST", srv.URL+"/v1/workspaces/1/projetos", map[string]string{"nome": "clientes", "caminho": t.TempDir()})
	if status != http.StatusOK || pasta["tipo"] != "pasta" || pasta["branch_padrao"] != "" {
		t.Errorf("pasta sem git: status %d, %v", status, pasta)
	}
	for _, caminho := range []string{"algum/lugar", "/nao/existe/mesmo"} {
		status, _ = pedir(t, "POST", srv.URL+"/v1/workspaces/1/projetos", map[string]string{"nome": caminho, "caminho": caminho})
		if status != http.StatusBadRequest {
			t.Errorf("%s: status %d, esperado 400", caminho, status)
		}
	}

	repo := repositorioGit(t)
	status, projeto := pedir(t, "POST", srv.URL+"/v1/workspaces/1/projetos", map[string]string{"nome": "loja-web", "caminho": repo})
	if status != http.StatusOK || projeto["branch_padrao"] != "main" || projeto["tipo"] != "git" {
		t.Fatalf("repositório de verdade: status %d, %v", status, projeto)
	}

	status, tarefa := pedir(t, "POST", srv.URL+"/v1/projetos/2/tarefas", map[string]string{"titulo": "Nova tela", "branch": "main"})
	if status != http.StatusOK || tarefa["coluna"] != "backlog" || tarefa["local"] != "pasta" {
		t.Fatalf("criar tarefa: status %d, %v", status, tarefa)
	}
	status, _ = pedir(t, "POST", srv.URL+"/v1/projetos/2/tarefas", map[string]string{"titulo": "Perigosa", "branch": "--upload-pack=x"})
	if status != http.StatusBadRequest {
		t.Errorf("branch perigosa: status %d, esperado 400", status)
	}
	status, _ = pedir(t, "PATCH", srv.URL+"/v1/tarefas/1", map[string]string{"coluna": "revisao"})
	if status != http.StatusOK {
		t.Errorf("mover tarefa: status %d", status)
	}
	status, _ = pedir(t, "PATCH", srv.URL+"/v1/tarefas/1", map[string]any{"coluna": "revisao", "campo_desconhecido": 1})
	if status != http.StatusBadRequest {
		t.Errorf("campo desconhecido: status %d, esperado 400", status)
	}
	status, _ = pedir(t, "POST", srv.URL+"/v1/projetos/1/tarefas", map[string]string{"titulo": "Analisar cliente"})
	if status != http.StatusOK {
		t.Errorf("tarefa na pasta de trabalho: status %d", status)
	}
}

func TestCopiaIsoladaDaTarefa(t *testing.T) {
	srv := servidorComDados(t)
	pedir(t, "POST", srv.URL+"/v1/perfis", map[string]string{"nome": "Profissional"})
	pedir(t, "POST", srv.URL+"/v1/perfis/1/workspaces", map[string]string{"nome": "W"})
	repo := repositorioGit(t)
	pedir(t, "POST", srv.URL+"/v1/workspaces/1/projetos", map[string]string{"nome": "loja", "caminho": repo})

	status, tarefa := pedir(t, "POST", srv.URL+"/v1/projetos/1/tarefas", map[string]any{"titulo": "Tela", "branch": "feature/tela", "local": "copia", "nova": true})
	if status != http.StatusOK {
		t.Fatalf("criar com cópia: status %d, %v", status, tarefa)
	}
	copia, _ := tarefa["copia"].(string)
	if _, err := os.Stat(filepath.Join(copia, ".git")); err != nil {
		t.Fatalf("a cópia não foi criada em %q: %v", copia, err)
	}
	// Branch que já está em uso na pasta principal: o git recusa e a tarefa não fica.
	status, _ = pedir(t, "POST", srv.URL+"/v1/projetos/1/tarefas", map[string]any{"titulo": "Na main", "branch": "main", "local": "copia"})
	if status != http.StatusBadRequest {
		t.Errorf("cópia da branch em uso: status %d, esperado 400", status)
	}
	_, lista := pedirLista(t, srv.URL+"/v1/projetos/1/tarefas")
	if len(lista) != 1 {
		t.Errorf("a tarefa recusada ficou gravada: %v", lista)
	}

	t.Setenv("SHELL", "/bin/sh")
	pedir(t, "POST", srv.URL+"/v1/tarefas/1/agentes", map[string]string{"ferramenta": "shell", "papel": "dev"})
	os.WriteFile(filepath.Join(copia, "rascunho.txt"), []byte("x"), 0o600)
	status, resposta := pedir(t, "DELETE", srv.URL+"/v1/tarefas/1", nil)
	if status != http.StatusConflict {
		t.Errorf("remover com mudanças: status %d, %v", status, resposta)
	}
	// A remoção recusada não para os agentes.
	if _, lista := pedirLista(t, srv.URL+"/v1/tarefas/1/agentes"); len(lista) != 1 || lista[0]["ativo"] != true {
		t.Errorf("o agente parou com a remoção recusada: %v", lista)
	}
	os.Remove(filepath.Join(copia, "rascunho.txt"))
	if status, _ := pedir(t, "DELETE", srv.URL+"/v1/tarefas/1", nil); status != http.StatusOK {
		t.Errorf("remover limpa: status %d", status)
	}
	if _, err := os.Stat(copia); !os.IsNotExist(err) {
		t.Error("a cópia continuou no disco")
	}
}

func TestAgenteAbreNaPastaDaTarefa(t *testing.T) {
	t.Setenv("SHELL", "/bin/sh")
	srv := servidorComDados(t)
	pedir(t, "POST", srv.URL+"/v1/perfis", map[string]string{"nome": "Profissional"})
	pedir(t, "POST", srv.URL+"/v1/perfis/1/workspaces", map[string]string{"nome": "W"})
	pasta := t.TempDir()
	pedir(t, "POST", srv.URL+"/v1/workspaces/1/projetos", map[string]string{"nome": "clientes", "caminho": pasta})
	pedir(t, "POST", srv.URL+"/v1/projetos/1/tarefas", map[string]string{"titulo": "Analisar"})

	status, _ := pedir(t, "POST", srv.URL+"/v1/tarefas/1/agentes", map[string]string{"ferramenta": "shell", "papel": "dev", "sessao": "--dangerously-skip-permissions"})
	if status != http.StatusBadRequest {
		t.Errorf("conversa inválida: status %d, esperado 400", status)
	}
	status, agente := pedir(t, "POST", srv.URL+"/v1/tarefas/1/agentes", map[string]string{"ferramenta": "shell", "papel": "dev"})
	if status != http.StatusOK || agente["ativo"] != true {
		t.Fatalf("criar agente: status %d, %v", status, agente)
	}

	ctx, cancelar := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancelar()
	conn, _, err := websocket.Dial(ctx, "ws"+strings.TrimPrefix(srv.URL, "http")+"/v1/agentes/1/terminal", nil)
	if err != nil {
		t.Fatal(err)
	}
	defer conn.CloseNow()
	conn.Write(ctx, websocket.MessageBinary, []byte("pwd; echo MARCA=$COLMEIA_AGENTE; exit\r"))
	var saida strings.Builder
	for {
		tipo, dados, err := conn.Read(ctx)
		if err != nil {
			t.Fatalf("lendo o terminal: %v (até aqui: %q)", err, saida.String())
		}
		if tipo == websocket.MessageText && strings.Contains(string(dados), "fim") {
			break
		}
		saida.Write(dados)
	}
	if !strings.Contains(saida.String(), pasta) || !strings.Contains(saida.String(), "MARCA=1") {
		t.Errorf("o agente não abriu na pasta da tarefa: %q", saida.String())
	}

	_, lista := pedirLista(t, srv.URL+"/v1/tarefas/1/agentes")
	if len(lista) != 1 || lista[0]["ativo"] != false {
		t.Errorf("agente encerrado: %v", lista)
	}
	if _, doProjeto := pedirLista(t, srv.URL+"/v1/projetos/1/agentes"); len(doProjeto) != 1 {
		t.Errorf("agentes do projeto: %v", doProjeto)
	}
	if status, _ := pedir(t, "POST", srv.URL+"/v1/agentes/1/iniciar", nil); status != http.StatusOK {
		t.Errorf("reiniciar: status %d", status)
	}
	if status, _ := pedir(t, "DELETE", srv.URL+"/v1/agentes/1", nil); status != http.StatusOK {
		t.Errorf("remover: status %d", status)
	}
	status, sessoes := pedir(t, "GET", srv.URL+"/v1/tarefas/1/sessoes", nil)
	if status != http.StatusOK || sessoes["pasta"] != pasta {
		t.Errorf("conversas da pasta: status %d, %v", status, sessoes)
	}
}

func pedirLista(t *testing.T, url string) (int, []map[string]any) {
	t.Helper()
	resposta, err := http.Get(url)
	if err != nil {
		t.Fatal(err)
	}
	defer resposta.Body.Close()
	var lista []map[string]any
	json.NewDecoder(resposta.Body).Decode(&lista)
	return resposta.StatusCode, lista
}

func TestContaSeparadaSoParaQuemPermite(t *testing.T) {
	srv := servidorComDados(t)
	pedir(t, "POST", srv.URL+"/v1/perfis", map[string]string{"nome": "Pessoal"})
	status, _ := pedir(t, "PUT", srv.URL+"/v1/perfis/1/contas", []map[string]string{{"ferramenta": "claude", "modo": "separada"}})
	if status != http.StatusOK {
		t.Errorf("Claude Code com conta separada: status %d", status)
	}
	status, _ = pedir(t, "PUT", srv.URL+"/v1/perfis/1/contas", []map[string]string{{"ferramenta": "gemini", "modo": "separada"}})
	if status != http.StatusBadRequest {
		t.Errorf("Gemini com conta separada: status %d, esperado 400", status)
	}
}
