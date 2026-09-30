//go:build unix

package api

import (
	"bytes"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"os/exec"
	"path/filepath"
	"sync/atomic"
	"testing"

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

func TestProjetoSoEntraSeForRepositorio(t *testing.T) {
	srv := servidorComDados(t)
	_, perfil := pedir(t, "POST", srv.URL+"/v1/perfis", map[string]string{"nome": "Profissional"})
	_, ws := pedir(t, "POST", srv.URL+"/v1/perfis/1/workspaces", map[string]string{"nome": "Empresa X"})
	if perfil["id"] == nil || ws["id"] == nil {
		t.Fatalf("perfil %v, workspace %v", perfil, ws)
	}

	status, _ := pedir(t, "POST", srv.URL+"/v1/workspaces/1/projetos", map[string]string{"nome": "pasta comum", "caminho": t.TempDir()})
	if status != http.StatusBadRequest {
		t.Errorf("pasta sem git: status %d, esperado 400", status)
	}
	status, _ = pedir(t, "POST", srv.URL+"/v1/workspaces/1/projetos", map[string]string{"nome": "relativo", "caminho": "algum/lugar"})
	if status != http.StatusBadRequest {
		t.Errorf("caminho relativo: status %d, esperado 400", status)
	}

	repo := repositorioGit(t)
	status, projeto := pedir(t, "POST", srv.URL+"/v1/workspaces/1/projetos", map[string]string{"nome": "loja-web", "caminho": repo})
	if status != http.StatusOK || projeto["branch_padrao"] != "main" {
		t.Fatalf("repositório de verdade: status %d, %v", status, projeto)
	}

	status, tarefa := pedir(t, "POST", srv.URL+"/v1/projetos/1/tarefas", map[string]string{"titulo": "Nova tela", "branch": "main"})
	if status != http.StatusOK || tarefa["coluna"] != "backlog" {
		t.Fatalf("criar tarefa: status %d, %v", status, tarefa)
	}
	status, _ = pedir(t, "POST", srv.URL+"/v1/projetos/1/tarefas", map[string]string{"titulo": "Perigosa", "branch": "--upload-pack=x"})
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
