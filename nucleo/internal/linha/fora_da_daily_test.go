package linha

import (
	"strings"
	"testing"
)

// Uma tarefa tirada da daily de hoje sai do deck e do texto, mas continua na
// sprint, e não muda o dia anterior que a daily olha.
func TestTarefaForaDaDaily(t *testing.T) {
	c := contexto()
	c.ForaDaDaily = map[int64]bool{2: true}

	d := Daily(semana(t), c)
	if strings.Contains(d.Texto, "Corrigir desconto") || !strings.Contains(d.Texto, "Na sexta (25/09): avancei Nova tela de pedidos") {
		t.Errorf("texto da daily:\n%s", d.Texto)
	}
	if d.Periodo != "Desde sexta (25/09) · loja-web" {
		t.Errorf("período: %q", d.Periodo)
	}

	deck := Apresentacao(semana(t), em("2026-09-28 00:00"), em("2026-09-28 00:00"), c, "daily")
	if got := strings.Join(titulos(deck), " | "); got != "Nova tela de pedidos" {
		t.Errorf("slides da daily: %s", got)
	}
	if len(deck.Fora) != 1 || deck.Fora[0].TarefaID != 2 || deck.Fora[0].Titulo != "Corrigir desconto" || deck.Fora[0].Projeto != "loja-web" {
		t.Errorf("fora da daily: %+v", deck.Fora)
	}
	if deck.Capa.Numeros.Concluidas != 0 || deck.Capa.Numeros.Erros != 0 {
		t.Errorf("números da capa contam a tarefa tirada: %+v", deck.Capa.Numeros)
	}

	sprint := Apresentacao(semana(t), em("2026-09-21 00:00"), em("2026-09-28 00:00"), c, "sprint")
	if got := strings.Join(titulos(sprint), " | "); !strings.Contains(got, "Corrigir desconto") || len(sprint.Fora) != 0 {
		t.Errorf("a sprint perdeu a tarefa: %s / %+v", got, sprint.Fora)
	}

	// Tirando a que espera você também: nada sobra, e o período continua na sexta.
	c.ForaDaDaily[1] = true
	d = Daily(semana(t), c)
	if d.Periodo != "Desde sexta (25/09) · loja-web" || d.Texto != "Nada para falar: 2 tarefas tiradas desta daily." || !d.Vazio {
		t.Errorf("daily com as duas fora: %q\n%s", d.Periodo, d.Texto)
	}
	deck = Apresentacao(semana(t), em("2026-09-28 00:00"), em("2026-09-28 00:00"), c, "daily")
	if len(deck.Slides) != 0 || len(deck.Fora) != 2 || deck.Periodo != "desde sexta, 25 de setembro · loja-web" {
		t.Errorf("deck com as duas fora: %+v", deck)
	}
}
