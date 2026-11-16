//go:build unix

package atualizacao

import (
	"os/exec"
	"syscall"
)

func processoVivo(pid int) bool { return syscall.Kill(pid, 0) == nil }

// soltar põe o programa numa sessão própria, sem entrada nem saída: o
// núcleo que abriu a troca vai sair, e a troca (e o que ela abre) segue.
func soltar(comando *exec.Cmd) {
	comando.Stdin, comando.Stdout, comando.Stderr = nil, nil, nil
	comando.SysProcAttr = &syscall.SysProcAttr{Setsid: true}
}

func iniciarSolto(caminho string, args ...string) error {
	comando := exec.Command(caminho, args...)
	soltar(comando)
	if err := comando.Start(); err != nil {
		return err
	}
	// Quem abriu sai logo; o sistema recolhe o processo depois.
	go comando.Wait()
	return nil
}
