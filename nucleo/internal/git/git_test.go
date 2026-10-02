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
