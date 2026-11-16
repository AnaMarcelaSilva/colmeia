package git

import (
	"context"
	"errors"
	"os"
	"os/exec"
	"path/filepath"
	"testing"
)

func TestInterpretarRemoto(t *testing.T) {
	casos := map[string]Remoto{
		"git@github.com:loja/loja-web.git":         {Host: "github.com", Dono: "loja", Repo: "loja-web"},
		"ssh://git@github.com/loja/loja-web.git":   {Host: "github.com", Dono: "loja", Repo: "loja-web"},
		"https://github.com/loja/loja-web":         {Host: "github.com", Dono: "loja", Repo: "loja-web"},
		"https://GitHub.com/loja/loja-web.git/":    {Host: "github.com", Dono: "loja", Repo: "loja-web"},
		"git@github-pessoal:loja/loja-web.git":     {Host: "github-pessoal", Dono: "loja", Repo: "loja-web"},
		"https://gitlab.com/grupo/sub/projeto.git": {Host: "gitlab.com"},
		"https://github.com/loja/loja web.git":     {Host: "github.com"},
		"/tmp/repositorio-local":                   {},
		"git@github.com:loja/../../segredo.git":    {Host: "github.com"},
	}
	for endereco, esperado := range casos {
		if got := interpretarRemoto(endereco); got != esperado {
			t.Errorf("%s: %+v, esperado %+v", endereco, got, esperado)
		}
	}
	r := Remoto{Host: "github.com", Dono: "loja", Repo: "loja-web"}
	if got := r.Comparar("main", "tarefa/12 frete"); got != "https://github.com/loja/loja-web/compare/main...tarefa%2F12%20frete?expand=1" {
		t.Errorf("comparar: %s", got)
	}
	if got := r.Pulls("tarefa/12"); got != "https://github.com/loja/loja-web/pulls?q=is%3Apr+head%3Atarefa%2F12" {
		t.Errorf("pulls: %s", got)
	}
	if (Remoto{Host: "gitlab.com"}).Comparar("main", "x") != "" {
		t.Error("fora do GitHub não há página de comparar")
	}
}

// Um origin local (bare) para enviar sem rede.
func origemLocal(t *testing.T, raiz string) string {
	t.Helper()
	origem := filepath.Join(t.TempDir(), "origem.git")
	for _, args := range [][]string{
		{"init", "-q", "--bare", origem},
		{"-C", raiz, "remote", "add", "origin", origem},
		{"-C", raiz, "push", "-q", "-u", "origin", "main"},
	} {
		if saida, err := exec.Command("git", args...).CombinedOutput(); err != nil {
			t.Fatalf("git %v: %v\n%s", args, err, saida)
		}
	}
	return origem
}

func TestEnvioEEnviar(t *testing.T) {
	ctx := context.Background()
	raiz := repositorioDeTeste(t)
	if _, err := LerRemoto(ctx, raiz); !errors.Is(err, ErrSemRemoto) {
		t.Fatalf("sem origin: %v", err)
	}
	if _, err := LerRemoto(ctx, t.TempDir()); !errors.Is(err, ErrNaoRepositorio) {
		t.Fatalf("pasta sem git: %v", err)
	}
	origemLocal(t, raiz)
	if r, err := LerRemoto(ctx, raiz); err != nil || r.GitHub() {
		t.Fatalf("origin local: %+v %v", r, err)
	}
	git := func(args ...string) {
		t.Helper()
		cmd := exec.Command("git", append([]string{"-C", raiz, "-c", "user.email=t@t", "-c", "user.name=t"}, args...)...)
		if saida, err := cmd.CombinedOutput(); err != nil {
			t.Fatalf("git %v: %v\n%s", args, err, saida)
		}
	}
	git("checkout", "-q", "-b", "tarefa/frete")
	os.WriteFile(filepath.Join(raiz, "frete.txt"), []byte("x"), 0o600)
	git("add", ".")
	git("commit", "-q", "-m", "frete")
	_, base, err := Base(ctx, raiz)
	if err != nil {
		t.Fatal(err)
	}
	if e, err := LerEnvio(ctx, raiz, "tarefa/frete", base); err != nil || e != (Envio{Enviada: false, AEnviar: 1}) {
		t.Fatalf("antes de enviar: %+v %v", e, err)
	}
	if err := Enviar(ctx, raiz, "tarefa/frete"); err != nil {
		t.Fatal(err)
	}
	if e, err := LerEnvio(ctx, raiz, "tarefa/frete", base); err != nil || e != (Envio{Enviada: true, AEnviar: 0}) {
		t.Fatalf("depois de enviar: %+v %v", e, err)
	}
	git("commit", "-q", "--allow-empty", "-m", "mais um")
	if e, _ := LerEnvio(ctx, raiz, "tarefa/frete", base); e != (Envio{Enviada: true, AEnviar: 1}) {
		t.Errorf("commit novo depois do envio: %+v", e)
	}
	if err := Enviar(ctx, raiz, "-x"); err == nil {
		t.Error("branch começando com traço deveria ser recusada")
	}
}

func TestInterpretarPRResumeOsChecks(t *testing.T) {
	saida := []byte(`{"number":42,"url":"https://github.com/loja/loja-web/pull/42","title":"Frete","state":"OPEN","isDraft":false,
	  "statusCheckRollup":[
	    {"__typename":"CheckRun","status":"COMPLETED","conclusion":"SUCCESS"},
	    {"__typename":"CheckRun","status":"IN_PROGRESS","conclusion":""},
	    {"__typename":"StatusContext","state":"FAILURE"}]}`)
	pr, err := interpretarPR(saida)
	if err != nil {
		t.Fatal(err)
	}
	if pr.Numero != 42 || pr.Estado != "aberto" || pr.Checks != (Checks{Resumo: "falhou", Ok: 1, Falhou: 1, Rodando: 1, Total: 3}) {
		t.Errorf("pr: %+v", pr)
	}
	pr, _ = interpretarPR([]byte(`{"number":7,"state":"OPEN","isDraft":true,"statusCheckRollup":[{"__typename":"CheckRun","status":"COMPLETED","conclusion":"SKIPPED"}]}`))
	if pr.Estado != "rascunho" || pr.Checks.Resumo != "ok" {
		t.Errorf("rascunho: %+v", pr)
	}
	pr, _ = interpretarPR([]byte(`{"number":8,"state":"MERGED"}`))
	if pr.Estado != "incorporado" || pr.Checks.Resumo != "" || pr.Checks.Total != 0 {
		t.Errorf("incorporado: %+v", pr)
	}
}
