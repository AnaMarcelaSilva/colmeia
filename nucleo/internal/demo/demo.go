// Package demo tem as cargas de teste usadas para medir as telas. Elas
// escrevem comandos nos terminais, por isso só existem com `--demo`.
package demo

import (
	"fmt"
	"time"

	"github.com/AnaMarcelaSilva/colmeia/nucleo/internal/terminal"
)

var cargas = map[string]string{
	// Parecido com um agente trabalhando: ~20 linhas/s por terminal, sem abrir
	// processos por linha (a espera usa o read embutido do bash).
	"leve": `clear; exec {espera}<> <(:); i=0; while :; do i=$((i+1)); printf '\033[2m%(%T)T\033[0m \033[36m[agente]\033[0m lendo \033[33msrc/modulo_%d.go\033[0m ... \033[32mok\033[0m\n' -1 $i; read -t 0.05 -u $espera; done`,
	// Estresse: saída contínua na velocidade máxima.
	"pesada": `clear; yes $'\033[36m[build]\033[0m compilando \033[1;33minternal/agentes\033[0m \033[35mpacote\033[0m ... \033[32mok\033[0m'`,
}

// Aplicar interrompe o que estiver rodando e inicia a carga pedida em todos os
// terminais ("parada" só limpa a tela).
func Aplicar(sessoes []*terminal.Sessao, modo string) error {
	cmd, existe := cargas[modo]
	if !existe && modo != "parada" {
		return fmt.Errorf("carga desconhecida: %q", modo)
	}
	for _, s := range sessoes {
		s.Escrever([]byte{3}) // Ctrl+C no que estiver rodando
	}
	time.Sleep(100 * time.Millisecond)
	for _, s := range sessoes {
		if existe {
			s.Escrever([]byte(cmd + "\r"))
		} else {
			s.Escrever([]byte("clear\r"))
		}
	}
	return nil
}
