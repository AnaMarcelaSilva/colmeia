// Package ferramentas descobre quais ferramentas de agente estão instaladas e
// diz onde fica a conta de cada uma quando o perfil usa uma conta separada.
package ferramentas

import (
	"context"
	"os/exec"
	"path/filepath"
	"strconv"
	"strings"
	"time"
)

type Ferramenta struct {
	ID        string `json:"id"`
	Nome      string `json:"nome"`
	Instalada bool   `json:"instalada"`
	Versao    string `json:"versao,omitempty"`
	Caminho   string `json:"caminho,omitempty"`
	// ContaSeparada diz se dá para ter uma conta por perfil, e por qual variável.
	ContaSeparada bool   `json:"conta_separada"`
	Variavel      string `json:"variavel,omitempty"`
}

type conhecida struct {
	id, nome, executavel, variavel string
}

// Claude Code e Codex guardam configuração e login numa pasta que dá para
// trocar por variável de ambiente; é isso que separa as contas por perfil.
var conhecidas = []conhecida{
	{"claude", "Claude Code", "claude", "CLAUDE_CONFIG_DIR"},
	{"codex", "Codex", "codex", "CODEX_HOME"},
	{"gemini", "Gemini CLI", "gemini", ""},
	{"opencode", "OpenCode", "opencode", ""},
}

// Detectar procura cada ferramenta no PATH e lê a versão (com limite de tempo).
func Detectar(ctx context.Context) []Ferramenta {
	lista := make([]Ferramenta, 0, len(conhecidas))
	for _, c := range conhecidas {
		f := Ferramenta{ID: c.id, Nome: c.nome, ContaSeparada: c.variavel != "", Variavel: c.variavel}
		if caminho, err := exec.LookPath(c.executavel); err == nil {
			f.Instalada, f.Caminho = true, caminho
			f.Versao = versao(ctx, caminho)
		}
		lista = append(lista, f)
	}
	return lista
}

func versao(ctx context.Context, caminho string) string {
	ctx, cancelar := context.WithTimeout(ctx, 3*time.Second)
	defer cancelar()
	saida, err := exec.CommandContext(ctx, caminho, "--version").Output()
	if err != nil {
		return ""
	}
	linha, _, _ := strings.Cut(strings.TrimSpace(string(saida)), "\n")
	if len(linha) > 60 {
		linha = linha[:60]
	}
	return linha
}

// PastaDaConta é onde fica a conta separada de uma ferramenta num perfil.
func PastaDaConta(dirDados string, perfil int64, ferramenta string) string {
	return filepath.Join(dirDados, "perfis", strconv.FormatInt(perfil, 10), "contas", ferramenta)
}

// Variavel devolve a variável de ambiente que aponta a conta separada, ou "".
func Variavel(ferramenta string) string {
	for _, c := range conhecidas {
		if c.id == ferramenta {
			return c.variavel
		}
	}
	return ""
}
