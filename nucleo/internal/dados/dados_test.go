package dados

import (
	"context"
	"errors"
	"os"
	"path/filepath"
	"testing"
)

func bancoDeTeste(t *testing.T) (*Banco, string) {
	t.Helper()
	dir := filepath.Join(t.TempDir(), "dados")
	b, err := Abrir(dir)
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { b.Fechar() })
	return b, dir
}

func TestBancoFechadoParaOutros(t *testing.T) {
	_, dir := bancoDeTeste(t)
	info, _ := os.Stat(dir)
	if info.Mode().Perm() != 0o700 {
		t.Errorf("diretório de dados com permissão %o", info.Mode().Perm())
	}
	info, _ = os.Stat(filepath.Join(dir, "colmeia.db"))
	if info.Mode().Perm()&0o077 != 0 {
		t.Errorf("banco aberto para outros: %o", info.Mode().Perm())
	}
}

func TestCicloCompleto(t *testing.T) {
	b, _ := bancoDeTeste(t)
	ctx := context.Background()

	perfil, err := b.CriarPerfil(ctx, "  Profissional  ", "")
	if err != nil {
		t.Fatal(err)
	}
	if perfil.Nome != "Profissional" || perfil.Tema != "escuro" {
		t.Errorf("perfil criado como %+v", perfil)
	}
	if _, err := b.CriarPerfil(ctx, "Profissional", "claro"); !errors.Is(err, ErrJaExiste) {
		t.Errorf("perfil repetido: esperado ErrJaExiste, veio %v", err)
	}

	if err := b.DefinirContas(ctx, perfil.ID, []Conta{{"claude", "separada"}, {"codex", "sistema"}}); err != nil {
		t.Fatal(err)
	}
	contas, _ := b.ListarContas(ctx, perfil.ID)
	if len(contas) != 2 {
		t.Errorf("esperadas 2 contas, vieram %d", len(contas))
	}

	ws, err := b.CriarWorkspace(ctx, perfil.ID, "Empresa X")
	if err != nil {
		t.Fatal(err)
	}
	projeto, err := b.CriarProjeto(ctx, ws.ID, "loja-web", "/tmp/loja-web", "git", "main")
	if err != nil {
		t.Fatal(err)
	}
	projetos, _ := b.ListarProjetos(ctx, perfil.ID)
	if len(projetos) != 1 || projetos[0].Workspace != "Empresa X" {
		t.Errorf("projetos listados: %+v", projetos)
	}

	primeira, _ := b.CriarTarefa(ctx, projeto.ID, "Nova tela de pedidos", "main", "")
	segunda, _ := b.CriarTarefa(ctx, projeto.ID, "Corrigir filtro", "main", "")
	if segunda.Ordem <= primeira.Ordem {
		t.Error("a tarefa nova deveria ir para o fim do Backlog")
	}
	coluna := "revisao"
	movida, err := b.AtualizarTarefa(ctx, primeira.ID, Mudanca{Coluna: &coluna})
	if err != nil || movida.Coluna != "revisao" {
		t.Fatalf("mover tarefa: %+v, %v", movida, err)
	}
	if err := b.RemoverTarefa(ctx, segunda.ID); err != nil {
		t.Fatal(err)
	}
	tarefas, _ := b.ListarTarefas(ctx, projeto.ID)
	if len(tarefas) != 1 {
		t.Errorf("esperada 1 tarefa, vieram %d", len(tarefas))
	}

	// Remover o projeto leva as tarefas junto, mas nunca toca na pasta.
	if err := b.RemoverProjeto(ctx, projeto.ID); err != nil {
		t.Fatal(err)
	}
	if _, err := b.ListarTarefas(ctx, projeto.ID); !errors.Is(err, ErrNaoEncontrado) {
		t.Errorf("tarefas de projeto removido: %v", err)
	}
	if err := b.VerificarHistorico(ctx); err != nil {
		t.Errorf("histórico íntegro acusou problema: %v", err)
	}
}

func TestValidacoes(t *testing.T) {
	b, _ := bancoDeTeste(t)
	ctx := context.Background()
	casos := []error{}
	_, err := b.CriarPerfil(ctx, "   ", "")
	casos = append(casos, err)
	_, err = b.CriarPerfil(ctx, "linha\nquebrada", "")
	casos = append(casos, err)
	_, err = b.CriarPerfil(ctx, "Tema estranho", "neon")
	casos = append(casos, err)
	perfil, _ := b.CriarPerfil(ctx, "Estudo", "leitura")
	casos = append(casos, b.DefinirContas(ctx, perfil.ID, []Conta{{"ferramenta-inventada", "sistema"}}))
	casos = append(casos, b.DefinirContas(ctx, perfil.ID, []Conta{{"claude", "sistema"}, {"claude", "separada"}}))
	for i, err := range casos {
		var invalido ErrInvalido
		if !errors.As(err, &invalido) {
			t.Errorf("caso %d: esperado ErrInvalido, veio %v", i, err)
		}
	}
}

func TestBranchValida(t *testing.T) {
	for _, ok := range []string{"main", "dev", "feature/pedidos", "hotfix/desconto-1.2", "release_2026"} {
		if err := BranchValida(ok); err != nil {
			t.Errorf("%q deveria ser aceita: %v", ok, err)
		}
	}
	for _, ruim := range []string{"", "-rf", "--upload-pack=x", "a..b", "a//b", "fim/", "x.lock", "com espaço", "ponto.", "../fora"} {
		if err := BranchValida(ruim); err == nil {
			t.Errorf("%q deveria ser recusada", ruim)
		}
	}
}

