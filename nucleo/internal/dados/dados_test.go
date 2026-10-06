package dados

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"path/filepath"
	"strings"
	"testing"

	"github.com/AnaMarcelaSilva/colmeia/nucleo/internal/protecao"
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
	// 0700 e 0600 no Linux; no Windows, lista de acesso só do usuário e do sistema.
	for _, c := range []string{dir, filepath.Join(dir, "colmeia.db")} {
		if so, err := protecao.SoDoUsuario(c); err != nil || !so {
			t.Errorf("%s aberto para outros (%v)", c, err)
		}
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
	movida, err := b.AtualizarTarefa(ctx, primeira.ID, Mudanca{Coluna: &coluna}, OrigemVoce)
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
		`DROP TABLE pedidos`, `DROP TABLE agentes`, `DROP TABLE tarefas`, `ALTER TABLE projetos DROP COLUMN tipo`,
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

func TestEventosLevamOEscopo(t *testing.T) {
	b, _ := bancoDeTeste(t)
	ctx := context.Background()
	var avisados []Evento
	b.AoGravar(func(e []Evento) { avisados = append(avisados, e...) })
	perfil, _ := b.CriarPerfil(ctx, "Profissional", "")
	outro, _ := b.CriarPerfil(ctx, "Pessoal", "")
	ws, _ := b.CriarWorkspace(ctx, perfil.ID, "W")
	projeto, _ := b.CriarProjeto(ctx, ws.ID, "loja-web", "/tmp/loja-web", "git", "main")
	tarefa, _ := b.CriarTarefa(ctx, projeto.ID, "Nova tela de pedidos", "main", "")
	coluna := "concluido"
	b.AtualizarTarefa(ctx, tarefa.ID, Mudanca{Coluna: &coluna}, OrigemVoce)
	b.RemoverTarefa(ctx, tarefa.ID)

	eventos, err := b.ListarEventos(ctx, FiltroEventos{Perfil: perfil.ID})
	if err != nil {
		t.Fatal(err)
	}
	// perfil, workspace, projeto, tarefa criada, atualizada e removida, do mais novo ao mais antigo.
	if len(eventos) != 6 || eventos[0].Tipo != "tarefa.removida" {
		t.Fatalf("eventos do perfil: %+v", eventos)
	}
	removida := eventos[0]
	if removida.Escopo != (Escopo{Perfil: perfil.ID, Projeto: projeto.ID, Tarefa: tarefa.ID}) {
		t.Errorf("escopo da remoção: %+v", removida.Escopo)
	}
	// O título fica no evento: a linha do tempo não depende da tarefa existir.
	var d struct {
		Titulo      string
		ProjetoNome string `json:"projeto_nome"`
	}
	json.Unmarshal(removida.Dados, &d)
	if d.Titulo != "Nova tela de pedidos" || d.ProjetoNome != "loja-web" {
		t.Errorf("dados da remoção: %s", removida.Dados)
	}
	if outros, _ := b.ListarEventos(ctx, FiltroEventos{Perfil: outro.ID}); len(outros) != 1 {
		t.Errorf("o outro perfil vê eventos que não são dele: %+v", outros)
	}
	if len(avisados) != 7 {
		t.Errorf("avisados %d eventos, esperados 7", len(avisados))
	}

	// Uma transação desfeita não avisa nada.
	antes := len(avisados)
	if _, err := b.CriarPerfil(ctx, "Profissional", ""); err == nil {
		t.Fatal("perfil repetido foi aceito")
	}
	erroProposital := errors.New("desfazer")
	if err := b.emTransacao(ctx, func(tx *transacao) error {
		if err := registrar(ctx, tx, "teste", Escopo{Perfil: perfil.ID}, map[string]any{}); err != nil {
			return err
		}
		return erroProposital
	}); !errors.Is(err, erroProposital) {
		t.Fatal(err)
	}
	if len(avisados) != antes {
		t.Error("transação desfeita avisou eventos")
	}
	if err := b.VerificarHistorico(ctx); err != nil {
		t.Errorf("histórico íntegro acusou problema: %v", err)
	}
}

func TestEscopoMudadoPorForaEDetectado(t *testing.T) {
	b, _ := bancoDeTeste(t)
	ctx := context.Background()
	b.CriarPerfil(ctx, "Profissional", "")
	b.CriarPerfil(ctx, "Pessoal", "")
	if _, err := b.db.Exec(`UPDATE eventos SET perfil_id = 2 WHERE id = 1`); err != nil {
		t.Fatal(err)
	}
	if err := b.VerificarHistorico(ctx); err == nil {
		t.Error("um evento mudado de perfil passou despercebido")
	}
}

// Um banco da entrega B: eventos sem escopo, com tarefas e agentes já removidos.
func TestEventosAntigosGanhamOEscopo(t *testing.T) {
	dir := t.TempDir()
	b, err := Abrir(dir)
	if err != nil {
		t.Fatal(err)
	}
	ctx := context.Background()
	perfil, _ := b.CriarPerfil(ctx, "Antigo", "")
	ws, _ := b.CriarWorkspace(ctx, perfil.ID, "W")
	projeto, _ := b.CriarProjeto(ctx, ws.ID, "loja-web", "/tmp/loja-web", "git", "main")
	tarefa, _ := b.CriarTarefa(ctx, projeto.ID, "Antiga", "main", "")
	agente, _ := b.CriarAgente(ctx, tarefa.ID, "shell", "dev", "")
	b.RemoverAgente(ctx, agente.ID)
	b.RemoverTarefa(ctx, tarefa.ID)
	// Refaz o histórico como a entrega B gravava: sem _escopo, sem colunas, versão 0.
	b.db.Exec(`DELETE FROM eventos`)
	anterior := ""
	for _, e := range []struct{ tipo, dados string }{
		{"perfil.criado", `{"id":1,"nome":"Antigo","tema":"escuro","criado_em":"x"}`},
		{"workspace.criado", `{"id":1,"perfil_id":1,"nome":"W"}`},
		{"projeto.criado", `{"id":1,"workspace_id":1,"nome":"loja-web"}`},
		{"tarefa.criada", `{"id":1,"projeto_id":1,"titulo":"Antiga"}`},
		{"agente.criado", `{"id":1,"tarefa_id":1,"ferramenta":"shell"}`},
		{"agente.removido", `{"agente":1}`},
		{"tarefa.atualizada", `{"tarefa":1,"mudanca":{"coluna":"revisao"}}`},
		{"tarefa.removida", `{"tarefa":1}`},
		{"misterio", `{}`},
	} {
		momento := agora()
		hash := hashEvento(anterior, momento, e.tipo, e.dados)
		if _, err := b.db.Exec(`INSERT INTO eventos (momento, tipo, dados, hash_anterior, hash) VALUES (?, ?, ?, ?, ?)`, momento, e.tipo, e.dados, anterior, hash); err != nil {
			t.Fatal(err)
		}
		anterior = hash
	}
	b.db.Exec(`PRAGMA user_version = 0`)
	b.Fechar()

	b, err = Abrir(dir)
	if err != nil {
		t.Fatal(err)
	}
	defer b.Fechar()
	eventos, _ := b.ListarEventos(ctx, FiltroEventos{Perfil: perfil.ID})
	if len(eventos) != 8 {
		t.Fatalf("esperados 8 eventos ligados ao perfil (o desconhecido fica de fora), vieram %d", len(eventos))
	}
	for _, e := range eventos {
		if e.Tipo == "agente.removido" && e.Escopo != (Escopo{Perfil: 1, Projeto: 1, Tarefa: 1, Agente: 1}) {
			t.Errorf("agente removido sem o caminho até o perfil: %+v", e.Escopo)
		}
	}
	if err := b.VerificarHistorico(ctx); err != nil {
		t.Errorf("a migração quebrou o histórico: %v", err)
	}
}

func TestEventosDeUmWorkspace(t *testing.T) {
	b, _ := bancoDeTeste(t)
	ctx := context.Background()
	perfil, _ := b.CriarPerfil(ctx, "Profissional", "")
	estudos, _ := b.CriarWorkspace(ctx, perfil.ID, "estudos")
	trabalho, _ := b.CriarWorkspace(ctx, perfil.ID, "trabalho-x")
	loja, _ := b.CriarProjeto(ctx, estudos.ID, "loja-web", "/tmp/loja-web", "pasta", "")
	cliente, _ := b.CriarProjeto(ctx, estudos.ID, "cliente-x", "/tmp/cliente-x", "pasta", "")
	outraLoja, _ := b.CriarProjeto(ctx, trabalho.ID, "loja-web", "/tmp/outra-loja", "pasta", "")
	for _, p := range []Projeto{loja, cliente, outraLoja} {
		if _, err := b.CriarTarefa(ctx, p.ID, "Tarefa de "+p.Nome, "", ""); err != nil {
			t.Fatal(err)
		}
	}
	eventos, err := b.ListarEventos(ctx, FiltroEventos{Perfil: perfil.ID, Workspace: estudos.ID})
	if err != nil {
		t.Fatal(err)
	}
	// Os projetos e as tarefas do estudos (o workspace em si não tem projeto).
	if len(eventos) != 4 {
		t.Fatalf("eventos do estudos: %+v", eventos)
	}
	for _, e := range eventos {
		if e.Escopo.Projeto != loja.ID && e.Escopo.Projeto != cliente.ID {
			t.Errorf("evento de fora do workspace: %+v", e)
		}
	}
	// Workspace e projeto juntos estreitam; de outro perfil, nada.
	if so, _ := b.ListarEventos(ctx, FiltroEventos{Perfil: perfil.ID, Workspace: trabalho.ID, Projeto: loja.ID}); len(so) != 0 {
		t.Errorf("projeto fora do workspace: %+v", so)
	}
	outro, _ := b.CriarPerfil(ctx, "Pessoal", "")
	if alheios, _ := b.ListarEventos(ctx, FiltroEventos{Perfil: outro.ID, Workspace: estudos.ID}); len(alheios) != 0 {
		t.Errorf("eventos de outro perfil: %+v", alheios)
	}
	// Uma consulta só, pelo índice do perfil (os projetos numa subconsulta).
	var plano []string
	linhas, err := b.db.QueryContext(ctx, `EXPLAIN QUERY PLAN SELECT id FROM eventos WHERE perfil_id = ? AND projeto_id IN (SELECT id FROM projetos WHERE workspace_id = ?) ORDER BY id DESC`, perfil.ID, estudos.ID)
	if err != nil {
		t.Fatal(err)
	}
	defer linhas.Close()
	for linhas.Next() {
		var id, pai, livre int
		var detalhe string
		linhas.Scan(&id, &pai, &livre, &detalhe)
		plano = append(plano, detalhe)
	}
	if junto := strings.Join(plano, " | "); !strings.Contains(junto, "eventos_por_perfil") {
		t.Errorf("a consulta não usa o índice do perfil: %s", junto)
	}
}

func TestPreferenciaDoTempoDosAgentes(t *testing.T) {
	b, _ := bancoDeTeste(t)
	ctx := context.Background()
	perfil, _ := b.CriarPerfil(ctx, "Profissional", "")
	if p, _ := b.Perfil(ctx, perfil.ID); p.TempoAgentes {
		t.Error("o tempo dos agentes começa ligado")
	}
	if err := b.DefinirTempoAgentes(ctx, perfil.ID, true); err != nil {
		t.Fatal(err)
	}
	if p, _ := b.Perfil(ctx, perfil.ID); !p.TempoAgentes {
		t.Error("não ficou ligado")
	}
	if lista, _ := b.ListarPerfis(ctx); len(lista) != 1 || !lista[0].TempoAgentes {
		t.Errorf("perfis: %+v", lista)
	}
	if err := b.DefinirTempoAgentes(ctx, 99, true); !errors.Is(err, ErrNaoEncontrado) {
		t.Errorf("perfil que não existe: %v", err)
	}
	eventos, _ := b.ListarEventos(ctx, FiltroEventos{Perfil: perfil.ID, Tipos: []string{"perfil.tempo_agentes"}})
	if len(eventos) != 1 || !strings.Contains(string(eventos[0].Dados), fmt.Sprintf(`"mostrar":true,"perfil":%d`, perfil.ID)) {
		t.Errorf("evento da preferência: %+v", eventos)
	}
}

func TestRecolherWorkspace(t *testing.T) {
	b, _ := bancoDeTeste(t)
	ctx := context.Background()
	perfil, _ := b.CriarPerfil(ctx, "P", "")
	w, err := b.CriarWorkspace(ctx, perfil.ID, "W")
	if err != nil {
		t.Fatal(err)
	}
	if _, err := b.CriarProjeto(ctx, w.ID, "loja-web", t.TempDir(), "pasta", ""); err != nil {
		t.Fatal(err)
	}
	if err := b.RecolherWorkspace(ctx, w.ID, true); err != nil {
		t.Fatal(err)
	}
	lista, err := b.ListarProjetos(ctx, perfil.ID)
	if err != nil || len(lista) != 1 || !lista[0].WorkspaceRecolhido {
		t.Fatalf("o projeto deveria vir com o workspace recolhido: %+v, %v", lista, err)
	}
	if err := b.RecolherWorkspace(ctx, 99, true); !errors.Is(err, ErrNaoEncontrado) {
		t.Errorf("workspace inexistente: %v", err)
	}
}
