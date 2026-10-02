//go:build linux

package processos

import (
	"os"
	"os/exec"
	"testing"
)

func TestRodandoForaVeSoOQueNaoTemAMarca(t *testing.T) {
	dir := t.TempDir()
	if RodandoFora("sleep", dir) {
		t.Fatal("nada rodando ainda")
	}
	fora := exec.Command("sleep", "30")
	fora.Dir = dir
	fora.Start()
	defer fora.Process.Kill()
	dentro := exec.Command("sleep", "30")
	dentro.Dir = dir
	dentro.Env = append(os.Environ(), Marca+"=1")
	dentro.Start()
	defer dentro.Process.Kill()
	if !RodandoFora("sleep", dir) {
		t.Error("não viu o processo aberto fora")
	}
	fora.Process.Kill()
	fora.Wait()
	if RodandoFora("sleep", dir) {
		t.Error("contou o processo da Colmeia como de fora")
	}
}
