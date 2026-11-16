package formato

import (
	"strings"
	"testing"
	"unicode/utf8"

	"github.com/AnaMarcelaSilva/colmeia/nucleo/internal/segredos"
)

func TestExemploCabeNoFormato(t *testing.T) {
	if n := utf8.RuneCountInString(ExemploNotaDaily); n > MaxNotaDeConclusao {
		t.Fatalf("o exemplo tem %d caracteres; o formato pede até %d", n, MaxNotaDeConclusao)
	}
	if linhas := strings.Count(ExemploNotaDaily, "\n") + 1; linhas > MaxLinhasDaNota {
		t.Fatalf("o exemplo tem %d linhas; o formato pede até %d", linhas, MaxLinhasDaNota)
	}
	if segredos.Parece(ExemploNotaDaily) {
		t.Fatal("o exemplo parece ter um segredo")
	}
	primeira, _, _ := strings.Cut(ExemploNotaDaily, "\n")
	if strings.HasPrefix(primeira, "-") || strings.HasPrefix(primeira, "1.") {
		t.Fatal("a primeira linha do exemplo deve ser a frase do resultado, não um item")
	}
	ultima := ExemploNotaDaily[strings.LastIndex(ExemploNotaDaily, "\n")+1:]
	if !strings.HasPrefix(ultima, "- Falta") {
		t.Fatalf("o item de pendência vem por último: %q", ultima)
	}
	// O fluxo novo vai numerado, na ordem.
	if !strings.Contains(ExemploNotaDaily, "\n1. ") || !strings.Contains(ExemploNotaDaily, "\n3. ") {
		t.Fatal("o exemplo mostra um fluxo numerado")
	}
}

func TestPedidoDailyCitaOFormato(t *testing.T) {
	p := PedidoDaily("Revisão")
	for _, trecho := range []string{"foi para Revisão", "complementar_nota", "o que falta", "Sem nomes de arquivo", "sintoma"} {
		if !strings.Contains(p, trecho) {
			t.Errorf("o pedido não diz %q", trecho)
		}
	}
	if strings.ContainsAny(Estilo+NotaDaily+p+AoTerminar, "—–") {
		t.Error("travessão no texto")
	}
}

func TestEstiloCobreOQueElaEscreve(t *testing.T) {
	// Os oito pontos do jeito dela, pelos trechos que o agente precisa ver.
	for _, trecho := range []string{
		"substantivo", "sintoma", "causa", "nome técnico e a tecnologia", "coluna nova", "endpoint novo", "numerado", "contexto vem antes",
		"de quem depende", "com sucesso", "tópico de duas a quatro palavras", "tela do erro", "Nunca terminal",
	} {
		if !strings.Contains(Estilo, trecho) {
			t.Errorf("o estilo não fala de %q", trecho)
		}
	}
	if segredos.Parece(Estilo) {
		t.Error("o estilo parece ter um segredo")
	}
}
