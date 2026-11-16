package git

import (
	"context"
	"errors"
	"os"
	"os/exec"
	"path/filepath"
	"testing"
)

func repositorioDeTeste(t *testing.T) string {
	t.Helper()
	raiz := t.TempDir()
	for _, args := range [][]string{
		{"init", "-q", "-b", "main"},
		{"-c", "user.email=teste@exemplo", "-c", "user.name=Teste", "commit", "-q", "--allow-empty", "-m", "início"},
	} {
		cmd := exec.Command("git", append([]string{"-C", raiz}, args...)...)
		if saida, err := cmd.CombinedOutput(); err != nil {
			t.Fatalf("git %v: %v\n%s", args, err, saida)
		}
	}
	return raiz
}

func TestRepositorioEPastaSemGit(t *testing.T) {
	ctx := context.Background()
	raiz := repositorioDeTeste(t)
	encontrada, branch, err := Repositorio(ctx, raiz)
	if err != nil || branch != "main" {
		t.Fatalf("repositório: %q %q %v", encontrada, branch, err)
	}
	if _, _, err := Repositorio(ctx, t.TempDir()); !errors.Is(err, ErrNaoRepositorio) {
		t.Errorf("pasta sem git: %v", err)
	}
	if _, _, err := Repositorio(ctx, "relativa"); err == nil || errors.Is(err, ErrNaoRepositorio) {
		t.Errorf("caminho relativo: %v", err)
	}
}

func TestCopiaIsolada(t *testing.T) {
	ctx := context.Background()
	raiz := repositorioDeTeste(t)
	destino := filepath.Join(t.TempDir(), "copia")
	if err := CriarCopia(ctx, raiz, destino, "feature/tela", "main", true); err != nil {
		t.Fatal(err)
	}
	_, branch, err := Repositorio(ctx, destino)
	if err != nil || branch != "feature/tela" {
		t.Fatalf("cópia na branch errada: %q %v", branch, err)
	}
	// Com mudança sem commit, a cópia não é apagada.
	os.WriteFile(filepath.Join(destino, "novo.txt"), []byte("x"), 0o600)
	if err := RemoverCopia(ctx, raiz, destino); !errors.Is(err, ErrMudancas) {
		t.Fatalf("esperava ErrMudancas, veio %v", err)
	}
	os.Remove(filepath.Join(destino, "novo.txt"))
	if err := RemoverCopia(ctx, raiz, destino); err != nil {
		t.Fatal(err)
	}
	if _, err := os.Stat(destino); !os.IsNotExist(err) {
		t.Error("a cópia continuou no disco")
	}
	branches, _ := Branches(ctx, raiz)
	if len(branches) != 2 {
		t.Errorf("a branch da cópia deveria continuar: %v", branches)
	}
	// Usar uma branch que já existe.
	if err := CriarCopia(ctx, raiz, destino, "feature/tela", "", false); err != nil {
		t.Fatal(err)
	}
}

func TestLeituraComLimiteESemBase(t *testing.T) {
	ctx := context.Background()
	raiz := repositorioDeTeste(t)
	for _, nome := range []string{"a.txt", "b.txt", "c.txt"} {
		os.WriteFile(filepath.Join(raiz, nome), []byte("pedido\npedido\n"), 0o600)
	}
	achados, mais, err := Grep(ctx, raiz, "pedido", false, 4)
	if err != nil || len(achados) != 4 || !mais {
		t.Errorf("grep com limite: %v %v %v", achados, mais, err)
	}
	if achados, mais, err := Grep(ctx, raiz, "nada-disso", false, 4); err != nil || len(achados) != 0 || mais {
		t.Errorf("grep sem achado: %v %v %v", achados, mais, err)
	}
	lista, mais, err := ListarArquivos(ctx, raiz, 2)
	if err != nil || len(lista) != 2 || !mais {
		t.Errorf("lista com limite: %v %v %v", lista, mais, err)
	}
	exec.Command("git", "-C", raiz, "branch", "-m", "main", "trabalho").Run()
	if _, _, err := Base(ctx, raiz); !errors.Is(err, ErrSemBase) {
		t.Errorf("sem main nem master: %v", err)
	}
	if _, _, err := Base(ctx, t.TempDir()); !errors.Is(err, ErrNaoRepositorio) {
		t.Errorf("sem git: %v", err)
	}
}

func TestUltimaTagECommitsDesde(t *testing.T) {
	raiz := repositorioDeTeste(t)
	ctx := context.Background()
	// Sem tag: nomes vazios, sem erro.
	if nome, data, err := UltimaTag(ctx, raiz); err != nil || nome != "" || data != "" {
		t.Fatalf("sem tag: %q %q %v", nome, data, err)
	}
	git := func(args ...string) {
		t.Helper()
		cmd := exec.Command("git", append([]string{"-C", raiz, "-c", "user.email=teste@exemplo", "-c", "user.name=Teste"}, args...)...)
		if saida, err := cmd.CombinedOutput(); err != nil {
			t.Fatalf("git %v: %v\n%s", args, err, saida)
		}
	}
	git("tag", "v1.0.0")
	git("commit", "-q", "--allow-empty", "-m", "corrige o desconto")
	git("commit", "-q", "--allow-empty", "-m", "melhora a lista")
	nome, data, err := UltimaTag(ctx, raiz)
	if err != nil || nome != "v1.0.0" || len(data) != 10 {
		t.Fatalf("última tag: %q %q %v", nome, data, err)
	}
	lista, mais, err := CommitsDesde(ctx, raiz, nome, 10)
	if err != nil || mais || len(lista) != 2 || lista[0].Mensagem != "melhora a lista" || lista[1].Mensagem != "corrige o desconto" {
		t.Fatalf("commits desde a tag: %v %v %v", lista, mais, err)
	}
	// Só o que ficou depois da tag e até o máximo.
	if lista, mais, _ := CommitsDesde(ctx, raiz, nome, 1); len(lista) != 1 || !mais {
		t.Fatalf("máximo: %v %v", lista, mais)
	}
	// Uma referência que parece opção ou intervalo não passa ao git.
	for _, ruim := range []string{"--output=/tmp/x", "a..b", ""} {
		if _, _, err := CommitsDesde(ctx, raiz, ruim, 10); err == nil {
			t.Errorf("aceitou %q", ruim)
		}
	}
}
