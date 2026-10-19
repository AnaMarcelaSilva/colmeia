package dados

import (
	"context"
	"errors"
	"strings"
	"testing"
)

func TestBaseDeConhecimento(t *testing.T) {
	b, _ := bancoDeTeste(t)
	ctx := context.Background()
	tarefa, agente := tarefaComAgente(t, b)
	_, projeto, perfil, _ := b.Tarefa(ctx, tarefa.ID)
	ws := projeto.WorkspaceID

	// Sem notas, o agente não recebe nada.
	notas, _ := b.ConhecimentoDoProjeto(ctx, projeto.ID)
	if TextoDoConhecimento(notas, projeto.Workspace, projeto.Nome) != "" {
		t.Fatal("sem notas o texto precisa ficar vazio")
	}

	// A sua nasce aprovada; a do workspace vale para o projeto.
	doWorkspace, err := b.CriarConhecimento(ctx, NovoConhecimento{Workspace: ws, Titulo: "Logs", Texto: "As pastas com números são os logs de cada aparelho.", Origem: OrigemVoce})
	if err != nil || doWorkspace.Estado != EstadoAprovada || doWorkspace.ProjetoID != 0 || doWorkspace.WorkspaceID != ws {
		t.Fatalf("nota do workspace: %+v %v", doWorkspace, err)
	}
	// A do agente nasce sugerida e não vai para o agente.
	sugerida, err := b.CriarConhecimento(ctx, NovoConhecimento{Projeto: projeto.ID, Titulo: "Nome do arquivo", Texto: "O nome sai da data e do código da loja.", Origem: OrigemAgente, Agente: agente.ID})
	if err != nil || sugerida.Estado != EstadoSugerida || sugerida.WorkspaceID != ws || sugerida.ProjetoID != projeto.ID {
		t.Fatalf("sugestão: %+v %v", sugerida, err)
	}
	notas, _ = b.ConhecimentoDoProjeto(ctx, projeto.ID)
	if len(notas) != 1 || notas[0].ID != doWorkspace.ID {
		t.Fatalf("só as aprovadas vão ao agente: %+v", notas)
	}
	contagem, _ := b.SugestoesPorWorkspace(ctx, perfil)
	if contagem[ws] != 1 {
		t.Fatalf("sugestões: %v", contagem)
	}
	// O evento não leva o texto.
	var conteudo string
	b.db.QueryRow(`SELECT dados FROM eventos WHERE tipo = 'conhecimento.sugerido'`).Scan(&conteudo)
	if conteudo == "" || strings.Contains(conteudo, "código da loja") {
		t.Fatalf("evento da sugestão: %q", conteudo)
	}

	// Aprovada, entra depois da do workspace.
	if _, err := b.AprovarConhecimento(ctx, sugerida.ID); err != nil {
		t.Fatal(err)
	}
	notas, _ = b.ConhecimentoDoProjeto(ctx, projeto.ID)
	texto := TextoDoConhecimento(notas, projeto.Workspace, projeto.Nome)
	if len(notas) != 2 || strings.Index(texto, "## Logs") > strings.Index(texto, "## Nome do arquivo") {
		t.Fatalf("ordem: %s", texto)
	}

	// A sugestão de mudar uma nota substitui o texto ao ser aprovada e some.
	mudanca, err := b.CriarConhecimento(ctx, NovoConhecimento{Workspace: ws, Titulo: "Logs", Texto: "Logs por aparelho, um por dia.", Origem: OrigemAgente, Substitui: doWorkspace.ID})
	if err != nil || mudanca.Substitui != doWorkspace.ID {
		t.Fatalf("sugestão de mudança: %+v %v", mudanca, err)
	}
	if _, err := b.EditarConhecimento(ctx, mudanca.ID, "Logs", "Logs por aparelho, um arquivo por dia."); err != nil {
		t.Fatal(err)
	}
	lista, _ := b.ListarConhecimento(ctx, ws)
	if len(lista) != 2 || lista[0].ID != doWorkspace.ID || lista[0].Texto != "Logs por aparelho, um arquivo por dia." {
		t.Fatalf("depois da mudança: %+v", lista)
	}

	// Não muda nota de outro lugar nem grava senha.
	if _, err := b.CriarConhecimento(ctx, NovoConhecimento{Projeto: projeto.ID, Titulo: "x", Texto: "y", Origem: OrigemAgente, Substitui: doWorkspace.ID}); err == nil {
		t.Fatal("mudou a nota do workspace como se fosse do projeto")
	}
	var invalido ErrInvalido
	if _, err := b.CriarConhecimento(ctx, NovoConhecimento{Workspace: ws, Titulo: "Acesso", Texto: "senha: Abacaxi#2024!xyz", Origem: OrigemVoce}); !errors.As(err, &invalido) {
		t.Fatalf("senha passou: %v", err)
	}

	// Com a aprovação automática, a do agente já vale.
	auto := true
	if _, err := b.EditarWorkspace(ctx, ws, MudancaWorkspace{ConhecimentoAuto: &auto}); err != nil {
		t.Fatal(err)
	}
	direta, err := b.CriarConhecimento(ctx, NovoConhecimento{Projeto: projeto.ID, Titulo: "Testes", Texto: "Rode os testes com make teste.", Origem: OrigemAgente})
	if err != nil || direta.Estado != EstadoAprovada {
		t.Fatalf("automática: %+v %v", direta, err)
	}

	// Remover o projeto leva as notas dele; as do workspace ficam.
	if err := b.RemoverProjeto(ctx, projeto.ID); err != nil {
		t.Fatal(err)
	}
	lista, _ = b.ListarConhecimento(ctx, ws)
	if len(lista) != 1 || lista[0].ID != doWorkspace.ID {
		t.Fatalf("depois de remover o projeto: %+v", lista)
	}
}

