//go:build !linux

package processos

// RodandoFora ainda não sabe olhar os processos fora do Linux.
func RodandoFora(nome, dir string) bool { return false }
