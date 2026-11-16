package api

import (
	"net/http"
	"os"
	"os/exec"
	"path/filepath"
	"testing"
)

func TestSituacaoNoGithubEEnviarABranch(t *testing.T) {
	srv := servidorComDados(t)
	pedir(t, "POST", srv.URL+"/v1/perfis", map[string]string{"nome": "Pessoal"})
	pedir(t, "POST", srv.URL+"/v1/perfis/1/workspaces", map[string]string{"nome": "Loja"})

	// Uma pasta sem git: a situação é erro, com a frase.
	pedir(t, "POST", srv.URL+"/v1/workspaces/1/projetos", map[string]string{"nome": "clientes", "caminho": t.TempDir()})
	pedir(t, "POST", srv.URL+"/v1/projetos/1/tarefas", map[string]string{"titulo": "Sem git"})
	if status, r := pedir(t, "GET", srv.URL+"/v1/tarefas/1/github", nil); status != http.StatusBadRequest || r["erro"] != "A tarefa não está num repositório git." {
		t.Errorf("sem git: %d %v", status, r)
	}

	// Um repositório sem origin: aviso, sem erro.
	repo := repositorioGit(t)
	pedir(t, "POST", srv.URL+"/v1/workspaces/1/projetos", map[string]string{"nome": "loja-web", "caminho": repo})
	pedir(t, "POST", srv.URL+"/v1/projetos/2/tarefas", map[string]string{"titulo": "Frete", "branch": "main"})
	status, r := pedir(t, "GET", srv.URL+"/v1/tarefas/2/github", nil)
	if status != http.StatusOK || r["branch"] != "main" || r["origin"] != false || r["aviso"] != "O repositório não tem um remoto origin." {
		t.Fatalf("sem origin: %d %v", status, r)
	}
	if status, r := pedir(t, "POST", srv.URL+"/v1/tarefas/2/github/enviar", nil); status != http.StatusBadRequest {
		t.Errorf("enviar sem origin: %d %v", status, r)
	}

	// Com um origin local e a tarefa numa branch própria.
	origem := filepath.Join(t.TempDir(), "origem.git")
	git := func(args ...string) {
		t.Helper()
		cmd := exec.Command("git", append([]string{"-C", repo, "-c", "user.email=t@t", "-c", "user.name=t"}, args...)...)
		if saida, err := cmd.CombinedOutput(); err != nil {
			t.Fatalf("git %v: %v\n%s", args, err, saida)
		}
	}
	if saida, err := exec.Command("git", "init", "-q", "--bare", origem).CombinedOutput(); err != nil {
		t.Fatal(err, string(saida))
	}
	git("remote", "add", "origin", origem)
	git("push", "-q", "-u", "origin", "main")
	status, r = pedir(t, "GET", srv.URL+"/v1/tarefas/2/github", nil)
	if status != http.StatusOK || r["aviso"] != "A tarefa está na branch main, a mesma da base: não há PR a abrir." {
		t.Fatalf("na base: %d %v", status, r)
	}
	git("checkout", "-q", "-b", "tarefa/frete")
	os.WriteFile(filepath.Join(repo, "frete.txt"), []byte("x"), 0o600)
	git("add", ".")
	git("commit", "-q", "-m", "frete")
	status, r = pedir(t, "GET", srv.URL+"/v1/tarefas/2/github", nil)
	if status != http.StatusOK || r["origin"] != true || r["branch"] != "tarefa/frete" || r["base"] != "main" || r["enviada"] != false || r["a_enviar"] != 1.0 || r["repositorio"] != "" || r["comparar"] != "" {
		t.Fatalf("antes de enviar: %d %v", status, r)
	}
	if r["aviso"] != "O origin não está no GitHub ()." {
		t.Errorf("aviso do origin local: %v", r["aviso"])
	}
	status, r = pedir(t, "POST", srv.URL+"/v1/tarefas/2/github/enviar", nil)
	if status != http.StatusOK || r["enviada"] != true || r["a_enviar"] != 0.0 {
		t.Fatalf("enviar: %d %v", status, r)
	}
}
