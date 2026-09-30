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
	projeto, err := b.CriarProjeto(ctx, ws.ID, "loja-web", "/tmp/loja-web", "main")
	if err != nil {
		t.Fatal(err)
	}
	projetos, _ := b.ListarProjetos(ctx, perfil.ID)
	if len(projetos) != 1 || projetos[0].Workspace != "Empresa X" {
		t.Errorf("projetos listados: %+v", projetos)
	}

	primeira, _ := b.CriarTarefa(ctx, projeto.ID, "Nova tela de pedidos", "main")
	segunda, _ := b.CriarTarefa(ctx, projeto.ID, "Corrigir filtro", "main")
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
