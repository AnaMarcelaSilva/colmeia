package dados

import (
	"context"
	"testing"
	"time"
)

func TestProximaSprintSegueORitmo(t *testing.T) {
	dia := func(s string) time.Time { d, _ := time.Parse("2006-01-02", s); return d }
	casos := []struct {
		hoje    string
		ultima  *Sprint
		de, ate string
	}{
		// Sem sprint: de sexta a quinta (o dia da reunião), com hoje dentro.
		{"2026-10-08", nil, "2026-10-02", "2026-10-08"}, // quinta
		{"2026-10-09", nil, "2026-10-09", "2026-10-15"}, // sexta
		{"2026-10-06", nil, "2026-10-02", "2026-10-08"}, // terça
		// Depois da última: o dia seguinte, com a mesma duração.
		{"2026-10-09", &Sprint{Inicio: "2026-10-02", Fim: "2026-10-08"}, "2026-10-09", "2026-10-15"},
		// Duas semanas sem abrir: a do ciclo de hoje, no mesmo ritmo.
		{"2026-10-23", &Sprint{Inicio: "2026-10-02", Fim: "2026-10-08"}, "2026-10-23", "2026-10-29"},
		{"2026-10-22", &Sprint{Inicio: "2026-10-02", Fim: "2026-10-08"}, "2026-10-16", "2026-10-22"},
		// Sprint de 14 dias que termina numa quarta.
		{"2026-10-20", &Sprint{Inicio: "2026-10-01", Fim: "2026-10-14"}, "2026-10-15", "2026-10-28"},
	}
	for _, c := range casos {
		de, ate := proximaSprint(dia(c.hoje), c.ultima)
		if de != c.de || ate != c.ate {
			t.Errorf("hoje %s depois de %+v: %s a %s, queria %s a %s", c.hoje, c.ultima, de, ate, c.de, c.ate)
		}
	}
}

func TestSprintsDoPerfil(t *testing.T) {
	b, _ := bancoDeTeste(t)
	ctx := context.Background()
	p, _ := b.CriarPerfil(ctx, "Pessoal", "")
	o, _ := b.CriarPerfil(ctx, "Outro", "")
	ws, _ := b.CriarWorkspace(ctx, p.ID, "Clientes")
	wsOutro, _ := b.CriarWorkspace(ctx, o.ID, "W")
	projeto, _ := b.CriarProjeto(ctx, ws.ID, "loja", "/tmp/loja", "pasta", "")
	projetoOutro, _ := b.CriarProjeto(ctx, wsOutro.ID, "x", "/tmp/x", "pasta", "")
	tarefa, _ := b.CriarTarefa(ctx, projeto.ID, "Comparar bancos", "", "")
	doOutro, _ := b.CriarTarefa(ctx, projetoOutro.ID, "Relatório", "", "")

	// A atual nasce sozinha e é a mesma na segunda vez.
	atual, err := b.SprintAtual(ctx, p.ID, "2026-10-08")
	if err != nil || atual.Inicio != "2026-10-02" || atual.Fim != "2026-10-08" {
		t.Fatalf("sprint atual: %+v %v", atual, err)
	}
	if deNovo, _ := b.SprintAtual(ctx, p.ID, "2026-10-05"); deNovo.ID != atual.ID {
		t.Fatalf("criou outra sprint para um dia da mesma: %+v", deNovo)
	}
	// Não cruza outra sprint do perfil; outro perfil é livre.
	if _, err := b.CriarSprint(ctx, p.ID, "2026-10-08", "2026-10-14"); err == nil {
		t.Fatal("criou uma sprint por cima de outra")
	}
	if _, err := b.CriarSprint(ctx, o.ID, "2026-10-01", "2026-10-31"); err != nil {
		t.Fatalf("outro perfil: %v", err)
	}
	if _, err := b.CriarSprint(ctx, p.ID, "2026-10-14", "2026-10-09"); err == nil {
		t.Fatal("aceitou fim antes do início")
	}

	// Título só da sprint; vazio volta ao da tarefa; tarefa de outro perfil não entra.
	if err := b.DefinirTituloSprint(ctx, atual.ID, tarefa.ID, "  Entrada da loja 5 "); err != nil {
		t.Fatal(err)
	}
	if titulos, _ := b.TitulosDaSprint(ctx, atual.ID); titulos[tarefa.ID] != "Entrada da loja 5" {
		t.Fatalf("títulos: %v", titulos)
	}
	if err := b.DefinirTituloSprint(ctx, atual.ID, doOutro.ID, "Não"); err != ErrNaoEncontrado {
		t.Fatalf("tarefa de outro perfil: %v", err)
	}

	// Mudar as datas leva a nota da sprint junto (e o título fica).
	if _, err := b.GravarNota(ctx, GravacaoNota{Tarefa: tarefa.ID, Tipo: "sprint", Periodo: atual.ChaveNota(), Texto: "Só atualizar os bancos."}); err != nil {
		t.Fatal(err)
	}
	// Uma nota solta no período novo dá lugar à da sprint.
	if _, err := b.GravarNota(ctx, GravacaoNota{Tarefa: tarefa.ID, Tipo: "sprint", Periodo: "2026-10-01..2026-10-08", Texto: "antiga"}); err != nil {
		t.Fatal(err)
	}
	editada, err := b.EditarSprint(ctx, atual.ID, "2026-10-01", "2026-10-08")
	if err != nil {
		t.Fatal(err)
	}
	if n, _ := b.Nota(ctx, tarefa.ID, "sprint", editada.ChaveNota()); n.Texto != "Só atualizar os bancos." {
		t.Fatalf("a nota não veio junto: %+v", n)
	}
	if n, _ := b.Nota(ctx, tarefa.ID, "sprint", "2026-10-02..2026-10-08"); n.Texto != "" {
		t.Fatalf("ficou nota no período antigo: %+v", n)
	}
	if titulos, _ := b.TitulosDaSprint(ctx, atual.ID); titulos[tarefa.ID] != "Entrada da loja 5" {
		t.Fatalf("o título não ficou: %v", titulos)
	}

	// A próxima segue a editada.
	proxima, err := b.SprintAtual(ctx, p.ID, "2026-10-10")
	if err != nil || proxima.Inicio != "2026-10-09" || proxima.Fim != "2026-10-16" {
		t.Fatalf("próxima: %+v %v", proxima, err)
	}
	lista, _ := b.ListarSprints(ctx, p.ID)
	if len(lista) != 2 || lista[0].ID != atual.ID {
		t.Fatalf("lista: %+v", lista)
	}
	// Remover leva os títulos.
	if err := b.RemoverSprint(ctx, atual.ID); err != nil {
		t.Fatal(err)
	}
	if titulos, _ := b.TitulosDaSprint(ctx, atual.ID); len(titulos) != 0 {
		t.Fatalf("títulos de sprint removida: %v", titulos)
	}
}