func TestLimitesDaBaseDeConhecimento(t *testing.T) {
	b, _ := bancoDeTeste(t)
	ctx := context.Background()
	tarefa, _ := tarefaComAgente(t, b)
	_, projeto, _, _ := b.Tarefa(ctx, tarefa.ID)

	for i := range MaxSugestoes {
		if _, err := b.CriarConhecimento(ctx, NovoConhecimento{Projeto: projeto.ID, Titulo: "Nota", Texto: strings.Repeat("a", 10+i), Origem: OrigemAgente}); err != nil {
			t.Fatal(err)
		}
	}
	if _, err := b.CriarConhecimento(ctx, NovoConhecimento{Projeto: projeto.ID, Titulo: "Demais", Texto: "b", Origem: OrigemAgente}); err == nil {
		t.Fatal("aceitou sugestões além do limite")
	}

	// O texto inicial respeita o limite e cita pelo título o que não coube.
	var notas []Conhecimento
	for i := range 5 {
		notas = append(notas, Conhecimento{ID: int64(i + 1), Titulo: "Parte", Texto: strings.Repeat("x", 3000)})
	}
	texto := TextoDoConhecimento(notas, "W", "loja-web")
	if len([]rune(texto)) > MaxConhecimentoInicio+1000 || !strings.Contains(texto, "não couberam") || !strings.Contains(texto, "(nota 5)") {
		t.Fatalf("texto com %d caracteres: %s", len([]rune(texto)), texto[len(texto)-300:])
	}
}

func TestEditarWorkspaceEImagem(t *testing.T) {
	b, _ := bancoDeTeste(t)
	ctx := context.Background()
	perfil, _ := b.CriarPerfil(ctx, "Pessoal", "")
	ws, _ := b.CriarWorkspace(ctx, perfil.ID, "Estudos")
	b.CriarWorkspace(ctx, perfil.ID, "Casa")
	projeto, _ := b.CriarProjeto(ctx, ws.ID, "favo", "/tmp/favo", "pasta", "")

	nome := "  Laboratório  "
	editado, err := b.EditarWorkspace(ctx, ws.ID, MudancaWorkspace{Nome: &nome})
	if err != nil || editado.Nome != "Laboratório" {
		t.Fatalf("renomear: %+v %v", editado, err)
	}
	repetido := "Casa"
	if _, err := b.EditarWorkspace(ctx, ws.ID, MudancaWorkspace{Nome: &repetido}); !errors.Is(err, ErrJaExiste) {
		t.Fatalf("nome repetido: %v", err)
	}

	png := []byte("\x89PNG...")
	comImagem, err := b.GravarIconeWorkspace(ctx, ws.ID, png)
	if err != nil || comImagem.Icone == "" {
		t.Fatalf("imagem: %+v %v", comImagem, err)
	}
	p, _ := b.Projeto(ctx, projeto.ID)
	if p.Workspace != "Laboratório" || p.WorkspaceIcone != comImagem.Icone {
		t.Fatalf("o projeto não leva o workspace novo: %+v", p)
	}
	if lido, _ := b.IconeWorkspace(ctx, ws.ID); string(lido) != string(png) {
		t.Fatal("imagem lida diferente")
	}
	semImagem, _ := b.GravarIconeWorkspace(ctx, ws.ID, nil)
	if lido, _ := b.IconeWorkspace(ctx, ws.ID); semImagem.Icone != "" || lido != nil {
		t.Fatal("a imagem ficou")
	}
}
