//go:build unix

package terminal

import (
	"os"
	"os/exec"

	"github.com/creack/pty"
)

type ptyUnix struct {
	*os.File
	cmd *exec.Cmd
}

func (p ptyUnix) Redimensionar(colunas, linhas uint16) error {
	return pty.Setsize(p.File, &pty.Winsize{Cols: colunas, Rows: linhas})
}

// Close encerra o terminal e o processo dentro dele.
func (p ptyUnix) Close() error {
	err := p.File.Close()
	if p.cmd.Process != nil {
		p.cmd.Process.Kill()
		p.cmd.Wait()
	}
	return err
}

// Iniciar abre um pseudo-terminal rodando `comando`, com as variáveis `env` somadas às do núcleo.
func Iniciar(comando []string, env []string, dir string) (Pty, error) {
	cmd := exec.Command(comando[0], comando[1:]...)
	cmd.Env = append(append(os.Environ(), "TERM=xterm-256color"), env...)
	cmd.Dir = dir
	arquivo, err := pty.StartWithSize(cmd, &pty.Winsize{Cols: 80, Rows: 24})
	if err != nil {
		return nil, err
	}
	return ptyUnix{File: arquivo, cmd: cmd}, nil
}
