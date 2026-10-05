//go:build unix

package navegador

import (
	"io"
	"os"
	"os/exec"
	"syscall"
)

// chromeAberto é o navegador rodando: as pontas do pipe do lado do núcleo,
// esperar (bloqueia até ele sair) e matar (encerra ele e o que ele abriu).
type chromeAberto struct {
	le, escreve *os.File
	esperar     func()
	matar       func()
}

func instalados() []string { return nil }

// abrirChrome inicia o navegador com o pipe de controle nos fds 3 (o Chrome
// lê os comandos) e 4 (o Chrome escreve as respostas), num grupo de
// processos próprio.
func abrirChrome(executavel string, args []string) (*chromeAberto, error) {
	chromeLe, nucleoEscreve, err := os.Pipe()
	if err != nil {
		return nil, err
	}
	nucleoLe, chromeEscreve, err := os.Pipe()
	if err != nil {
		chromeLe.Close()
		nucleoEscreve.Close()
		return nil, err
	}
	cmd := exec.Command(executavel, args...)
	cmd.ExtraFiles = []*os.File{chromeLe, chromeEscreve}
	cmd.SysProcAttr = &syscall.SysProcAttr{Setpgid: true}
	cmd.Stdout, cmd.Stderr = io.Discard, io.Discard
	err = cmd.Start()
	chromeLe.Close()
	chromeEscreve.Close()
	if err != nil {
		nucleoEscreve.Close()
		nucleoLe.Close()
		return nil, err
	}
	return &chromeAberto{
		le:      nucleoLe,
		escreve: nucleoEscreve,
		esperar: func() { cmd.Wait() },
		matar:   func() { syscall.Kill(-cmd.Process.Pid, syscall.SIGKILL) },
	}, nil
}
