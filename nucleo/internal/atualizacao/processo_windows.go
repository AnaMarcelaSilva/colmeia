//go:build windows

package atualizacao

import (
	"errors"
	"os/exec"
)

// No Windows a Colmeia não troca os programas sozinha (Instalavel fica
// falso e a tela leva à página da release).
var errSoLinux = errors.New("a troca automática só existe no Linux")

func processoVivo(int) bool { return false }

func soltar(*exec.Cmd) {}

func iniciarSolto(string, ...string) error { return errSoLinux }