func TestHistoricoAdulteradoEDetectado(t *testing.T) {
	b, _ := bancoDeTeste(t)
	ctx := context.Background()
	b.CriarPerfil(ctx, "Profissional", "")
	b.CriarPerfil(ctx, "Pessoal", "")
	if _, err := b.db.Exec(`UPDATE eventos SET dados = '{"nome":"Outro"}' WHERE id = 1`); err != nil {
		t.Fatal(err)
	}
	if err := b.VerificarHistorico(ctx); err == nil {
		t.Error("um evento alterado passou despercebido")
	}
}

func TestPastaSemGitEAgentes(t *testing.T) {
	b, _ := bancoDeTeste(t)
	ctx := context.Background()
	perfil, _ := b.CriarPerfil(ctx, "Profissional", "")
	ws, _ := b.CriarWorkspace(ctx, perfil.ID, "Trabalho")
	pasta, err := b.CriarProjeto(ctx, ws.ID, "clientes", "/tmp/clientes", "pasta", "ignorada")
	if err != nil || pasta.BranchPadrao != "" || pasta.Tipo != "pasta" {
		t.Fatalf("pasta sem git: %+v %v", pasta, err)
	}
	if _, err := b.CriarTarefa(ctx, pasta.ID, "Com branch", "main", ""); err == nil {
		t.Error("tarefa com branch numa pasta sem git deveria ser recusada")
	}
	if _, err := b.CriarTarefa(ctx, pasta.ID, "Com cópia", "", "copia"); err == nil {
		t.Error("cópia isolada numa pasta sem git deveria ser recusada")
	}
	tarefa, err := b.CriarTarefa(ctx, pasta.ID, "Analisar problema do cliente", "", "")
	if err != nil || tarefa.Local != "pasta" {
		t.Fatalf("tarefa na pasta: %+v %v", tarefa, err)
	}
	lida, projeto, dono, err := b.Tarefa(ctx, tarefa.ID)
	if err != nil || dono != perfil.ID || lida.Pasta(projeto) != "/tmp/clientes" {
		t.Fatalf("contexto da tarefa: %+v %+v %d %v", lida, projeto, dono, err)
	}

	id := "81701644-dffb-4e29-9873-ea49ec35ec50"
	if _, err := b.CriarAgente(ctx, tarefa.ID, "codex", "dev", id); err == nil {
		t.Error("só o Claude Code retoma conversa")
	}
	if _, err := b.CriarAgente(ctx, tarefa.ID, "claude", "chefe", ""); err == nil {
		t.Error("papel inventado deveria ser recusado")
	}
	if _, err := b.CriarAgente(ctx, 9999, "claude", "dev", ""); !errors.Is(err, ErrNaoEncontrado) {
		t.Errorf("agente de tarefa inexistente: %v", err)
	}
	agente, err := b.CriarAgente(ctx, tarefa.ID, "claude", "dev", id)
	if err != nil {
		t.Fatal(err)
	}
	b.CriarAgente(ctx, tarefa.ID, "shell", "testador", "")
	agentes, _ := b.ListarAgentes(ctx, tarefa.ID)
	if len(agentes) != 2 || agentes[0].ID != agente.ID || agentes[0].Sessao != id {
		t.Fatalf("agentes listados: %+v", agentes)
	}
	// Remover a tarefa leva os agentes junto.
	b.RemoverTarefa(ctx, tarefa.ID)
	if _, err := b.Agente(ctx, agente.ID); !errors.Is(err, ErrNaoEncontrado) {
		t.Errorf("agente de tarefa removida: %v", err)
	}
}

func TestBancoAntigoGanhaAsColunasNovas(t *testing.T) {
	dir := t.TempDir()
	b, err := Abrir(dir)
	if err != nil {
		t.Fatal(err)
	}
	ctx := context.Background()
	perfil, _ := b.CriarPerfil(ctx, "Antigo", "")
	ws, _ := b.CriarWorkspace(ctx, perfil.ID, "W")
	// Volta o banco para como era na primeira versão, com um projeto gravado.
	for _, c := range []string{
		`DROP TABLE agentes`, `DROP TABLE tarefas`, `ALTER TABLE projetos DROP COLUMN tipo`,
		`INSERT INTO projetos (workspace_id, nome, caminho, branch_padrao) VALUES (1, 'velho', '/tmp/velho', 'main')`,
	} {
		if _, err := b.db.Exec(c); err != nil {
			t.Fatalf("%s: %v", c, err)
		}
	}
	b.Fechar()
	b, err = Abrir(dir)
	if err != nil {
		t.Fatal(err)
	}
	defer b.Fechar()
	projetos, err := b.ListarProjetos(ctx, perfil.ID)
	if err != nil || len(projetos) != 1 || projetos[0].Tipo != "git" || projetos[0].WorkspaceID != ws.ID {
		t.Fatalf("projeto antigo: %+v %v", projetos, err)
	}
	if _, err := b.CriarTarefa(ctx, projetos[0].ID, "Depois da atualização", "main", ""); err != nil {
		t.Fatal(err)
	}
}
