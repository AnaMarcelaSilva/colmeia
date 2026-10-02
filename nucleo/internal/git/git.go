// Package git consulta repositórios sem nunca passar por um shell: os
// argumentos vão direto para o executável do git.
package git

import (
	"context"
	"errors"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"time"
)

// ErrNaoRepositorio diz que a pasta existe mas não está num repositório git.
var ErrNaoRepositorio = errors.New("a pasta não é um repositório git")

// ErrMudancas diz que a cópia isolada tem mudanças que ainda não foram salvas num commit.
var ErrMudancas = errors.New("a cópia isolada tem mudanças que não estão em nenhum commit")

func rodar(ctx context.Context, dir string, args ...string) (string, error) {
	return rodarPor(ctx, 5*time.Second, dir, args...)
}

func rodarPor(ctx context.Context, limite time.Duration, dir string, args ...string) (string, error) {
	ctx, cancelar := context.WithTimeout(ctx, limite)
	defer cancelar()
	cmd := exec.CommandContext(ctx, "git", append([]string{"-C", dir}, args...)...)
	// Nunca pedir senha nem abrir editor: a consulta falha em vez de travar.
	cmd.Env = append(os.Environ(), "GIT_TERMINAL_PROMPT=0", "GIT_EDITOR=true", "LC_ALL=C")
	saida, err := cmd.Output()
	if err != nil {
		var falha *exec.ExitError
		if errors.As(err, &falha) {
			return "", fmt.Errorf("%s", strings.TrimSpace(string(falha.Stderr)))
		}
		return "", err
	}
	return strings.TrimSpace(string(saida)), nil
}

// Repositorio confere uma pasta e devolve a raiz do repositório e a branch atual.
func Repositorio(ctx context.Context, caminho string) (raiz, branch string, err error) {
	if !filepath.IsAbs(caminho) {
		return "", "", errors.New("use o caminho completo da pasta")
	}
	info, err := os.Stat(caminho)
	if err != nil || !info.IsDir() {
		return "", "", errors.New("a pasta não existe")
	}
	raiz, err = rodar(ctx, caminho, "rev-parse", "--show-toplevel")
	if err != nil {
		return "", "", ErrNaoRepositorio
	}
	branch, err = rodar(ctx, raiz, "rev-parse", "--abbrev-ref", "HEAD")
	if err != nil || branch == "HEAD" {
		// Repositório sem commits ou com HEAD solto: usa a branch configurada.
		branch, err = rodar(ctx, raiz, "symbolic-ref", "--short", "HEAD")
		if err != nil {
			return "", "", errors.New("não consegui descobrir a branch atual do repositório")
		}
	}
	return raiz, branch, nil
}

// Branches lista as branches locais.
func Branches(ctx context.Context, raiz string) ([]string, error) {
	saida, err := rodar(ctx, raiz, "for-each-ref", "--format=%(refname:short)", "refs/heads")
	if err != nil {
		return nil, err
	}
	branches := []string{}
	for _, linha := range strings.Split(saida, "\n") {
		if linha = strings.TrimSpace(linha); linha != "" {
			branches = append(branches, linha)
		}
	}
	return branches, nil
}

// CriarCopia cria uma cópia isolada do repositório (git worktree) em
// `destino`. Com `nova`, cria a branch a partir de `base`; senão usa uma
// branch que já existe. Os nomes já chegam validados, e nenhum começa com '-'.
func CriarCopia(ctx context.Context, raiz, destino, branch, base string, nova bool) error {
	if !filepath.IsAbs(destino) {
		return errors.New("a cópia precisa de um caminho completo")
	}
	args := []string{"worktree", "add"}
	if nova {
		args = append(args, "-b", branch, destino, base)
	} else {
		args = append(args, destino, branch)
	}
	// Tirar a cópia de um repositório grande demora mais que uma consulta.
	_, err := rodarPor(ctx, 2*time.Minute, raiz, args...)
	return err
}

// TemMudancas diz se a cópia tem arquivos alterados ou novos fora de um commit.
func TemMudancas(ctx context.Context, destino string) (bool, error) {
	if _, err := os.Stat(destino); os.IsNotExist(err) {
		return false, nil
	}
	status, err := rodar(ctx, destino, "status", "--porcelain")
	return status != "", err
}

// RemoverCopia tira a cópia isolada, mas recusa se houver mudanças sem commit.
// A branch continua existindo no repositório.
func RemoverCopia(ctx context.Context, raiz, destino string) error {
	if _, err := os.Stat(destino); os.IsNotExist(err) {
		// Já foi apagada por fora: só esquece o registro dela.
		_, err := rodar(ctx, raiz, "worktree", "prune")
		return err
	}
	mudou, err := TemMudancas(ctx, destino)
	if err != nil {
		return err
	}
	if mudou {
		return ErrMudancas
	}
	_, err = rodar(ctx, raiz, "worktree", "remove", destino)
	return err
}
