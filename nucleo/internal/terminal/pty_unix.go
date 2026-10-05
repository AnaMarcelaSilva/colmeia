//go:build unix

package terminal

import (
	"os"
	"os/exec"
	"syscall"
	"time"

	"github.com/creack/pty"
)

// Quanto esperar o programa terminar sozinho antes de forçar.
const esperaAoFechar = 3 * time.Second

type ptyUnix struct {
	*os.File
	cmd   *exec.Cmd
	fim   chan struct{} // fechado quando o processo termina
	saida *Saida        // gravada antes de fim fechar
}

func (p ptyUnix) Esperar() Saida {
	<-p.fim
	return *p.saida
}

func (p ptyUnix) Redimensionar(colunas, linhas uint16) error {
	return pty.Setsize(p.File, &pty.Winsize{Cols: colunas, Rows: linhas})
}

// Close avisa o programa que o terminal fechou (SIGHUP, como ao fechar uma
// janela de terminal) e dá a ele a chance de salvar. Se não terminar a tempo,
// o grupo inteiro de processos é encerrado, para não sobrar nada rodando que o
// agente tenha aberto. O aviso vai direto ao grupo: fechar o arquivo não
// bastaria, porque o Go só o fecha de fato quando a leitura pendente termina.
func (p ptyUnix) Close() error {
	grupo := -p.cmd.Process.Pid
	syscall.Kill(grupo, syscall.SIGHUP)
	syscall.Kill(grupo, syscall.SIGCONT)
	select {
	case <-p.fim:
	case <-time.After(esperaAoFechar):
		syscall.Kill(grupo, syscall.SIGKILL)
		<-p.fim
	}
	return p.File.Close()
}

// Iniciar abre um pseudo-terminal rodando `comando` na pasta `dir`, com as
// variáveis `env` somadas às do núcleo. O programa vira líder de uma sessão
// própria (e de um grupo de processos), com o terminal como controlador.
// O terminal já nasce no tamanho da tela: um programa como o Claude Code
// desenha logo ao abrir, e um desenho feito em outra largura fica embaralhado
// quando o terminal muda de tamanho depois.
func Iniciar(comando []string, env []string, dir string, tamanho Tamanho) (Pty, error) {
	cmd := exec.Command(comando[0], comando[1:]...)
	cmd.Env = append(append(AmbienteLimpo(os.Environ()), "TERM=xterm-256color", "COLORTERM=truecolor"), env...)
	cmd.Dir = dir
	arquivo, err := pty.StartWithSize(cmd, &pty.Winsize{Cols: tamanho.Colunas, Rows: tamanho.Linhas})
	if err != nil {
		return nil, err
	}
	fim := make(chan struct{})
	saida := &Saida{}
	go func() {
		cmd.Wait()
		if estado, ok := cmd.ProcessState.Sys().(syscall.WaitStatus); ok && estado.Signaled() {
			*saida = Saida{Codigo: 128 + int(estado.Signal()), PorSinal: true}
		} else {
			saida.Codigo = cmd.ProcessState.ExitCode()
		}
		close(fim)
	}()
	return ptyUnix{File: arquivo, cmd: cmd, fim: fim, saida: saida}, nil
}

// ShellPadrao é o shell de um agente "shell": o $SHELL da pessoa, ou o bash.
func ShellPadrao() string {
	if s := os.Getenv("SHELL"); s != "" {
		return s
	}
	return "/bin/bash"
}
