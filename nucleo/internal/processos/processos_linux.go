//go:build linux

package processos

import (
	"bytes"
	"os"
	"path/filepath"
	"strconv"
)

// RodandoFora diz se há um processo `nome` do usuário com a pasta de trabalho
// `dir` que não foi aberto pela Colmeia. Só lê /proc; processos de outros
// usuários não são acessíveis e ficam de fora.
func RodandoFora(nome, dir string) bool {
	entradas, err := os.ReadDir("/proc")
	if err != nil {
		return false
	}
	dir = filepath.Clean(dir)
	for _, e := range entradas {
		if _, err := strconv.Atoi(e.Name()); err != nil {
			continue
		}
		base := filepath.Join("/proc", e.Name())
		if cwd, err := os.Readlink(filepath.Join(base, "cwd")); err != nil || cwd != dir {
			continue
		}
		if !eDaFerramenta(base, nome) {
			continue
		}
		ambiente, err := os.ReadFile(filepath.Join(base, "environ"))
		if err != nil || bytes.Contains(ambiente, []byte(Marca+"=")) {
			continue
		}
		return true
	}
	return false
}

// eDaFerramenta confere o nome do processo e o executável na linha de comando
// (ferramentas feitas em Node aparecem como "node .../claude").
func eDaFerramenta(base, nome string) bool {
	if comm, err := os.ReadFile(filepath.Join(base, "comm")); err == nil && string(bytes.TrimSpace(comm)) == nome {
		return true
	}
	linha, err := os.ReadFile(filepath.Join(base, "cmdline"))
	if err != nil {
		return false
	}
	partes := bytes.SplitN(linha, []byte{0}, 3)
	for _, p := range partes[:min(len(partes), 2)] {
		if filepath.Base(string(p)) == nome {
			return true
		}
	}
	return false
}
