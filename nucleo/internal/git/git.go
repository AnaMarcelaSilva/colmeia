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

func rodar(ctx context.Context, dir string, args ...string) (string, error) {
	ctx, cancelar := context.WithTimeout(ctx, 5*time.Second)
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
		return "", "", errors.New("a pasta não é um repositório git")
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
